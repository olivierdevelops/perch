//! Request routing and the JSON / NDJSON API handlers.
use crate::gofmt::sprint_v;
use crate::gojson::{decode_first, encode_line, from_value, kind_name, J};
use crate::http::{ChunkSink, Reply, Request};
use crate::template::{root_data, Template};
use crate::KnownOpsFn;
use perch_domain::{Command, Program};
use perch_interpreter::{HTTPPolicy, Handler, Interpreter, SharedWriter};
use perch_simulate::{Fixture, OracleSet, Scenario_, SimEnv};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::sync::Arc;

const INDEX_HTML: &str = include_str!("../index.html");

pub struct Shared {
    program: Arc<Program>,
    handlers: HashMap<String, Handler>,
    config_path: String,
    known_ops: Option<KnownOpsFn>,
    template: Template,
}

impl Shared {
    pub fn new(p: Program, handlers: HashMap<String, Handler>, config_path: String, known_ops: Option<KnownOpsFn>) -> Shared {
        Shared { program: Arc::new(p), handlers, config_path, known_ops, template: Template::parse(INDEX_HTML) }
    }
}

type Headers = Vec<(String, String)>;

fn h(k: &str, v: &str) -> (String, String) {
    (k.to_string(), v.to_string())
}

fn json_reply(j: J) -> Reply {
    Reply::Full { status: 200, headers: vec![h("Content-Type", "application/json")], body: encode_line(&j) }
}

/// `http.Error`: plain-text message plus newline.
fn http_error(status: u16, msg: &str) -> Reply {
    let headers: Headers = vec![h("Content-Type", "text/plain; charset=utf-8"), h("X-Content-Type-Options", "nosniff")];
    Reply::Full { status, headers, body: format!("{msg}\n").into_bytes() }
}

// ── routing (net/http ServeMux semantics) ───────────────────────────────

/// `path.Clean` preserving a trailing slash (`ServeMux.cleanPath`).
fn clean_path(p: &str) -> String {
    if p.is_empty() {
        return "/".into();
    }
    let p = if p.starts_with('/') { p.to_string() } else { format!("/{p}") };
    let mut stack: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            s => stack.push(s),
        }
    }
    let mut np = format!("/{}", stack.join("/"));
    if p.ends_with('/') && np != "/" {
        np.push('/');
    }
    np
}

fn escape_path(p: &str) -> String {
    let mut o = String::new();
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/".contains(&b) {
            o.push(b as char);
        } else {
            o.push_str(&format!("%{b:02X}"));
        }
    }
    o
}

fn html_escape_min(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&#34;").replace('\'', "&#39;")
}

pub fn route(s: &Shared, req: &Request) -> Reply {
    if req.method != "CONNECT" {
        let clean = clean_path(&req.path);
        if clean != req.path && req.path != "*" {
            let mut loc = escape_path(&clean);
            if !req.raw_query.is_empty() {
                loc.push('?');
                loc.push_str(&req.raw_query);
            }
            let mut headers: Headers = vec![h("Location", &loc)];
            let mut body = Vec::new();
            if req.method == "GET" || req.method == "HEAD" {
                headers.push(h("Content-Type", "text/html; charset=utf-8"));
                body = format!("<a href=\"{}\">Moved Permanently</a>.\n\n", html_escape_min(&loc)).into_bytes();
            }
            return Reply::Full { status: 301, headers, body };
        }
    }
    match req.path.as_str() {
        "/api/program" => handle_program(s),
        "/api/exec" => handle_exec(s, req),
        "/api/check" => handle_check(s),
        "/api/scan" => handle_scan(s),
        "/api/simulate" => handle_simulate(s, req),
        _ => handle_index(s),
    }
}

// ── / ────────────────────────────────────────────────────────────────

fn visible_commands(p: &Program) -> Vec<&Command> {
    let mut out: Vec<&Command> = p.commands.values().filter(|c| !c.modifiers.private).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn handle_index(s: &Shared) -> Reply {
    let cmds = visible_commands(&s.program);
    let root = root_data(&s.program, &cmds, &s.config_path);
    // Execution errors keep the partial output (Go ignores Execute's error).
    let (body, _err) = s.template.execute(&root);
    Reply::Full { status: 200, headers: vec![h("Content-Type", "text/html; charset=utf-8")], body: body.into_bytes() }
}

// ── /api/program ───────────────────────────────────────────────────────

fn arg_view(a: &perch_domain::ArgSpec) -> J {
    let mut f: Vec<(String, J)> = vec![("name".into(), J::s(&a.name)), ("type".into(), J::s(&a.ty))];
    if !a.description.is_empty() {
        f.push(("description".into(), J::s(&a.description)));
    }
    if !a.default.is_null() {
        f.push(("default".into(), from_value(&a.default)));
    }
    if a.has_default {
        f.push(("has_default".into(), J::Bool(true)));
    }
    if a.optional {
        f.push(("optional".into(), J::Bool(true)));
    }
    if let Some(i) = a.index {
        f.push(("index".into(), J::Num(i as f64)));
    }
    if a.rest {
        f.push(("rest".into(), J::Bool(true)));
    }
    J::Obj(f)
}

fn handle_program(s: &Shared) -> Reply {
    let p = &*s.program;
    let mut out: BTreeMap<String, J> = BTreeMap::new();
    out.insert("name".into(), J::s(&p.name));
    out.insert("description".into(), J::s(&p.description));
    out.insert("version".into(), J::s(&p.version));
    out.insert("path".into(), J::s(&s.config_path));
    let cmds: Vec<J> = visible_commands(p)
        .into_iter()
        .map(|c| {
            let mut f: Vec<(String, J)> = vec![("name".into(), J::s(&c.name))];
            if !c.description.is_empty() {
                f.push(("description".into(), J::s(&c.description)));
            }
            if !c.args.is_empty() {
                f.push(("args".into(), J::Arr(c.args.iter().map(arg_view).collect())));
            }
            if c.modifiers.test {
                f.push(("is_test".into(), J::Bool(true)));
            }
            if c.modifiers.detached {
                f.push(("detached".into(), J::Bool(true)));
            }
            if c.modifiers.proxy_args {
                f.push(("proxy_args".into(), J::Bool(true)));
            }
            J::Obj(f)
        })
        .collect();
    out.insert("commands".into(), J::Arr(cmds));
    let globals: Vec<J> = p
        .globals
        .bindings
        .iter()
        .map(|g| J::Obj(vec![("name".into(), J::s(&g.name)), ("type".into(), J::s(&g.ty)), ("value".into(), from_value(&g.value))]))
        .collect();
    out.insert("globals".into(), J::arr_or_null(globals));
    if let Some(c) = &p.catch {
        out.insert("catch".into(), J::map(vec![("bind", J::s(&c.bind)), ("description", J::s(&c.description))]));
    }
    json_reply(J::Map(out))
}

// ── request decoding (Go encoding/json semantics) ──────────────────────

/// Looks up `name` among object keys the way Go does: exact match or
/// case-insensitive; later keys win.
fn keys_matching<'a>(m: &'a Map<String, Value>, name: &str) -> Vec<&'a Value> {
    m.iter().filter(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v).collect()
}

struct DecodeErr {
    first: Option<String>,
}

impl DecodeErr {
    fn type_err(&mut self, v: &Value, strukt: &str, field: &str, ty: &str) {
        if self.first.is_none() {
            self.first =
                Some(format!("json: cannot unmarshal {} into Go struct field {}.{} of type {}", kind_name(v), strukt, field, ty));
        }
    }
}

fn get_string(d: &mut DecodeErr, m: &Map<String, Value>, key: &str, strukt: &str, field: &str) -> Option<String> {
    let mut out = None;
    for v in keys_matching(m, key) {
        match v {
            Value::String(s) => out = Some(s.clone()),
            Value::Null => {}
            other => d.type_err(other, strukt, field, "string"),
        }
    }
    out
}

fn get_bool(d: &mut DecodeErr, m: &Map<String, Value>, key: &str, strukt: &str, field: &str) -> Option<bool> {
    let mut out = None;
    for v in keys_matching(m, key) {
        match v {
            Value::Bool(b) => out = Some(*b),
            Value::Null => {}
            other => d.type_err(other, strukt, field, "bool"),
        }
    }
    out
}

/// `Some(None)` = explicit null (nil slice), `Some(Some(v))` = value.
fn get_strings(d: &mut DecodeErr, m: &Map<String, Value>, key: &str, strukt: &str, field: &str) -> Option<Vec<String>> {
    let mut out = None;
    for v in keys_matching(m, key) {
        match v {
            Value::Array(a) => {
                let mut items = Vec::new();
                for e in a {
                    match e {
                        Value::String(s) => items.push(s.clone()),
                        Value::Null => items.push(String::new()),
                        other => {
                            d.type_err(other, strukt, field, "string");
                            items.push(String::new());
                        }
                    }
                }
                out = Some(items);
            }
            Value::Null => out = None,
            other => d.type_err(other, strukt, field, "[]string"),
        }
    }
    out
}

fn decode_sim_env(d: &mut DecodeErr, v: &Value) -> SimEnv {
    let mut env = SimEnv::default();
    let Value::Object(m) = v else {
        if !v.is_null() {
            d.type_err(v, "simReq", "env", "simulate.SimEnv");
        }
        return env;
    };
    let f = |n: &str| format!("env.{n}");
    if let Some(s) = get_string(d, m, "OS", "SimEnv", &f("OS")) {
        env.os = s;
    }
    if let Some(s) = get_string(d, m, "Arch", "SimEnv", &f("Arch")) {
        env.arch = s;
    }
    for v in keys_matching(m, "Env") {
        match v {
            Value::Object(o) => {
                let mut mm = BTreeMap::new();
                for (k, e) in o {
                    match e {
                        Value::String(s) => {
                            mm.insert(k.clone(), s.clone());
                        }
                        Value::Null => {
                            mm.insert(k.clone(), String::new());
                        }
                        other => d.type_err(other, "SimEnv", &f("Env"), "string"),
                    }
                }
                env.env = Some(mm);
            }
            Value::Null => env.env = None,
            other => d.type_err(other, "SimEnv", &f("Env"), "map[string]string"),
        }
    }
    if let Some(b) = get_bool(d, m, "EnvRestrict", "SimEnv", &f("EnvRestrict")) {
        env.env_restrict = b;
    }
    env.fs_read = get_strings(d, m, "FsRead", "SimEnv", &f("FsRead"));
    env.fs_write = get_strings(d, m, "FsWrite", "SimEnv", &f("FsWrite"));
    for v in keys_matching(m, "Bins") {
        match v {
            Value::Object(o) => {
                let mut mm = BTreeMap::new();
                for (k, e) in o {
                    match e {
                        Value::Bool(b) => {
                            mm.insert(k.clone(), *b);
                        }
                        Value::Null => {
                            mm.insert(k.clone(), false);
                        }
                        other => d.type_err(other, "SimEnv", &f("Bins"), "bool"),
                    }
                }
                env.bins = Some(mm);
            }
            Value::Null => env.bins = None,
            other => d.type_err(other, "SimEnv", &f("Bins"), "map[string]bool"),
        }
    }
    env.network = get_strings(d, m, "Network", "SimEnv", &f("Network"));
    if let Some(b) = get_bool(d, m, "NoShell", "SimEnv", &f("NoShell")) {
        env.no_shell = b;
    }
    if let Some(b) = get_bool(d, m, "NoSubprocess", "SimEnv", &f("NoSubprocess")) {
        env.no_subprocess = b;
    }
    if let Some(b) = get_bool(d, m, "NoNetwork", "SimEnv", &f("NoNetwork")) {
        env.no_network = b;
    }
    if let Some(b) = get_bool(d, m, "NoWrite", "SimEnv", &f("NoWrite")) {
        env.no_write = b;
    }
    env
}

// ── /api/exec (NDJSON stream) ───────────────────────────────────────────

struct ExecRequest {
    command: String,
    args: Map<String, Value>,
    env_only: Vec<String>,
    allow_bin: Vec<String>,
    allow_host: Vec<String>,
}

fn decode_exec(body: &[u8]) -> Result<ExecRequest, String> {
    let v = decode_first(body)?;
    let mut d = DecodeErr { first: None };
    let mut req = ExecRequest { command: String::new(), args: Map::new(), env_only: vec![], allow_bin: vec![], allow_host: vec![] };
    match &v {
        Value::Object(m) => {
            if let Some(s) = get_string(&mut d, m, "command", "execRequest", "command") {
                req.command = s;
            }
            for a in keys_matching(m, "args") {
                match a {
                    Value::Object(o) => req.args = o.clone(),
                    Value::Null => req.args = Map::new(),
                    other => d.type_err(other, "execRequest", "args", "map[string]interface {}"),
                }
            }
            req.env_only = get_strings(&mut d, m, "env_only", "execRequest", "env_only").unwrap_or_default();
            req.allow_bin = get_strings(&mut d, m, "allow_bin", "execRequest", "allow_bin").unwrap_or_default();
            req.allow_host = get_strings(&mut d, m, "allow_host", "execRequest", "allow_host").unwrap_or_default();
        }
        Value::Null => {}
        other => {
            return Err(format!("json: cannot unmarshal {} into Go value of type httpserver.execRequest", kind_name(other)));
        }
    }
    match d.first {
        Some(e) => Err(e),
        None => Ok(req),
    }
}

/// Adapts the chunk sink to an `io::Writer`: every write becomes one
/// `{"kind":K,"msg":M}` NDJSON line, flushed immediately.
struct NdjsonWriter {
    sink: ChunkSink,
    kind: &'static str,
}

fn emit(sink: &ChunkSink, kind: &str, msg: &str) {
    let j = J::map(vec![("kind", J::s(kind)), ("msg", J::s(msg.trim_end_matches('\n')))]);
    let _ = sink.write(&encode_line(&j));
}

impl Write for NdjsonWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        emit(&self.sink, self.kind, &String::from_utf8_lossy(buf));
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn handle_exec(s: &Shared, req: &Request) -> Reply {
    if req.method != "POST" {
        return http_error(405, "POST only");
    }
    let er = match decode_exec(&req.body) {
        Ok(r) => r,
        Err(e) => return http_error(400, &e),
    };
    match s.program.commands.get(&er.command) {
        Some(c) if !c.modifiers.private => {}
        _ => return http_error(404, "command not found"),
    }
    let program = s.program.clone();
    let handlers = s.handlers.clone();
    let headers: Headers = vec![h("Content-Type", "application/x-ndjson"), h("Cache-Control", "no-cache")];
    Reply::Stream {
        headers,
        run: Box::new(move |sink: ChunkSink| {
            let mut i = Interpreter::new(handlers, program);
            i.preflight_hook = Some(Arc::new(perch_ops::preflight));
            i.hook_category = Some(Arc::new(perch_ops::hook_category_of));
            i.stdout = SharedWriter::new(Box::new(NdjsonWriter { sink: sink.clone(), kind: "out" }));
            i.stderr = SharedWriter::new(Box::new(NdjsonWriter { sink: sink.clone(), kind: "err" }));
            if !er.allow_bin.is_empty() {
                i.allowed_shell_bins = Some(er.allow_bin.iter().map(|b| (b.clone(), true)).collect());
            }
            if !er.allow_host.is_empty() {
                i.http_policy = Some(HTTPPolicy { allowed_hosts: er.allow_host.clone(), max_redirects: 5, ..Default::default() });
            }
            if !er.env_only.is_empty() {
                i.env_allowlist = Some(er.env_only.iter().map(|e| (e.clone(), true)).collect());
            }
            // Turn the named-args map into a flat -k=v argv (sorted for stability).
            let mut keys: Vec<&String> = er.args.keys().collect();
            keys.sort();
            let argv: Vec<String> = keys.iter().map(|k| format!("-{}={}", k, sprint_v(&er.args[*k]))).collect();

            emit(&sink, "status", "started");
            if let Err(e) = i.run(&er.command, &argv) {
                emit(&sink, "err", &e.to_string());
                emit(&sink, "status", "error");
                return;
            }
            emit(&sink, "status", "ok");
        }),
    }
}

// ── /api/check ────────────────────────────────────────────────────────

fn handle_check(s: &Shared) -> Reply {
    let known = s.known_ops.as_ref().map(|f| f()).unwrap_or_default();
    let issues = perch_validate::check(&s.program, &known);
    let (mut errors, mut warnings) = (0u32, 0u32);
    let mut res = Vec::new();
    for iss in &issues {
        let mut f: Vec<(String, J)> = vec![("severity".into(), J::s(&iss.severity))];
        if !iss.where_.is_empty() {
            f.push(("where".into(), J::s(&iss.where_)));
        }
        f.push(("message".into(), J::s(&iss.message)));
        res.push(J::Obj(f));
        match iss.severity.as_str() {
            "error" => errors += 1,
            "warning" => warnings += 1,
            _ => {}
        }
    }
    json_reply(J::map(vec![
        ("ok", J::Bool(errors == 0)),
        ("errors", J::Num(errors as f64)),
        ("warnings", J::Num(warnings as f64)),
        ("issues", J::Arr(res)),
    ]))
}

// ── /api/scan ─────────────────────────────────────────────────────────

fn counts(m: &BTreeMap<String, usize>) -> J {
    J::Map(m.iter().map(|(k, v)| (k.clone(), J::Num(*v as f64))).collect())
}

fn handle_scan(s: &Shared) -> Reply {
    let p = &*s.program;
    let rep = perch_scan::analyze(p);
    let mut buf: Vec<u8> = Vec::new();
    let _ = perch_scan::print_report(&mut buf, p, &s.config_path, &rep);
    let recommended = perch_scan::recommended_invocation(&s.config_path, &rep);
    let (score, reasons) = perch_scan::score_report(&rep);
    let findings: Vec<J> = rep
        .findings
        .iter()
        .map(|f| {
            J::Obj(vec![
                ("Severity".into(), J::s(&f.severity)),
                ("Where".into(), J::s(&f.where_)),
                ("Issue".into(), J::s(&f.issue)),
                ("Fix".into(), J::s(&f.fix)),
            ])
        })
        .collect();
    let reasons_j = if reasons.is_empty() { J::Null } else { J::strs(&reasons) };
    json_reply(J::map(vec![
        ("report", J::s(String::from_utf8_lossy(&buf))),
        ("risk", J::map(vec![("score", J::s(score.to_string())), ("reasons", reasons_j)])),
        (
            "capabilities",
            J::map(vec![
                ("shell", J::Bool(rep.needs_shell)),
                ("shell_bins", counts(&rep.shell_bins)),
                ("subprocess", J::Bool(rep.needs_subprocess)),
                ("subprocess_ops", counts(&rep.subprocess_ops)),
                ("network", counts(&rep.hosts)),
                ("writes", counts(&rep.write_roots)),
                ("envs", counts(&rep.env_vars)),
                ("sudo", J::Bool(rep.has_shell_sudo)),
                ("shell_pipe", J::Bool(rep.has_shell_pipe)),
                ("proxy_args", J::Bool(rep.has_proxy_args)),
                ("catch_forwards", J::Bool(rep.catch_forwards)),
            ]),
        ),
        ("findings", J::arr_or_null(findings)),
        ("recommended", J::strs(&recommended)),
    ]))
}

// ── /api/simulate ─────────────────────────────────────────────────────

fn merge_fixture(f: &Fixture, cli: &SimEnv) -> SimEnv {
    let fx = f.to_sim_env();
    let mut out = cli.clone();
    if out.os.is_empty() {
        out.os = fx.os;
    }
    if out.arch.is_empty() {
        out.arch = fx.arch;
    }
    if out.env.is_none() {
        out.env = fx.env;
    }
    if !out.env_restrict {
        out.env_restrict = fx.env_restrict;
    }
    if out.fs_read.is_none() {
        out.fs_read = fx.fs_read;
    }
    if out.fs_write.is_none() {
        out.fs_write = fx.fs_write;
    }
    if out.bins.is_none() {
        out.bins = fx.bins;
    }
    if out.network.is_none() {
        out.network = fx.network;
    }
    out.no_shell = out.no_shell || fx.no_shell;
    out.no_subprocess = out.no_subprocess || fx.no_subprocess;
    out.no_network = out.no_network || fx.no_network;
    out.no_write = out.no_write || fx.no_write;
    out
}

fn command_names_of(p: &Program) -> Vec<String> {
    let mut out: Vec<String> = p.commands.iter().filter(|(_, c)| !c.modifiers.private).map(|(n, _)| n.clone()).collect();
    out.sort();
    out
}

struct SimRequest {
    command: String,
    env: SimEnv,
    fixture: Option<Fixture>,
}

fn decode_sim(body: &[u8]) -> Result<SimRequest, String> {
    let v = decode_first(body)?;
    let mut d = DecodeErr { first: None };
    let mut req = SimRequest { command: String::new(), env: SimEnv::default(), fixture: None };
    match &v {
        Value::Object(m) => {
            if let Some(s) = get_string(&mut d, m, "command", "simReq", "command") {
                req.command = s;
            }
            for e in keys_matching(m, "env") {
                req.env = decode_sim_env(&mut d, e);
            }
            for f in keys_matching(m, "fixture") {
                match f {
                    Value::Null => req.fixture = None,
                    Value::Object(_) => match serde_json::from_value::<Fixture>(f.clone()) {
                        Ok(fx) => req.fixture = Some(fx),
                        Err(e) => {
                            if d.first.is_none() {
                                d.first = Some(format!("json: cannot unmarshal into Go struct field simReq.fixture: {e}"));
                            }
                        }
                    },
                    other => d.type_err(other, "simReq", "fixture", "simulate.Fixture"),
                }
            }
        }
        Value::Null => {}
        other => {
            return Err(format!("json: cannot unmarshal {} into Go value of type httpserver.simReq", kind_name(other)));
        }
    }
    match d.first {
        Some(e) => Err(e),
        None => Ok(req),
    }
}

fn handle_simulate(s: &Shared, req: &Request) -> Reply {
    if req.method != "POST" {
        return http_error(405, "POST only");
    }
    let sr = match decode_sim(&req.body) {
        Ok(r) => r,
        Err(e) => return http_error(400, &e),
    };
    let p = &*s.program;
    let mut buf: Vec<u8> = Vec::new();
    if let Some(fix) = &sr.fixture {
        let merged = merge_fixture(fix, &sr.env);
        let scenarios: Vec<Scenario_> = if fix.scenarios.is_empty() {
            vec![Scenario_ { name: "default".into(), ..Default::default() }]
        } else {
            fix.scenarios.clone()
        };
        let mut failures = 0usize;
        for (idx, sc) in scenarios.iter().enumerate() {
            if idx > 0 {
                buf.push(b'\n');
            }
            let _ = writeln!(buf, "═══ Scenario: {} ═══", sc.name);
            let oracles: OracleSet = perch_simulate::merge_oracles(&fix.oracles, &sc.overrides);
            let names = if sr.command.is_empty() { command_names_of(p) } else { vec![sr.command.clone()] };
            for (j, name) in names.iter().enumerate() {
                if j > 0 {
                    buf.push(b'\n');
                }
                let (res, _) = perch_simulate::simulate_with_state(p, name, &merged, &oracles);
                let _ = perch_simulate::render_result(&mut buf, &res, p, name);
                failures += res.will_fail;
            }
        }
        return json_reply(J::map(vec![("ok", J::Bool(failures == 0)), ("report", J::s(String::from_utf8_lossy(&buf)))]));
    }
    // No fixture: the static walker.
    let names = if sr.command.is_empty() { command_names_of(p) } else { vec![sr.command.clone()] };
    for (idx, name) in names.iter().enumerate() {
        if idx > 0 {
            buf.push(b'\n');
        }
        let res = perch_simulate::simulate_command(p, name, &sr.env);
        let _ = perch_simulate::render_result(&mut buf, &res, p, name);
    }
    json_reply(J::map(vec![("ok", J::Bool(true)), ("report", J::s(String::from_utf8_lossy(&buf)))]))
}
