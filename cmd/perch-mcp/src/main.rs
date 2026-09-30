//! MCP server for perch (JSON-RPC 2.0 over stdio).
//!
//! Exposes two tools so AI agents can discover and run commands from a
//! `commands.perch`:
//!
//! * `perch_list` — lists the public commands with their args/types/descriptions.
//! * `perch_run`  — runs a named command; stdout/stderr come back as the tool
//!   result, and (when the client sends `_meta.progressToken`) are also
//!   streamed as `notifications/progress`.
//!
//! Client config: `{"command":"perch-mcp","args":["-f","/abs/path/commands.perch"]}`.
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use perch_interpreter::{Handler, Interpreter, SharedWriter};
use serde_json::{json, Map, Value};

const PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_NAME: &str = "perch-mcp";
const SERVER_VERSION: &str = "0.1.0";

/// Serializes every write to the JSON-RPC stream: progress notifications from
/// `parallel` blocks run concurrently with the main loop's responses.
static ENC_MU: Mutex<()> = Mutex::new(());

/// Encodes like Go's `json.Encoder` (HTML-escaping on) plus a trailing newline.
fn encode_line(v: &Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    // `<`, `>`, `&`, U+2028/2029 can only occur inside JSON strings.
    let mut out = String::with_capacity(s.len() + 1);
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
    out.push('\n');
    out
}

fn emit(v: &Value) {
    let _g = ENC_MU.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(encode_line(v).as_bytes());
    let _ = out.flush();
}

/// Recursively sorts object keys (Go encodes `map[string]any` sorted).
fn sorted(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut e: Vec<(String, Value)> = m.into_iter().collect();
            e.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(e.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
        v => v,
    }
}

type Id = Option<Value>;

fn envelope(id: &Id) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("jsonrpc".into(), json!("2.0"));
    if let Some(id) = id {
        m.insert("id".into(), id.clone());
    }
    m
}

fn respond(id: &Id, result: Value) {
    let mut m = envelope(id);
    m.insert("result".into(), sorted(result));
    emit(&Value::Object(m));
}

fn respond_err(id: &Id, code: i64, msg: &str) {
    let mut m = envelope(id);
    m.insert("error".into(), json!({"code": code, "message": msg}));
    emit(&Value::Object(m));
}

fn respond_tool_result(id: &Id, text: &str, is_error: bool) {
    respond(id, json!({"content": [{"type": "text", "text": text}], "isError": is_error}));
}

/// An output sink that accumulates everything written and, when the client
/// supplied a progress token, also emits `notifications/progress`.
struct ProgressWriter {
    token: Option<Value>,
    stream: &'static str,
    n: AtomicU64,
    acc: Mutex<Vec<u8>>,
}

impl ProgressWriter {
    fn new(token: Option<Value>, stream: &'static str) -> Arc<Self> {
        Arc::new(ProgressWriter { token, stream, n: AtomicU64::new(0), acc: Mutex::new(Vec::new()) })
    }

    fn write(&self, b: &[u8]) {
        self.acc.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(b);
        let Some(token) = &self.token else { return };
        let n = self.n.fetch_add(1, Ordering::SeqCst) + 1;
        let message = String::from_utf8_lossy(b).trim_end_matches('\n').to_string();
        let mut params = Map::new();
        params.insert("_meta".into(), json!({"stream": self.stream}));
        params.insert("message".into(), Value::String(message));
        params.insert("progress".into(), json!(n));
        params.insert("progressToken".into(), token.clone());
        let mut m = Map::new();
        m.insert("jsonrpc".into(), json!("2.0"));
        m.insert("method".into(), json!("notifications/progress"));
        m.insert("params".into(), Value::Object(params));
        emit(&Value::Object(m));
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.acc.lock().unwrap_or_else(|e| e.into_inner())).into_owned()
    }

    fn len(&self) -> usize {
        self.acc.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

struct PwSink(Arc<ProgressWriter>);

impl Write for PwSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "perch_list",
                "description": "List all callable commands in the current commands.perch, with their args, types, and descriptions.",
                "inputSchema": {"type": "object", "properties": {}},
            },
            {
                "name": "perch_run",
                "description": "Run a named perch command. Returns combined stdout/stderr output.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "Name of the command to run (from commands.perch)"},
                        "args": {"type": "object", "description": "Named arguments to pass to the command. Keys are arg names; values are strings/numbers/bools."},
                    },
                    "required": ["command"],
                },
            },
        ]
    })
}

/// Go's `%v` for a decoded JSON value.
fn go_v(v: &Value) -> String {
    match v {
        Value::Null => "<nil>".to_string(),
        v => perch_interpreter::to_string_value(v),
    }
}

/// Approximates Go's `encoding/json` error text for a failed decode.
fn json_err_text(raw: &str, e: &serde_json::Error) -> String {
    if raw.trim().is_empty() || e.is_eof() {
        return "unexpected end of JSON input".to_string();
    }
    if e.is_syntax() {
        let (line, col) = (e.line(), e.column());
        let ch = raw.lines().nth(line.saturating_sub(1)).and_then(|l| l.chars().nth(col.saturating_sub(1)));
        if let Some(c) = ch {
            return format!("invalid character '{c}' looking for beginning of value");
        }
    }
    e.to_string()
}

fn handle_perch_list(id: &Id, cfg: &str) {
    let p = match perch_capyloader::load(cfg) {
        Ok(p) => p,
        Err(e) => return respond_tool_result(id, &format!("Error loading {cfg}: {e}"), true),
    };
    let mut b = String::new();
    let _ = writeln!(b, "perch program: {} (v{})", p.name, p.version);
    if !p.description.is_empty() {
        let _ = write!(b, "{}\n\n", p.description);
    }
    // BTreeMap iterates in sorted (byte) order, same as sort.Strings.
    for (k, c) in p.commands.iter().filter(|(_, c)| !c.modifiers.private) {
        let _ = writeln!(b, "• {} — {}", k, c.description);
        for a in &c.args {
            let def = if a.has_default { format!(" (default {})", go_v(&a.default)) } else { String::new() };
            let _ = writeln!(b, "    -{} {}{} — {}", a.name, a.ty, def, a.description);
        }
    }
    respond_tool_result(id, &b, false);
}

fn handle_perch_run(
    id: &Id,
    cfg: &str,
    handlers: &HashMap<String, Handler>,
    raw: Option<&str>,
    token: Option<Value>,
) {
    let raw = raw.unwrap_or("");
    let parsed: Result<Value, _> = serde_json::from_str(raw);
    let v = match parsed {
        Ok(v) => v,
        Err(e) => return respond_err(id, -32602, &format!("invalid arguments: {}", json_err_text(raw, &e))),
    };
    let (command, args) = match &v {
        Value::Null => (String::new(), Map::new()),
        Value::Object(m) => {
            let command = match m.get("command") {
                None | Some(Value::Null) => String::new(),
                Some(Value::String(s)) => s.clone(),
                Some(other) => {
                    return respond_err(
                        id,
                        -32602,
                        &format!("invalid arguments: json: cannot unmarshal {} into Go struct field .command of type string", kind_of(other)),
                    )
                }
            };
            let args = match m.get("args") {
                None | Some(Value::Null) => Map::new(),
                Some(Value::Object(a)) => a.clone(),
                Some(other) => {
                    return respond_err(
                        id,
                        -32602,
                        &format!("invalid arguments: json: cannot unmarshal {} into Go struct field .args of type map[string]interface {{}}", kind_of(other)),
                    )
                }
            };
            (command, args)
        }
        other => {
            return respond_err(
                id,
                -32602,
                &format!("invalid arguments: json: cannot unmarshal {} into Go value of type struct {{ Command string \"json:\\\"command\\\"\"; Args map[string]interface {{}} \"json:\\\"args\\\"\" }}", kind_of(other)),
            )
        }
    };
    if command.is_empty() {
        return respond_tool_result(id, "missing 'command' argument", true);
    }
    let p = match perch_capyloader::load(cfg) {
        Ok(p) => p,
        Err(e) => return respond_tool_result(id, &format!("Error loading {cfg}: {e}"), true),
    };
    let known_public = p.commands.get(&command).is_some_and(|c| !c.modifiers.private);
    if !known_public && p.catch.is_none() {
        return respond_tool_result(id, &format!("Unknown command: {}", perch_interpreter::go_quote(&command)), true);
    }

    // One writer per stream: independent buffers plus optional live progress.
    let stdout = ProgressWriter::new(token.clone(), "stdout");
    let stderr = ProgressWriter::new(token, "stderr");

    let mut i = Interpreter::new(handlers.clone(), p);
    i.preflight_hook = Some(Arc::new(perch_ops::preflight));
    i.stdout = SharedWriter::new(Box::new(PwSink(stdout.clone())));
    i.stderr = SharedWriter::new(Box::new(PwSink(stderr.clone())));

    let argv: Vec<String> = args.iter().map(|(k, v)| format!("-{}={}", k, go_v(v))).collect();
    let run_err = i.run(&command, &argv).err();

    let mut out = stdout.text();
    if stderr.len() > 0 {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("--- stderr ---\n");
        out.push_str(&stderr.text());
    }
    let is_err = run_err.is_some();
    if let Some(e) = run_err {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("--- error ---\n");
        out.push_str(&e.to_string());
    }
    respond_tool_result(id, out.trim_end_matches('\n'), is_err);
}

fn kind_of(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn handle(method: &str, id: &Id, params: Option<&str>, cfg: &str, handlers: &HashMap<String, Handler>) {
    match method {
        "initialize" => respond(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                "capabilities": {"tools": {}},
            }),
        ),
        "notifications/initialized" => {}
        "tools/list" => respond(id, tools_list()),
        "tools/call" => {
            let raw = params.unwrap_or("");
            let v: Value = match serde_json::from_str(raw) {
                Ok(v) => v,
                Err(e) => return respond_err(id, -32602, &format!("invalid params: {}", json_err_text(raw, &e))),
            };
            let (name, arguments, token) = match &v {
                Value::Null => (String::new(), None, None),
                Value::Object(m) => {
                    let name = match m.get("name") {
                        None | Some(Value::Null) => String::new(),
                        Some(Value::String(s)) => s.clone(),
                        Some(o) => {
                            return respond_err(
                                id,
                                -32602,
                                &format!("invalid params: json: cannot unmarshal {} into Go struct field .name of type string", kind_of(o)),
                            )
                        }
                    };
                    let arguments = m.get("arguments").map(|a| serde_json::to_string(a).unwrap_or_default());
                    let token = m.get("_meta").and_then(|m| m.get("progressToken")).cloned();
                    (name, arguments, token)
                }
                o => {
                    return respond_err(
                        id,
                        -32602,
                        &format!("invalid params: json: cannot unmarshal {} into Go value of type struct", kind_of(o)),
                    )
                }
            };
            match name.as_str() {
                "perch_list" => handle_perch_list(id, cfg),
                "perch_run" => handle_perch_run(id, cfg, handlers, arguments.as_deref(), token),
                _ => respond_err(id, -32601, &format!("unknown tool: {name}")),
            }
        }
        _ => {
            // Unknown notifications: silent. Unknown requests: error.
            if id.is_some() {
                respond_err(id, -32601, &format!("method not found: {method}"));
            }
        }
    }
}

/// Parses one request line, replying with a parse error when malformed.
fn process_line(line: &str, cfg: &str, handlers: &HashMap<String, Handler>) {
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return respond_err(&None, -32700, &format!("parse error: {}", json_err_text(line, &e))),
    };
    let Value::Object(m) = &v else {
        return respond_err(
            &None,
            -32700,
            &format!("parse error: json: cannot unmarshal {} into Go value of type main.rpcReq", kind_of(&v)),
        );
    };
    let method = match m.get("method") {
        None | Some(Value::Null) => "",
        Some(Value::String(s)) => s.as_str(),
        Some(o) => {
            return respond_err(
                &None,
                -32700,
                &format!("parse error: json: cannot unmarshal {} into Go struct field rpcReq.method of type string", kind_of(o)),
            )
        }
    };
    let id: Id = m.get("id").cloned();
    let params = m.get("params").map(|p| serde_json::to_string(p).unwrap_or_default());
    handle(method, &id, params.as_deref(), cfg, handlers);
}

fn usage_exit(code: i32, msg: Option<&str>) -> ! {
    if let Some(m) = msg {
        eprintln!("{m}");
    }
    eprintln!("Usage of perch-mcp:\n  -f string\n    \tPath to commands.perch (default \"commands.perch\")");
    std::process::exit(code)
}

/// Minimal port of Go's `flag` parsing for the single `-f` flag.
fn parse_flags(args: &[String]) -> String {
    let mut cfg = "commands.perch".to_string();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "-" || a == "--" || !a.starts_with('-') {
            break;
        }
        let name = a.trim_start_matches('-');
        let (name, val) = match name.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (name, None),
        };
        match name {
            "f" => match val {
                Some(v) => cfg = v,
                None => {
                    i += 1;
                    match args.get(i) {
                        Some(v) => cfg = v.clone(),
                        None => usage_exit(2, Some("flag needs an argument: -f")),
                    }
                }
            },
            "h" | "help" => usage_exit(0, None),
            _ => usage_exit(2, Some(&format!("flag provided but not defined: -{name}"))),
        }
        i += 1;
    }
    cfg
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = parse_flags(&args);
    let handlers = perch_ops::all_handlers();
    let mut reader = std::io::BufReader::new(std::io::stdin());
    loop {
        let mut buf = Vec::new();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) => {
                eprintln!("mcp: read: {e}");
                return;
            }
        }
        // Like Go's ReadString: a final unterminated line is dropped at EOF.
        if buf.last() != Some(&b'\n') {
            return;
        }
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        process_line(line, &cfg, &handlers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_escapes_html_like_go() {
        let s = encode_line(&json!({"a": "<x>&"}));
        assert_eq!(s, "{\"a\":\"\\u003cx\\u003e\\u0026\"}\n");
    }

    #[test]
    fn sorted_orders_keys() {
        let s = serde_json::to_string(&sorted(json!({"b": 1, "a": {"z": 1, "y": 2}}))).unwrap();
        assert_eq!(s, r#"{"a":{"y":2,"z":1},"b":1}"#);
    }

    #[test]
    fn tools_list_has_two_tools() {
        assert_eq!(tools_list()["tools"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn go_v_nil() {
        assert_eq!(go_v(&Value::Null), "<nil>");
        assert_eq!(go_v(&json!(3)), "3");
    }

    #[test]
    fn flags_default_and_set() {
        assert_eq!(parse_flags(&[]), "commands.perch");
        assert_eq!(parse_flags(&["-f".into(), "x.perch".into()]), "x.perch");
        assert_eq!(parse_flags(&["--f=y.perch".into()]), "y.perch");
    }
}
