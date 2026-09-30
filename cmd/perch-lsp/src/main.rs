//! `perch-lsp`: a Language Server Protocol implementation for `.perch` files.
//! JSON-RPC 2.0 over stdio with `Content-Length:` framing. Port of the Go
//! `cmd/perch-lsp/main.go`; wire output is kept byte-compatible (Go's key
//! ordering and HTML-escaping included).
//!
//! Capabilities: didOpen/didChange/didClose, publishDiagnostics,
//! completion, hover, documentSymbol.

mod docs;

use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, Write};

use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Map, Value};

// ── Framing ────────────────────────────────────────────────────────────

enum ReadErr {
    Eof,
    Other(String),
}

/// Reads one LSP message payload; headers are consumed and dropped.
fn read_framed<R: BufRead>(r: &mut R) -> Result<Vec<u8>, ReadErr> {
    let mut content_length: i64 = -1;
    loop {
        let mut raw = Vec::new();
        match r.read_until(b'\n', &mut raw) {
            Ok(0) => return Err(ReadErr::Eof),
            Ok(_) => {}
            Err(e) => return Err(ReadErr::Other(e.to_string())),
        }
        if raw.last() != Some(&b'\n') {
            // Go's ReadString returns io.EOF with partial data.
            return Err(ReadErr::Eof);
        }
        let line = String::from_utf8_lossy(&raw);
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if line.to_lowercase().starts_with("content-length:") {
            let v = line["Content-Length:".len()..].trim();
            match v.parse::<i64>() {
                Ok(n) => content_length = n,
                Err(_) => {
                    return Err(ReadErr::Other(format!(
                        "bad content-length: strconv.Atoi: parsing {:?}: invalid syntax",
                        v
                    )))
                }
            }
        }
    }
    if content_length < 0 {
        return Err(ReadErr::Other("missing Content-Length header".into()));
    }
    let mut buf = vec![0u8; content_length as usize];
    if let Err(e) = r.read_exact(&mut buf) {
        return Err(if e.kind() == io::ErrorKind::UnexpectedEof {
            ReadErr::Other("unexpected EOF".into())
        } else {
            ReadErr::Other(e.to_string())
        });
    }
    Ok(buf)
}

/// Serializes like Go's `json.Marshal`: `<`, `>`, `&`, U+2028, U+2029 are
/// escaped (they can only occur inside strings in serde's output).
fn go_marshal(v: &Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_else(|_| "null".into());
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out
}

fn write_framed<W: Write>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    write!(w, "Content-Length: {}\r\n\r\n", payload.len())?;
    w.write_all(payload)?;
    w.flush()
}

// ── Server ─────────────────────────────────────────────────────────────

struct Server<W: Write> {
    docs: HashMap<String, String>,
    out: W,
    known: HashSet<String>,
    re: Res,
}

struct Res {
    command_header: Regex,
    arg_header: Regex,
    ident: Regex,
    line_kw: Regex,
    line_col: Regex,
    openers: Regex,
}

impl Res {
    fn new() -> Self {
        Res {
            command_header: Regex::new(r"(?m)^\s*command\s+([A-Za-z_][A-Za-z0-9_]*)\s*$").unwrap(),
            arg_header: Regex::new(r"(?m)^\s*arg\s+([A-Za-z_][A-Za-z0-9_]*)\s*$").unwrap(),
            ident: Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").unwrap(),
            line_kw: Regex::new(r"line\s+(\d+)").unwrap(),
            line_col: Regex::new(r"(\d+):\d+:").unwrap(),
            openers: Regex::new(r"^\s*(command|catch|globals|arg|do|if|for_each)\b").unwrap(),
        }
    }
}

#[derive(Deserialize, Default, Clone, Copy)]
#[serde(default)]
struct Position {
    line: i64,
    character: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TextDocId {
    uri: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DidOpenParams {
    #[serde(rename = "textDocument")]
    text_document: DidOpenDoc,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct DidOpenDoc {
    uri: String,
    text: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DidChangeParams {
    #[serde(rename = "textDocument")]
    text_document: TextDocId,
    #[serde(rename = "contentChanges")]
    content_changes: Vec<Change>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct Change {
    text: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DocParams {
    #[serde(rename = "textDocument")]
    text_document: TextDocId,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DocPosParams {
    #[serde(rename = "textDocument")]
    text_document: TextDocId,
    position: Position,
}

/// Go marshals structs with fixed field order; objects here are built with
/// `Map` (preserve_order) in that order. Go `map[string]any` sorts keys, so
/// those call sites insert keys alphabetically.
fn obj(pairs: Vec<(&str, Value)>) -> Value {
    let mut m = Map::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v);
    }
    Value::Object(m)
}

fn full_line_range(line: i64) -> Value {
    range(line, 0, line + 1, 0)
}

fn range(sl: i64, sc: i64, el: i64, ec: i64) -> Value {
    json!({"start": {"line": sl, "character": sc}, "end": {"line": el, "character": ec}})
}

impl<W: Write> Server<W> {
    fn handle(&mut self, msg: &Map<String, Value>, method: &str) {
        let id = msg.get("id");
        let params = msg.get("params");
        match method {
            "initialize" => self.respond(
                id,
                Some(json!({
                    "capabilities": {
                        "completionProvider": {"triggerCharacters": [" ", "$", "{"]},
                        "documentSymbolProvider": true,
                        "hoverProvider": true,
                        "textDocumentSync": 1
                    },
                    "serverInfo": {"name": "perch-lsp", "version": "0.1.0"}
                })),
            ),
            "initialized" | "$/setTrace" => {}
            "shutdown" => self.respond(id, None),
            "exit" => {
                let _ = self.out.flush();
                std::process::exit(0);
            }
            "textDocument/didOpen" => {
                if let Some(p) = parse::<DidOpenParams>(params) {
                    let (uri, text) = (p.text_document.uri, p.text_document.text);
                    self.docs.insert(uri.clone(), text.clone());
                    self.publish_diagnostics(&uri, &text);
                }
            }
            "textDocument/didChange" => {
                if let Some(p) = parse::<DidChangeParams>(params) {
                    if let Some(last) = p.content_changes.last() {
                        let uri = p.text_document.uri;
                        let text = last.text.clone();
                        self.docs.insert(uri.clone(), text.clone());
                        self.publish_diagnostics(&uri, &text);
                    }
                }
            }
            "textDocument/didClose" => {
                if let Some(p) = parse::<DocParams>(params) {
                    let uri = p.text_document.uri;
                    self.docs.remove(&uri);
                    self.notify(
                        "textDocument/publishDiagnostics",
                        json!({"diagnostics": [], "uri": uri}),
                    );
                }
            }
            "textDocument/didSave" => {}
            "textDocument/completion" => match parse::<DocPosParams>(params) {
                Some(p) => {
                    let text = self.doc(&p.text_document.uri);
                    let scope = detect_scope(&text, p.position.line);
                    let items = self.completions_for(&text, scope);
                    self.respond(id, Some(items));
                }
                None => self.respond_err(id, -32602, invalid_params_msg(params)),
            },
            "textDocument/hover" => match parse::<DocPosParams>(params) {
                Some(p) => {
                    let text = self.doc(&p.text_document.uri);
                    let word = word_at(&self.re.ident, &text, p.position);
                    let doc = if word.is_empty() { String::new() } else { hover_doc(&word) };
                    if doc.is_empty() {
                        self.respond(id, None);
                    } else {
                        self.respond(id, Some(json!({"contents": {"kind": "markdown", "value": doc}})));
                    }
                }
                None => self.respond_err(id, -32602, invalid_params_msg(params)),
            },
            "textDocument/documentSymbol" => match parse::<DocParams>(params) {
                Some(p) => {
                    let text = self.doc(&p.text_document.uri);
                    let syms = self.document_symbols(&text);
                    self.respond(id, Some(syms));
                }
                None => self.respond_err(id, -32602, invalid_params_msg(params)),
            },
            _ => {
                if let Some(id) = id {
                    self.respond_err(Some(id), -32601, &format!("method not found: {method}"));
                }
            }
        }
    }

    fn doc(&self, uri: &str) -> String {
        self.docs.get(uri).cloned().unwrap_or_default()
    }

    fn send(&mut self, m: Value) {
        let b = go_marshal(&m);
        let _ = write_framed(&mut self.out, b.as_bytes());
    }

    /// Go's `Result any` is `omitempty`: a nil result is omitted entirely.
    fn respond(&mut self, id: Option<&Value>, result: Option<Value>) {
        let Some(id) = id else { return };
        let mut pairs = vec![("jsonrpc", json!("2.0")), ("id", id.clone())];
        if let Some(r) = result {
            pairs.push(("result", r));
        }
        self.send(obj(pairs));
    }

    fn respond_err(&mut self, id: Option<&Value>, code: i64, msg: &str) {
        let Some(id) = id else { return };
        self.send(obj(vec![
            ("jsonrpc", json!("2.0")),
            ("id", id.clone()),
            ("error", json!({"code": code, "message": msg})),
        ]));
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(obj(vec![
            ("jsonrpc", json!("2.0")),
            ("method", json!(method)),
            ("params", params),
        ]));
    }

    // ── Diagnostics ──

    fn publish_diagnostics(&mut self, uri: &str, text: &str) {
        let mut diags: Vec<Value> = Vec::new();
        match perch_capyloader::load_from_string(text) {
            Err(e) => {
                let msg = e.to_string();
                let line = self.extract_line(&msg);
                diags.push(json!({
                    "range": full_line_range(line),
                    "severity": 1,
                    "source": "perch (parse)",
                    "message": msg,
                }));
            }
            Ok(prog) => {
                let command_line = self.scan_command_lines(text);
                for is in perch_validate::check(&prog, &self.known) {
                    let ln = command_line_from_where(&is.where_, &command_line);
                    let sev = if is.severity == "warning" { 2 } else { 1 };
                    diags.push(json!({
                        "range": full_line_range(ln),
                        "severity": sev,
                        "source": "perch (--check)",
                        "message": is.message,
                    }));
                }
            }
        }
        self.notify(
            "textDocument/publishDiagnostics",
            json!({"diagnostics": diags, "uri": uri}),
        );
    }

    /// 0-indexed line from `line N` or `N:COL:` in an error string.
    fn extract_line(&self, s: &str) -> i64 {
        for re in [&self.re.line_kw, &self.re.line_col] {
            if let Some(m) = re.captures(s) {
                if let Ok(n) = m[1].parse::<i64>() {
                    if n > 0 {
                        return n - 1;
                    }
                }
            }
        }
        0
    }

    fn scan_command_lines(&self, text: &str) -> HashMap<String, i64> {
        let mut out = HashMap::new();
        for (i, line) in text.split('\n').enumerate() {
            if let Some(m) = self.re.command_header.captures(line) {
                out.entry(m[1].to_string()).or_insert(i as i64);
            }
        }
        out
    }

    // ── Completion ──

    fn completions_for(&self, text: &str, s: Scope) -> Value {
        let items = match s {
            Scope::Top => static_items(
                &[
                    ("name", "Program name shown in --help."),
                    ("about", "Top-level program description."),
                    ("version", "Version string returned by --version."),
                    ("globals", "Block of bindings shared by every command (`NAME = LITERAL`)."),
                    ("command", "Declare a callable command. Opens a block terminated by `end`."),
                    ("catch", "Fallback handler for unknown command names."),
                ],
                14,
            ),
            Scope::Globals => vec![item("end", 14, "close globals block", "")],
            Scope::Command => static_items(
                &[
                    ("description", "Help text shown in `--help`."),
                    ("arg", "Declare a typed CLI argument. Opens a block terminated by `end`."),
                    ("private", "Hide from CLI; callable only via `run`."),
                    ("detached", "Don't wait on processes started by `shell_detached`."),
                    ("proxy_args", "Skip arg parsing; argv → ${proxy_args}."),
                    ("require_os", "Refuse to run on other OSes. Pass strings: \"darwin\" \"linux\" …"),
                    ("require_arch", "Refuse to run on other archs."),
                    ("dir", "Set the cwd for the body."),
                    ("on_signal", "Run HANDLER on SIGINT/SIGTERM."),
                    ("env", "Set an env var for the body's shell calls."),
                    ("do", "Open the executable body block."),
                    ("end", "Close the command block."),
                ],
                14,
            ),
            Scope::CommandArg => static_items(
                &[
                    ("type", "string / int / float / bool (required)."),
                    ("default", "Default literal value; presence makes the arg optional."),
                    ("description", "Help text shown in --help."),
                    ("optional", "Mark optional even without a default."),
                    ("index", "Bind to a positional index (instead of -name flag)."),
                    ("end", "Close the arg block."),
                ],
                14,
            ),
            Scope::DoBody => {
                let mut items = Vec::new();
                for (name, detail, doc) in docs::OP_DOCS {
                    items.push(item(name, 3, detail, doc));
                }
                items.push(item("end", 14, "close do block", ""));
                items.push(item(
                    "finally",
                    14,
                    "finally — cleanup that always runs (`do … finally … end` or inside `try`)",
                    "",
                ));
                items.push(item(
                    "NAME=value",
                    14,
                    "inline env prefix: NAME=value binary verb --args (bins / exec only)",
                    "",
                ));
                items.push(item(
                    "if",
                    14,
                    "if EXPR ... end — comparison / predicate / truthy / falsy",
                    "",
                ));
                let mut names: Vec<String> = self
                    .re
                    .command_header
                    .captures_iter(text)
                    .map(|m| m[1].to_string())
                    .collect();
                names.sort();
                for n in names {
                    items.push(item(&n, 22, "command in this file", ""));
                }
                items
            }
        };
        Value::Array(items)
    }

    // ── documentSymbol ──

    fn document_symbols(&self, text: &str) -> Value {
        let lines: Vec<&str> = text.split('\n').collect();
        let mut symbols = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            let t = l.trim();
            if t.starts_with("command ") {
                let name = t["command".len()..].trim();
                let end_line = self.find_matching_end(&lines, i);
                let mut children = Vec::new();
                for j in (i + 1)..end_line {
                    if let Some(m) = self.re.arg_header.captures(lines[j]) {
                        let arg_end = self.find_matching_end(&lines, j);
                        children.push(obj(vec![
                            ("name", json!(&m[1])),
                            ("detail", json!("arg")),
                            ("kind", json!(13)),
                            ("range", range(j as i64, 0, arg_end as i64 + 1, 0)),
                            ("selectionRange", full_line_range(j as i64)),
                        ]));
                    }
                }
                let mut pairs = vec![
                    ("name", json!(name)),
                    ("kind", json!(12)),
                    ("range", range(i as i64, 0, end_line as i64 + 1, 0)),
                    ("selectionRange", full_line_range(i as i64)),
                ];
                if !children.is_empty() {
                    pairs.push(("children", Value::Array(children)));
                }
                symbols.push(obj(pairs));
            }
        }
        Value::Array(symbols)
    }

    fn find_matching_end(&self, lines: &[&str], start: usize) -> usize {
        let mut depth = 0i64;
        for (i, l) in lines.iter().enumerate().skip(start) {
            let t = l.trim();
            if self.re.openers.is_match(t) {
                depth += 1;
            }
            if t == "end" {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
        }
        lines.len() - 1
    }
}

fn parse<T: for<'de> Deserialize<'de>>(p: Option<&Value>) -> Option<T> {
    match p {
        None => None,
        Some(v) => serde_json::from_value(v.clone()).ok(),
    }
}

/// Go's message for missing params; type errors get a generic text (Go's
/// `json.UnmarshalTypeError` wording is not reproduced).
fn invalid_params_msg(p: Option<&Value>) -> &'static str {
    if p.is_none() {
        "unexpected end of JSON input"
    } else {
        "invalid params"
    }
}

fn command_line_from_where(where_: &str, command_line: &HashMap<String, i64>) -> i64 {
    let name = match where_.find(' ') {
        Some(i) => &where_[..i],
        None => where_,
    };
    command_line.get(name).copied().unwrap_or(0)
}

fn item(label: &str, kind: i64, detail: &str, doc: &str) -> Value {
    let mut pairs = vec![("label", json!(label)), ("kind", json!(kind))];
    if !detail.is_empty() {
        pairs.push(("detail", json!(detail)));
    }
    if !doc.is_empty() {
        pairs.push(("documentation", json!(doc)));
    }
    obj(pairs)
}

fn static_items(kws: &[(&str, &str)], kind: i64) -> Vec<Value> {
    kws.iter().map(|(n, d)| item(n, kind, d, "")).collect()
}

#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Top,
    Globals,
    Command,
    CommandArg,
    DoBody,
}

fn detect_scope(text: &str, line: i64) -> Scope {
    let lines: Vec<&str> = text.split('\n').collect();
    let line = if line >= lines.len() as i64 { lines.len() as i64 - 1 } else { line };
    let mut cur = Scope::Top;
    let mut stack: Vec<Scope> = Vec::new();
    let mut i = 0i64;
    while i <= line && (i as usize) < lines.len() {
        let l = lines[i as usize].trim();
        i += 1;
        if l.is_empty() || l.starts_with('#') {
            continue;
        } else if l.starts_with("globals") {
            stack.push(cur);
            cur = Scope::Globals;
        } else if l.starts_with("command ") || l.starts_with("catch ") {
            stack.push(cur);
            cur = Scope::Command;
        } else if l.starts_with("arg ") && cur == Scope::Command {
            stack.push(cur);
            cur = Scope::CommandArg;
        } else if l.starts_with("do") {
            stack.push(cur);
            cur = Scope::DoBody;
        } else if l == "end" {
            if let Some(p) = stack.pop() {
                cur = p;
            }
        }
    }
    cur
}

fn word_at(ident: &Regex, text: &str, pos: Position) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    if pos.line < 0 || pos.line >= lines.len() as i64 {
        return String::new();
    }
    let line = lines[pos.line as usize];
    let ch = pos.character.min(line.len() as i64);
    for m in ident.find_iter(line) {
        if ch >= m.start() as i64 && ch <= m.end() as i64 {
            return m.as_str().to_string();
        }
    }
    String::new()
}

fn hover_doc(word: &str) -> String {
    if let Some((_, detail, doc)) = docs::OP_DOCS.iter().find(|(n, _, _)| *n == word) {
        return format!("**op** `{word}`\n\n{detail}\n\n{doc}");
    }
    if let Some((_, kw)) = docs::KEYWORD_DOCS.iter().find(|(n, _)| *n == word) {
        return format!("**keyword** `{word}`\n\n{kw}");
    }
    String::new()
}

fn main() {
    let known: HashSet<String> = perch_ops::all_handlers().into_keys().collect();
    let stdout = io::stdout();
    let mut s = Server { docs: HashMap::new(), out: stdout.lock(), known, re: Res::new() };
    let stdin = io::stdin();
    let mut r = io::BufReader::new(stdin.lock());
    loop {
        let raw = match read_framed(&mut r) {
            Ok(b) => b,
            Err(ReadErr::Eof) => return,
            Err(ReadErr::Other(e)) => {
                eprintln!("perch-lsp: read: {e}");
                return;
            }
        };
        let Ok(Value::Object(msg)) = serde_json::from_slice::<Value>(&raw) else { continue };
        let method = match msg.get("method") {
            None | Some(Value::Null) => "",
            Some(Value::String(m)) => m.as_str(),
            Some(_) => continue,
        };
        let method = method.to_string();
        s.handle(&msg, &method);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server<Vec<u8>> {
        Server { docs: HashMap::new(), out: Vec::new(), known: HashSet::new(), re: Res::new() }
    }

    #[test]
    fn extract_line_shapes() {
        let s = server();
        assert_eq!(s.extract_line("line 4: unterminated string"), 3);
        assert_eq!(s.extract_line("parse script: 11:1: x"), 10);
        assert_eq!(s.extract_line("nothing"), 0);
    }

    #[test]
    fn scopes() {
        let t = "command a\n  arg x\n    type int\n  end\n  do\n    print \"x\"\n  end\nend\n";
        assert!(detect_scope(t, 0) == Scope::Command);
        assert!(detect_scope(t, 2) == Scope::CommandArg);
        assert!(detect_scope(t, 5) == Scope::DoBody);
        assert!(detect_scope(t, 7) == Scope::Top);
    }

    #[test]
    fn hover_and_word() {
        let re = Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").unwrap();
        let w = word_at(&re, "  print \"x\"", Position { line: 0, character: 4 });
        assert_eq!(w, "print");
        assert!(hover_doc("print").starts_with("**op** `print`"));
        assert!(hover_doc("command").starts_with("**keyword**"));
        assert!(hover_doc("finally").contains("always runs"));
        assert_eq!(hover_doc("zzz"), "");
    }

    #[test]
    fn framing_roundtrip_and_escape() {
        let mut out = Vec::new();
        write_framed(&mut out, go_marshal(&json!({"a": "<&>"})).as_bytes()).unwrap();
        let s = String::from_utf8(out.clone()).unwrap();
        assert!(s.contains("\\u003c\\u0026\\u003e"));
        let mut r = io::BufReader::new(&out[..]);
        assert!(read_framed(&mut r).is_ok());
    }

    #[test]
    fn shutdown_omits_result() {
        let mut s = server();
        s.respond(Some(&json!(1)), None);
        let o = String::from_utf8(s.out).unwrap();
        assert!(o.ends_with("{\"jsonrpc\":\"2.0\",\"id\":1}"));
    }
}
