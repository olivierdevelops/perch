//! A minimal re-implementation of the subset of Go's `html/template` that the
//! embedded `index.html` uses: `if`/`else if`/`else`/`range`/`end`, field
//! chains, `$`, `len`/`eq`/`ne`/`and`, comment stripping, and contextual
//! escaping (HTML text, HTML attribute, JS string). Execution errors stop the
//! output where they occur, exactly like `template.Execute` writing to a
//! `ResponseWriter` (partial output, error ignored by the caller).
use crate::gofmt::sprint_v;
use perch_domain::{Command, Program};
use serde_json::Value;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum Val {
    Nil,
    Bool(bool),
    Int(i64),
    Str(String),
    Json(Value),
    List(Vec<Val>),
    Obj(Rc<Vec<(&'static str, Val)>>),
}

impl Val {
    fn truthy(&self) -> bool {
        match self {
            Val::Nil => false,
            Val::Bool(b) => *b,
            Val::Int(i) => *i != 0,
            Val::Str(s) => !s.is_empty(),
            Val::Json(v) => match v {
                Value::Null => false,
                Value::Bool(b) => *b,
                Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
                Value::String(s) => !s.is_empty(),
                Value::Array(a) => !a.is_empty(),
                Value::Object(m) => !m.is_empty(),
            },
            Val::List(l) => !l.is_empty(),
            Val::Obj(_) => true,
        }
    }

    fn print(&self) -> String {
        match self {
            Val::Nil => "<no value>".into(),
            Val::Bool(b) => b.to_string(),
            Val::Int(i) => i.to_string(),
            Val::Str(s) => s.clone(),
            Val::Json(v) => sprint_v(v),
            Val::List(_) | Val::Obj(_) => String::new(),
        }
    }
}

fn obj(fields: Vec<(&'static str, Val)>) -> Val {
    Val::Obj(Rc::new(fields))
}

fn command_val(c: &Command) -> Val {
    let args = c
        .args
        .iter()
        .map(|a| {
            obj(vec![
                ("Name", Val::Str(a.name.clone())),
                ("Type", Val::Str(a.ty.clone())),
                ("Description", Val::Str(a.description.clone())),
                ("Default", Val::Json(a.default.clone())),
                ("HasDefault", Val::Bool(a.has_default)),
                ("Optional", Val::Bool(a.optional)),
                ("Rest", Val::Bool(a.rest)),
            ])
        })
        .collect();
    obj(vec![
        ("Name", Val::Str(c.name.clone())),
        ("Description", Val::Str(c.description.clone())),
        (
            "Modifiers",
            obj(vec![
                ("Test", Val::Bool(c.modifiers.test)),
                ("Detached", Val::Bool(c.modifiers.detached)),
                ("ProxyArgs", Val::Bool(c.modifiers.proxy_args)),
            ]),
        ),
        ("Args", Val::List(args)),
    ])
}

/// The template's root data (`tplData`): Program, Commands, Path.
pub fn root_data(p: &Program, cmds: &[&Command], path: &str) -> Val {
    let bindings = p
        .globals
        .bindings
        .iter()
        .map(|g| {
            obj(vec![
                ("Name", Val::Str(g.name.clone())),
                ("Type", Val::Str(g.ty.clone())),
                ("Value", Val::Json(g.value.clone())),
            ])
        })
        .collect();
    obj(vec![
        (
            "Program",
            obj(vec![
                ("Name", Val::Str(p.name.clone())),
                ("Description", Val::Str(p.description.clone())),
                ("Version", Val::Str(p.version.clone())),
                ("Globals", obj(vec![("Bindings", Val::List(bindings))])),
            ]),
        ),
        ("Commands", Val::List(cmds.iter().map(|c| command_val(c)).collect())),
        ("Path", Val::Str(path.to_string())),
    ])
}

// ── comment stripping (html/template elides comments) ──────────────────

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    Text,
    Script,
    Style,
}

fn starts_ci(s: &[u8], at: usize, pat: &str) -> bool {
    s.len() >= at + pat.len() && s[at..at + pat.len()].eq_ignore_ascii_case(pat.as_bytes())
}

/// Removes HTML comments, CSS/JS block comments (one space) and JS line
/// comments (their newline is kept), as Go's contextual escaper does.
pub fn strip_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    let mut mode = Mode::Text;
    while i < b.len() {
        match mode {
            Mode::Text => {
                if b[i..].starts_with(b"<!--") {
                    match src[i + 4..].find("-->") {
                        Some(e) => i += 4 + e + 3,
                        None => i = b.len(),
                    }
                } else if starts_ci(b, i, "<script") || starts_ci(b, i, "<style") {
                    let is_script = starts_ci(b, i, "<script");
                    let end = src[i..].find('>').map(|e| i + e + 1).unwrap_or(b.len());
                    out.extend_from_slice(&b[i..end]);
                    i = end;
                    mode = if is_script { Mode::Script } else { Mode::Style };
                } else {
                    out.push(b[i]);
                    i += 1;
                }
            }
            Mode::Style => {
                if starts_ci(b, i, "</style") {
                    mode = Mode::Text;
                } else if b[i..].starts_with(b"/*") {
                    match src[i + 2..].find("*/") {
                        Some(e) => i += 2 + e + 2,
                        None => i = b.len(),
                    }
                    out.push(b' ');
                    continue;
                }
                if mode == Mode::Style {
                    out.push(b[i]);
                    i += 1;
                }
            }
            Mode::Script => {
                if starts_ci(b, i, "</script") {
                    mode = Mode::Text;
                    continue;
                }
                match b[i] {
                    q @ (b'"' | b'\'' | b'`') => {
                        out.push(q);
                        i += 1;
                        while i < b.len() && b[i] != q {
                            if b[i] == b'\\' && i + 1 < b.len() {
                                out.push(b[i]);
                                i += 1;
                            }
                            out.push(b[i]);
                            i += 1;
                        }
                        if i < b.len() {
                            out.push(q);
                            i += 1;
                        }
                    }
                    b'/' if b.get(i + 1) == Some(&b'/') => {
                        while i < b.len() && b[i] != b'\n' {
                            i += 1;
                        }
                    }
                    b'/' if b.get(i + 1) == Some(&b'*') => {
                        match src[i + 2..].find("*/") {
                            Some(e) => i += 2 + e + 2,
                            None => i = b.len(),
                        }
                        out.push(b' ');
                    }
                    b'/' if regex_allowed(&out) => {
                        // Regular-expression literal: copy verbatim so quotes inside it
                        // don't start a string.
                        out.push(b'/');
                        i += 1;
                        let mut in_class = false;
                        while i < b.len() && b[i] != b'\n' {
                            let c = b[i];
                            out.push(c);
                            i += 1;
                            if c == b'\\' && i < b.len() {
                                out.push(b[i]);
                                i += 1;
                            } else if c == b'[' {
                                in_class = true;
                            } else if c == b']' {
                                in_class = false;
                            } else if c == b'/' && !in_class {
                                break;
                            }
                        }
                    }
                    c => {
                        out.push(c);
                        i += 1;
                    }
                }
            }
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Whether a `/` at this point starts a regex literal (previous significant
/// byte is an operator or opening punctuation).
fn regex_allowed(out: &[u8]) -> bool {
    match out.iter().rev().find(|c| !c.is_ascii_whitespace()) {
        None => true,
        Some(c) => b"(,=:[!&|?{};+-*%<>~^".contains(c),
    }
}

// ── parsing ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ctx {
    Text,
    Attr,
    JsAttr,
    JsStr,
}

#[derive(Debug)]
enum Expr {
    Dot(Vec<String>),
    Root(Vec<String>),
    Str(String),
    Int(i64),
    Call(String, Vec<Expr>),
}

#[derive(Debug)]
enum Node {
    Text(String),
    Out(Expr, Ctx),
    If(Vec<(Expr, Vec<Node>)>, Vec<Node>),
    Range(Expr, Vec<Node>),
}

#[derive(Default)]
struct Html {
    script: bool,
    style: bool,
    in_tag: bool,
    tag: String,
    quote: Option<char>,
    attr: String,
}

impl Html {
    fn feed(&mut self, s: &str) {
        let cs: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < cs.len() {
            let c = cs[i];
            if self.script || self.style {
                let close = if self.script { "</script" } else { "</style" };
                let rest: String = cs[i..].iter().take(close.len()).collect();
                if rest.eq_ignore_ascii_case(close) {
                    self.script = false;
                    self.style = false;
                }
                i += 1;
                continue;
            }
            if self.in_tag {
                if let Some(q) = self.quote {
                    if c == q {
                        self.quote = None;
                    }
                } else if c == '"' || c == '\'' {
                    // attribute name = word before the preceding '='
                    self.quote = Some(c);
                } else if c == '>' {
                    self.in_tag = false;
                    if self.tag.eq_ignore_ascii_case("script") {
                        self.script = true;
                    } else if self.tag.eq_ignore_ascii_case("style") {
                        self.style = true;
                    }
                } else if c == '=' {
                    // find attribute name backwards
                    let mut j = i;
                    while j > 0 && cs[j - 1].is_whitespace() {
                        j -= 1;
                    }
                    let end = j;
                    while j > 0 && !cs[j - 1].is_whitespace() {
                        j -= 1;
                    }
                    self.attr = cs[j..end].iter().collect::<String>().to_ascii_lowercase();
                }
            } else if c == '<' && cs.get(i + 1).is_some_and(|n| n.is_ascii_alphabetic()) {
                self.in_tag = true;
                self.quote = None;
                self.attr.clear();
                let mut j = i + 1;
                let mut name = String::new();
                while j < cs.len() && (cs[j].is_ascii_alphanumeric()) {
                    name.push(cs[j]);
                    j += 1;
                }
                self.tag = name;
                i = j;
                continue;
            }
            i += 1;
        }
    }

    fn ctx(&self) -> Ctx {
        if self.script {
            Ctx::JsStr
        } else if self.in_tag {
            if self.quote.is_some() && self.attr.starts_with("on") {
                Ctx::JsAttr
            } else {
                Ctx::Attr
            }
        } else {
            Ctx::Text
        }
    }
}

enum Tok {
    Text(String),
    Action(String, Ctx),
}

fn tokenize(src: &str) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut html = Html::default();
    let mut rest = src;
    while let Some(start) = rest.find("{{") {
        let text = &rest[..start];
        if !text.is_empty() {
            html.feed(text);
            toks.push(Tok::Text(text.to_string()));
        }
        let after = &rest[start + 2..];
        let end = after.find("}}").unwrap_or(after.len());
        toks.push(Tok::Action(after[..end].trim().to_string(), html.ctx()));
        rest = &after[(end + 2).min(after.len())..];
    }
    if !rest.is_empty() {
        toks.push(Tok::Text(rest.to_string()));
    }
    toks
}

fn lex_expr(s: &str) -> Vec<String> {
    let cs: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '(' || c == ')' {
            out.push(c.to_string());
            i += 1;
        } else if c == '"' {
            let mut t = String::from("\"");
            i += 1;
            while i < cs.len() && cs[i] != '"' {
                t.push(cs[i]);
                i += 1;
            }
            t.push('"');
            i += 1;
            out.push(t);
        } else {
            let mut t = String::new();
            while i < cs.len() && !cs[i].is_whitespace() && cs[i] != '(' && cs[i] != ')' {
                t.push(cs[i]);
                i += 1;
            }
            out.push(t);
        }
    }
    out
}

fn parse_term(toks: &[String], pos: &mut usize) -> Expr {
    let t = &toks[*pos];
    *pos += 1;
    if t == "(" {
        let e = parse_pipeline(toks, pos);
        if toks.get(*pos).map(|s| s.as_str()) == Some(")") {
            *pos += 1;
        }
        return e;
    }
    if let Some(s) = t.strip_prefix('"') {
        return Expr::Str(s.trim_end_matches('"').to_string());
    }
    if let Ok(n) = t.parse::<i64>() {
        return Expr::Int(n);
    }
    if let Some(rest) = t.strip_prefix('$') {
        return Expr::Root(rest.split('.').filter(|s| !s.is_empty()).map(String::from).collect());
    }
    Expr::Dot(t.split('.').filter(|s| !s.is_empty()).map(String::from).collect())
}

fn parse_pipeline(toks: &[String], pos: &mut usize) -> Expr {
    if let Some(t) = toks.get(*pos) {
        if matches!(t.as_str(), "len" | "eq" | "ne" | "and") {
            let name = t.clone();
            *pos += 1;
            let mut args = Vec::new();
            while *pos < toks.len() && toks[*pos] != ")" {
                args.push(parse_term(toks, pos));
            }
            return Expr::Call(name, args);
        }
    }
    parse_term(toks, pos)
}

fn parse_expr(s: &str) -> Expr {
    let toks = lex_expr(s);
    let mut pos = 0;
    parse_pipeline(&toks, &mut pos)
}

enum Stop {
    End,
    Else,
    ElseIf(String),
    Eof,
}

fn parse_nodes(toks: &mut std::iter::Peekable<std::vec::IntoIter<Tok>>) -> (Vec<Node>, Stop) {
    let mut nodes = Vec::new();
    while let Some(t) = toks.next() {
        match t {
            Tok::Text(s) => nodes.push(Node::Text(s)),
            Tok::Action(a, ctx) => {
                if a == "end" {
                    return (nodes, Stop::End);
                }
                if a == "else" {
                    return (nodes, Stop::Else);
                }
                if let Some(c) = a.strip_prefix("else if ") {
                    return (nodes, Stop::ElseIf(c.to_string()));
                }
                if let Some(c) = a.strip_prefix("if ") {
                    nodes.push(parse_if(c, toks));
                } else if let Some(c) = a.strip_prefix("range ") {
                    let (body, _) = parse_nodes(toks);
                    nodes.push(Node::Range(parse_expr(c), body));
                } else {
                    nodes.push(Node::Out(parse_expr(&a), ctx));
                }
            }
        }
    }
    (nodes, Stop::Eof)
}

fn parse_if(cond: &str, toks: &mut std::iter::Peekable<std::vec::IntoIter<Tok>>) -> Node {
    let mut branches = Vec::new();
    let mut els = Vec::new();
    let mut cur = cond.to_string();
    loop {
        let (body, stop) = parse_nodes(toks);
        branches.push((parse_expr(&cur), body));
        match stop {
            Stop::ElseIf(c) => cur = c,
            Stop::Else => {
                let (b, _) = parse_nodes(toks);
                els = b;
                break;
            }
            Stop::End | Stop::Eof => break,
        }
    }
    Node::If(branches, els)
}

// ── evaluation ─────────────────────────────────────────────────────────

struct Exec<'a> {
    root: &'a Val,
    out: String,
}

fn field(v: &Val, chain: &[String]) -> Result<Val, String> {
    let mut cur = v.clone();
    for f in chain {
        cur = match &cur {
            Val::Obj(fs) => fs
                .iter()
                .find(|(k, _)| k == f)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| format!("can't evaluate field {f}"))?,
            _ => return Err(format!("can't evaluate field {f}")),
        };
    }
    Ok(cur)
}

fn eval(e: &Expr, dot: &Val, root: &Val) -> Result<Val, String> {
    match e {
        Expr::Dot(c) => field(dot, c),
        Expr::Root(c) => field(root, c),
        Expr::Str(s) => Ok(Val::Str(s.clone())),
        Expr::Int(i) => Ok(Val::Int(*i)),
        Expr::Call(name, args) => {
            let vals: Result<Vec<Val>, String> = args.iter().map(|a| eval(a, dot, root)).collect();
            let vals = vals?;
            match name.as_str() {
                "len" => Ok(Val::Int(match vals.first() {
                    Some(Val::List(l)) => l.len() as i64,
                    Some(Val::Str(s)) => s.len() as i64,
                    _ => 0,
                })),
                "eq" | "ne" => {
                    let same = match (vals.first(), vals.get(1)) {
                        (Some(Val::Str(a)), Some(Val::Str(b))) => a == b,
                        (Some(Val::Int(a)), Some(Val::Int(b))) => a == b,
                        (Some(Val::Bool(a)), Some(Val::Bool(b))) => a == b,
                        _ => false,
                    };
                    Ok(Val::Bool(if name == "eq" { same } else { !same }))
                }
                "and" => {
                    let mut last = Val::Nil;
                    for v in vals {
                        if !v.truthy() {
                            return Ok(v);
                        }
                        last = v;
                    }
                    Ok(last)
                }
                _ => Err(format!("function {name} not defined")),
            }
        }
    }
}

fn html_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\0' => o.push('\u{fffd}'),
            '"' => o.push_str("&#34;"),
            '&' => o.push_str("&amp;"),
            '\'' => o.push_str("&#39;"),
            '+' => o.push_str("&#43;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            c => o.push(c),
        }
    }
    o
}

fn js_str_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\0' => o.push_str("\\u0000"),
            '\t' => o.push_str("\\t"),
            '\n' => o.push_str("\\n"),
            '\u{b}' => o.push_str("\\u000b"),
            '\u{c}' => o.push_str("\\f"),
            '\r' => o.push_str("\\r"),
            '"' => o.push_str("\\u0022"),
            '`' => o.push_str("\\u0060"),
            '&' => o.push_str("\\u0026"),
            '\'' => o.push_str("\\u0027"),
            '+' => o.push_str("\\u002b"),
            '/' => o.push_str("\\/"),
            '<' => o.push_str("\\u003c"),
            '>' => o.push_str("\\u003e"),
            '\\' => o.push_str("\\\\"),
            '\u{2028}' => o.push_str("\\u2028"),
            '\u{2029}' => o.push_str("\\u2029"),
            c => o.push(c),
        }
    }
    o
}

impl Exec<'_> {
    fn run(&mut self, nodes: &[Node], dot: &Val) -> Result<(), String> {
        for n in nodes {
            match n {
                Node::Text(s) => self.out.push_str(s),
                Node::Out(e, ctx) => {
                    let s = eval(e, dot, self.root)?.print();
                    let esc = match ctx {
                        Ctx::Text | Ctx::Attr => html_escape(&s),
                        Ctx::JsStr => js_str_escape(&s),
                        Ctx::JsAttr => html_escape(&js_str_escape(&s)),
                    };
                    self.out.push_str(&esc);
                }
                Node::If(branches, els) => {
                    let mut done = false;
                    for (c, body) in branches {
                        if eval(c, dot, self.root)?.truthy() {
                            self.run(body, dot)?;
                            done = true;
                            break;
                        }
                    }
                    if !done {
                        self.run(els, dot)?;
                    }
                }
                Node::Range(e, body) => {
                    if let Val::List(items) = eval(e, dot, self.root)? {
                        for it in &items {
                            self.run(body, it)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// A parsed page template.
pub struct Template {
    nodes: Vec<Node>,
}

impl Template {
    pub fn parse(src: &str) -> Template {
        let stripped = strip_comments(src);
        let mut toks = tokenize(&stripped).into_iter().peekable();
        let (nodes, _) = parse_nodes(&mut toks);
        Template { nodes }
    }

    /// Renders with `root`; on an execution error returns the partial output
    /// produced so far (Go streams to the writer and the caller ignores the error).
    pub fn execute(&self, root: &Val) -> (String, Option<String>) {
        let mut ex = Exec { root, out: String::new() };
        let r = ex.run(&self.nodes, root);
        (ex.out, r.err())
    }
}
