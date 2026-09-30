//! Checks a perch program for problems before it runs:
//!
//!   - every arg has a valid `type` and its `default` (if any) matches that type
//!   - duplicate arg names / duplicate `index` slots within a command
//!   - every `run TARGET` op resolves to an existing command
//!   - every `on_signal HANDLER` modifier resolves to an existing command
//!   - every op kind in a body is registered with the interpreter
//!   - `finally` / `rescue` dividers sit only inside a `try` body, once each, in order
//!   - an inline `NAME=value` env prefix sits on an `exec` op only, with a valid name
//!   - every `${name}` placeholder in a string-valued op arg resolves to a
//!     declared arg / `let` capture / global, or looks like an env var
use perch_domain::{Command, Catch, Op, Program, Requirements};
use perch_interpreter::{go_quote, provided_var_names, to_string_value};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::Write;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Loads a program from disk.
pub type LoadFn = Box<dyn Fn(&str) -> Result<Program, Error> + Send + Sync>;

/// Returns the registered op kinds (so the validator can check that every kind
/// referenced in a body has a handler).
pub type KnownOps = Box<dyn Fn() -> HashSet<String> + Send + Sync>;

pub struct Impl {
    pub load: LoadFn,
    pub known_ops: KnownOps,
}

/// One validation finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// "error" | "warning"
    pub severity: String,
    /// Command name + sub-location, or "" for file-level.
    pub where_: String,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.where_.is_empty() {
            write!(f, "{}: {}", self.severity, self.message)
        } else {
            write!(f, "{}: {}: {}", self.severity, self.where_, self.message)
        }
    }
}

impl Impl {
    /// Runs the full validation pass, printing the report to stdout.
    pub fn execute(&self, config_path: &str) -> Result<(), Error> {
        self.execute_to(config_path, &mut std::io::stdout())
    }

    /// Like [`Impl::execute`] but writes the report to `out`.
    pub fn execute_to(&self, config_path: &str, out: &mut dyn Write) -> Result<(), Error> {
        let p = (self.load)(config_path).map_err(|e| -> Error { format!("✗ {config_path}: {e}").into() })?;
        let known = (self.known_ops)();
        let issues = check(&p, &known);

        let (mut errors, mut warnings) = (0usize, 0usize);
        for iss in &issues {
            let _ = writeln!(out, "{iss}");
            if iss.severity == "error" {
                errors += 1;
            } else {
                warnings += 1;
            }
        }

        let cmds = p.commands.len();
        let globals = p.globals.bindings.len();
        let catch = usize::from(p.catch.is_some());

        if errors == 0 {
            let _ = writeln!(
                out,
                "✓ {config_path}: {cmds} command{}, {catch} catch, {globals} binding{} — {}",
                plural(cmds),
                plural(globals),
                summary(warnings)
            );
            return Ok(());
        }
        let _ = writeln!(
            out,
            "✗ {config_path}: {errors} error{}, {warnings} warning{}",
            plural(errors),
            plural(warnings)
        );
        Err("validation failed".into())
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn summary(warnings: usize) -> String {
    if warnings == 0 {
        "no issues".to_string()
    } else {
        format!("{warnings} warning{}", plural(warnings))
    }
}

/// The pure validation entry point.
pub fn check(p: &Program, known_ops: &HashSet<String>) -> Vec<Issue> {
    let mut v = Checker { prog: p, ops: known_ops, issues: Vec::new() };
    v.run();
    // Stable: errors first, then by location.
    v.issues.sort_by(|a, b| {
        let (ea, eb) = (a.severity == "error", b.severity == "error");
        if a.severity != b.severity {
            return eb.cmp(&ea);
        }
        a.where_.cmp(&b.where_)
    });
    v.issues
}

struct Checker<'a> {
    prog: &'a Program,
    ops: &'a HashSet<String>,
    issues: Vec<Issue>,
}

type Known = HashSet<String>;

fn valid_type(t: &str) -> bool {
    matches!(t, "string" | "int" | "float" | "bool")
}

/// Capability categories a `hooks` line may target (besides an op kind or "any").
fn valid_hook_target(t: &str) -> bool {
    matches!(t, "exec" | "net" | "write" | "read" | "env" | "any")
}

fn provided() -> Known {
    provided_var_names().into_iter().map(String::from).collect()
}

impl Checker<'_> {
    fn add_err(&mut self, where_: &str, msg: String) {
        self.issues.push(Issue { severity: "error".into(), where_: where_.into(), message: msg });
    }
    fn add_warn(&mut self, where_: &str, msg: String) {
        self.issues.push(Issue { severity: "warning".into(), where_: where_.into(), message: msg });
    }

    fn run(&mut self) {
        let prog = self.prog;
        for cmd in prog.commands.values() {
            self.check_command(cmd);
        }
        if let Some(c) = &prog.catch {
            self.check_catch(c);
        }
        self.check_hooks();
    }

    fn check_hooks(&mut self) {
        let prog = self.prog;
        for h in &prog.hooks {
            let where_ = format!("hook {}", go_quote(&format!("{} {}", h.timing, h.target)));
            match h.timing.as_str() {
                "before" | "after" | "on_error" => {}
                _ => self.add_err(
                    &where_,
                    format!("unknown hook timing {} (use before/after/on_error)", go_quote(&h.timing)),
                ),
            }
            if !prog.commands.contains_key(&h.handler) {
                self.add_err(&where_, format!("handler `{}` is not a command", h.handler));
            }
            if !valid_hook_target(&h.target) && !self.ops.contains(&h.target) {
                self.add_warn(&where_, format!("target `{}` is neither a capability category (exec/net/write/read/env) nor a known op kind — the hook may never fire", h.target));
            }
        }
    }

    fn check_command(&mut self, cmd: &Command) {
        let prog = self.prog;
        let where_ = cmd.name.as_str();

        if cmd.description.is_empty() && !cmd.modifiers.test && !cmd.modifiers.private {
            self.add_warn(where_, "no description (won't show up nicely in --help)".into());
        }

        let mut seen_names: HashSet<&str> = HashSet::new();
        let mut seen_idx: HashMap<i64, &str> = HashMap::new();
        for (arg_idx, a) in cmd.args.iter().enumerate() {
            let arg_where = format!("{} arg {}", cmd.name, go_quote(&a.name));
            if a.name.is_empty() {
                self.add_err(where_, "arg with empty name".into());
                continue;
            }
            if seen_names.contains(a.name.as_str()) {
                self.add_err(&arg_where, "duplicate arg name".into());
            }
            seen_names.insert(&a.name);

            if a.ty.is_empty() {
                self.add_err(&arg_where, "missing `type` field".into());
            } else if !valid_type(&a.ty) {
                self.add_err(&arg_where, format!("unknown type {} (use string/int/float/bool)", go_quote(&a.ty)));
            } else if a.has_default && !default_matches_type(&a.ty, &a.default) {
                self.add_err(
                    &arg_where,
                    format!("default {} does not match type {}", to_string_value(&a.default), go_quote(&a.ty)),
                );
            }
            if let Some(idx) = a.index {
                if idx < 0 {
                    self.add_err(&arg_where, "index must be ≥ 0".into());
                }
                if let Some(other) = seen_idx.get(&idx) {
                    self.add_err(&arg_where, format!("index {idx} collides with arg {}", go_quote(other)));
                }
                seen_idx.insert(idx, &a.name);
            }
            if a.rest {
                if arg_idx != cmd.args.len() - 1 {
                    self.add_err(&arg_where, "`rest` arg must be the last declared arg".into());
                }
                if a.index.is_none() {
                    self.add_err(&arg_where, "`rest` arg must be positional (declare `index N`)".into());
                }
                if !a.ty.is_empty() && a.ty != "string" {
                    self.add_err(&arg_where, format!("`rest` arg must be type \"string\" (got {})", go_quote(&a.ty)));
                }
                if a.has_default {
                    self.add_err(&arg_where, "`rest` arg cannot have a default".into());
                }
            }
        }

        let h = &cmd.modifiers.on_signal;
        if !h.is_empty() && !prog.commands.contains_key(h) {
            self.add_err(where_, format!("on_signal handler {} is not a declared command", go_quote(h)));
        }

        let mut known = provided();
        if cmd.modifiers.proxy_args {
            known.insert("proxy_args".into());
        }
        for a in &cmd.args {
            known.insert(a.name.clone());
            if a.rest {
                known.insert(format!("{}_count", a.name));
            }
        }
        for g in &prog.globals.bindings {
            known.insert(g.name.clone());
        }
        for k in cmd.env.keys() {
            known.insert(k.clone());
        }
        self.check_markers(&cmd.ops, where_, false);
        self.check_ops(&cmd.ops, where_, known);
    }

    fn check_catch(&mut self, ca: &Catch) {
        let mut known = provided();
        known.insert(ca.bind.clone());
        known.insert("proxy_args".into());
        for g in &self.prog.globals.bindings {
            known.insert(g.name.clone());
        }
        self.check_markers(&ca.ops, "catch", false);
        self.check_ops(&ca.ops, "catch", known);
    }

    /// R02: the `_catch` / `_finally` dividers are only meaningful directly
    /// inside a `try` body, at most once each, `rescue` before `finally`. (The
    /// grammar guarantees this for source programs; this guards hand-built
    /// ones and any future lowering.)
    fn check_markers(&mut self, ops: &[Op], where_: &str, in_try: bool) {
        let (mut catches, mut finallies) = (0usize, 0usize);
        for op in ops {
            match op.kind.as_str() {
                "_catch" | "_finally" if !in_try => self.add_err(
                    where_,
                    format!(
                        "`{}` divider outside a `try` block — `finally` belongs in `try … finally … end` or at the end of a command's `do … finally … end`",
                        op.kind.trim_start_matches('_')
                    ),
                ),
                "_catch" => {
                    catches += 1;
                    if catches > 1 || finallies > 0 {
                        self.add_err(where_, "`rescue` must appear once, before `finally`".into());
                    }
                }
                "_finally" => {
                    finallies += 1;
                    if finallies > 1 {
                        self.add_err(where_, "a `try` block can have only one `finally` section".into());
                    }
                }
                _ => {}
            }
            if !op.body.is_empty() {
                self.check_markers(&op.body, where_, op.kind == "try");
            }
        }
    }

    /// `known` is the set of names available for `${...}` resolution at this
    /// point in the op list; `let` captures are added as we walk.
    fn check_ops(&mut self, ops: &[Op], where_: &str, mut known: Known) {
        let prog = self.prog;
        for op in ops {
            if !self.ops.contains(&op.kind) {
                if op.kind == "_template_call" {
                    let name = op.args.get("name").and_then(Value::as_str).unwrap_or("");
                    self.add_err(where_, format!("`{name}` — no such template (check imports + spelling)"));
                } else if !prog.requirements.bin_allowed(&op.kind) {
                    self.add_err(
                        where_,
                        format!("`{}` is not a known op and not a declared bin in `requires`", op.kind),
                    );
                }
            }
            if op.kind == "run" {
                if let Some(t) = op.args.get("target").and_then(Value::as_str) {
                    if !prog.commands.contains_key(t) {
                        self.add_err(where_, format!("`run {t}` — no such command"));
                    }
                }
            }
            if prog.requirements.declared {
                self.check_requires_usage(op, where_);
            }
            for v in op.args.values() {
                if let Value::String(s) = v {
                    for name in placeholders(s) {
                        if name.is_empty() || known.contains(name) {
                            continue;
                        }
                        if looks_like_env(name) {
                            continue;
                        }
                        self.add_err(where_, format!("unknown placeholder ${{{name}}} in op {}", go_quote(&op.kind)));
                    }
                }
            }
            if let Some(ep) = op.args.get("env_prefix") {
                self.check_env_prefix(op, ep, where_, &known);
            }
            if !op.capture_into.is_empty() {
                known.insert(op.capture_into.clone());
            }
            if !op.body.is_empty() {
                let mut inner = known.clone();
                if op.kind == "for_each" {
                    if let Some(Value::String(name)) = op.args.get("_1") {
                        if !name.is_empty() {
                            inner.insert(name.clone());
                        }
                    }
                }
                self.check_ops(&op.body, where_, inner);
            }
        }
    }

    /// R05: an inline env prefix (`env_prefix` arg) belongs on a declared-bin
    /// call / `exec` only (decision D2), names must be valid identifiers, values
    /// strings whose `${REF}`s resolve like any op arg. A host env var the file
    /// did not declare is a warning here (the runtime refuses it with
    /// `env_not_declared` unless `--env` allows it).
    fn check_env_prefix(&mut self, op: &Op, ep: &Value, where_: &str, known: &Known) {
        if op.kind != "exec" {
            self.add_err(
                where_,
                format!(
                    "env prefix (NAME=value before the call) is only valid on a declared-bin call or `exec`, not on the built-in op {}",
                    go_quote(&op.kind)
                ),
            );
        }
        let Value::Object(m) = ep else {
            self.add_err(where_, "malformed env prefix: expected NAME=value assignments".into());
            return;
        };
        for (name, v) in m {
            let valid = name.bytes().enumerate().all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()));
            if name.is_empty() || !valid {
                self.add_err(
                    where_,
                    format!("malformed env assignment {}: the name before `=` must match [A-Za-z_][A-Za-z0-9_]*", go_quote(name)),
                );
            }
            let Value::String(s) = v else {
                self.add_err(where_, format!("env prefix {name}: value must be a string"));
                continue;
            };
            for r in placeholders(s) {
                if r.is_empty() || known.contains(r) {
                    continue;
                }
                if looks_like_env(r) {
                    if self.prog.requirements.declared && !env_declared(&self.prog.requirements, r) {
                        self.add_warn(
                            where_,
                            format!("env prefix {name}=${{{r}}} reads host env {r:?} which is not declared in `requires` (add `env {r:?}`) — the run is refused with env_not_declared unless `--env {r}` allows it"),
                        );
                    }
                    continue;
                }
                self.add_err(where_, format!("unknown placeholder ${{{r}}} in env prefix {name}"));
            }
        }
    }

    /// Statically verifies one op against the file's `requires` manifest. Only
    /// literal args (no `${...}`) are checked.
    fn check_requires_usage(&mut self, op: &Op, where_: &str) {
        let prog = self.prog;
        let r = &prog.requirements;
        match op.kind.as_str() {
            "shell" | "shell_output" | "shell_detached" => {
                let cmd = arg_str(op, &["cmd", "_0"]);
                if cmd.is_empty() || cmd.contains("${") {
                    return;
                }
                let bin = first_shell_token(cmd);
                if bin.is_empty() || is_shell_builtin(&bin) || bin_declared(r, &bin) {
                    return;
                }
                let q = go_quote(&bin);
                self.add_err(
                    where_,
                    format!("shell uses bin {q} which is not declared in `requires` (add `bin {q}` or run with the bin allowed)"),
                );
            }
            "http_get" | "http_post" | "http_put" | "http_delete" | "download" => {
                let url = arg_str(op, &["url", "_0"]);
                if url.is_empty() || url.contains("${") {
                    return;
                }
                let host = host_from_url(url);
                if host.is_empty() || host_declared(r, &host) {
                    return;
                }
                let q = go_quote(&host);
                self.add_err(
                    where_,
                    format!("{} targets host {q} which is not declared in `requires` (add `host {q}`)", op.kind),
                );
            }
            "get_env" => {
                let name = arg_str(op, &["name", "_0"]);
                if name.is_empty() || name.contains("${") || env_declared(r, name) {
                    return;
                }
                let q = go_quote(name);
                self.add_err(
                    where_,
                    format!("get_env reads {q} which is not declared in `requires` (add `env {q}`)"),
                );
            }
            "mkdir" | "rm" | "touch" | "chmod" | "write_file" | "append_file" | "append_line" | "ensure_dir"
            | "make_executable" | "ensure_line_in_file" | "replace_in_file" => {
                self.check_path_arg(where_, op, "write", arg_str(op, &["path", "_0"]));
            }
            "cp" | "mv" | "copy_dir" => {
                self.check_path_arg(where_, op, "read", arg_str(op, &["src", "_0"]));
                self.check_path_arg(where_, op, "write", arg_str(op, &["dst", "_1"]));
            }
            "read_file" | "exists" | "is_dir" | "is_file" | "file_size" | "list_dir" | "read_link" | "sha256_file"
            | "md5_file" => {
                self.check_path_arg(where_, op, "read", arg_str(op, &["path", "_0"]));
            }
            _ => {}
        }
    }

    /// Flags a literal fs path outside the declared read/write roots.
    fn check_path_arg(&mut self, where_: &str, op: &Op, mode: &str, p: &str) {
        if p.is_empty() || p.contains("${") {
            return;
        }
        let prog = self.prog;
        let r = &prog.requirements;
        if mode == "write" {
            if !path_in_roots(p, &r.write_roots) {
                self.add_err(
                    where_,
                    format!(
                        "{} writes {} which is outside every declared `write` root (add `write \"{p}\"`)",
                        op.kind,
                        go_quote(p)
                    ),
                );
            }
            return;
        }
        if !path_in_roots(p, &r.read_roots) && !path_in_roots(p, &r.write_roots) {
            self.add_err(
                where_,
                format!(
                    "{} reads {} which is outside every declared `read` root (add `read \"{p}\"`)",
                    op.kind,
                    go_quote(p)
                ),
            );
        }
    }
}

/// Textual prefix check (the validator can't canonicalize against a runtime cwd).
fn path_in_roots(p: &str, roots: &[String]) -> bool {
    let clean = p.strip_prefix("./").unwrap_or(p);
    roots.iter().any(|root| {
        let r = root.strip_prefix("./").unwrap_or(root);
        clean == r || clean.starts_with(&format!("{r}/"))
    })
}

/// The first present string arg among `names`.
fn arg_str<'a>(op: &'a Op, names: &[&str]) -> &'a str {
    for n in names {
        if let Some(Value::String(s)) = op.args.get(*n) {
            return s;
        }
    }
    ""
}

/// Basename of the first non-assignment token of a shell command.
fn first_shell_token(raw: &str) -> String {
    for f in raw.split_whitespace() {
        if let Some(eq) = f.find('=') {
            if eq > 0 {
                let before = &f[..eq];
                if before == before.to_uppercase() {
                    continue;
                }
            }
        }
        let base = match f.rfind(['/', '\\']) {
            Some(i) => &f[i + 1..],
            None => f,
        };
        return base.to_string();
    }
    String::new()
}

fn is_shell_builtin(name: &str) -> bool {
    matches!(name, "echo" | "cd" | "true" | "false" | "pwd" | ":" | "set" | "unset" | "export" | "test" | "[")
}

/// Hostname of a literal URL; "" if none can be parsed.
fn host_from_url(u: &str) -> String {
    let mut s = u;
    if let Some(i) = s.find("://") {
        s = &s[i + 3..];
    }
    if let Some(i) = s.find(['/', '?', '#']) {
        s = &s[..i];
    }
    if let Some(i) = s.rfind('@') {
        s = &s[i + 1..];
    }
    if let Some(i) = s.rfind(':') {
        s = &s[..i];
    }
    s.to_lowercase()
}

fn bin_declared(r: &Requirements, bin: &str) -> bool {
    r.bins.iter().any(|b| b.name == bin)
}

fn host_declared(r: &Requirements, host: &str) -> bool {
    let host = host.to_lowercase();
    r.hosts.iter().any(|h| {
        let want = h.name.to_lowercase();
        want == host || (want.starts_with("*.") && host.ends_with(&want[1..]))
    })
}

fn env_declared(r: &Requirements, name: &str) -> bool {
    r.envs.iter().any(|e| e.name == name)
}

/// Every `${name}` placeholder in `s` (`[A-Za-z_][A-Za-z_0-9]*`), except those
/// escaped with a leading backslash.
fn placeholders(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'$' && b[i + 1] == b'{' {
            let start = i + 2;
            let mut j = start;
            if j < b.len() && (b[j].is_ascii_alphabetic() || b[j] == b'_') {
                j += 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if j < b.len() && b[j] == b'}' {
                    if !(i > 0 && b[i - 1] == b'\\') {
                        out.push(&s[start..j]);
                    }
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// Convention: an uppercase identifier likely refers to a host env var.
fn looks_like_env(name: &str) -> bool {
    name.to_uppercase() == name
}

fn go_parse_bool(s: &str) -> bool {
    matches!(s, "1" | "t" | "T" | "TRUE" | "true" | "True" | "0" | "f" | "F" | "FALSE" | "false" | "False")
}

fn go_parse_float(s: &str) -> bool {
    match s.parse::<f64>() {
        Ok(x) => {
            if x.is_infinite() {
                let l = s.to_ascii_lowercase();
                l.contains("inf")
            } else {
                true
            }
        }
        Err(_) => false,
    }
}

fn default_matches_type(t: &str, v: &Value) -> bool {
    // Defaults are normalized to a STRING by the grammar, so type-checking is
    // parse-based.
    if let Value::String(s) = v {
        return match t {
            "string" => true,
            "int" => s.parse::<i64>().is_ok(),
            "float" => go_parse_float(s),
            "bool" => go_parse_bool(s),
            _ => false,
        };
    }
    match t {
        "bool" => v.is_boolean(),
        "int" => match v {
            Value::Number(n) => {
                n.is_i64() || n.is_u64() || n.as_f64().is_some_and(|x| x == (x as i64) as f64)
            }
            _ => false,
        },
        "float" => v.is_number(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_domain::*;
    use serde_json::json;

    fn known(kinds: &[&str]) -> HashSet<String> {
        kinds.iter().map(|s| s.to_string()).collect()
    }
    fn op(kind: &str, args: Value) -> Op {
        Op { kind: kind.into(), args: args.as_object().cloned().unwrap_or_default(), ..Default::default() }
    }
    fn prog1(cmd: Command) -> Program {
        let mut p = Program::default();
        p.commands.insert(cmd.name.clone(), cmd);
        p
    }
    fn cmd(ops: Vec<Op>) -> Command {
        Command { name: "x".into(), description: "d".into(), ops, ..Default::default() }
    }
    fn has_err(issues: &[Issue], sub: &str) -> bool {
        issues.iter().any(|i| i.severity == "error" && i.message.contains(sub))
    }
    fn n_err(issues: &[Issue]) -> usize {
        issues.iter().filter(|i| i.severity == "error").count()
    }

    #[test]
    fn clean_program() {
        let mut p = Program::default();
        p.commands.insert(
            "build".into(),
            Command {
                name: "build".into(),
                description: "Build the binary".into(),
                args: vec![
                    ArgSpec { name: "target".into(), ty: "string".into(), default: json!("darwin"), has_default: true, ..Default::default() },
                    ArgSpec { name: "path".into(), ty: "string".into(), index: Some(0), ..Default::default() },
                ],
                modifiers: Modifiers { on_signal: "cleanup".into(), ..Default::default() },
                ops: vec![
                    op("print", json!({"msg": "Building for ${target} into ${path}"})),
                    op("shell", json!({"cmd": "echo ${target} ${HOME}"})),
                ],
                ..Default::default()
            },
        );
        p.commands.insert(
            "cleanup".into(),
            Command { name: "cleanup".into(), description: "cleanup".into(), modifiers: Modifiers { private: true, ..Default::default() }, ..Default::default() },
        );
        assert_eq!(n_err(&check(&p, &known(&["print", "shell"]))), 0);
    }

    #[test]
    fn unknown_op() {
        let p = prog1(cmd(vec![op("blorp", json!({}))]));
        assert!(has_err(&check(&p, &known(&["print"])), "not a known op"));
    }

    #[test]
    fn unknown_placeholder() {
        let p = prog1(cmd(vec![op("print", json!({"msg": "hi ${nope}"}))]));
        assert!(has_err(&check(&p, &known(&["print"])), "${nope}"));
    }

    #[test]
    fn proxy_args_on_command() {
        let mut c = cmd(vec![op("shell", json!({"cmd": "python3 main.py ${proxy_args}"}))]);
        c.modifiers.proxy_args = true;
        assert_eq!(n_err(&check(&prog1(c), &known(&["shell"]))), 0);
    }

    #[test]
    fn env_looking_placeholder_passes() {
        let p = prog1(cmd(vec![op("print", json!({"msg": "PATH=${PATH}"}))]));
        assert_eq!(n_err(&check(&p, &known(&["print"]))), 0);
    }

    #[test]
    fn run_target_missing() {
        let p = prog1(cmd(vec![op("run", json!({"target": "ghost"}))]));
        assert!(has_err(&check(&p, &known(&["run"])), "run ghost"));
    }

    #[test]
    fn on_signal_missing() {
        let mut c = cmd(vec![op("print", json!({"msg": "hi"}))]);
        c.modifiers.on_signal = "ghost".into();
        assert!(has_err(&check(&prog1(c), &known(&["print"])), "on_signal"));
    }

    #[test]
    fn arg_validation() {
        let cases = [
            (ArgSpec { name: "a".into(), ty: "strring".into(), ..Default::default() }, "unknown type"),
            (ArgSpec { name: "a".into(), ty: "int".into(), default: json!("hello"), has_default: true, ..Default::default() }, "does not match"),
            (ArgSpec { name: "a".into(), ..Default::default() }, "missing `type`"),
        ];
        for (arg, want) in cases {
            let mut c = cmd(vec![op("print", json!({"msg": "hi"}))]);
            c.args = vec![arg];
            assert!(has_err(&check(&prog1(c), &known(&["print"])), want), "{want}");
        }
    }

    #[test]
    fn duplicate_arg_index() {
        let mut c = cmd(vec![op("print", json!({"msg": "hi"}))]);
        c.args = vec![
            ArgSpec { name: "a".into(), ty: "string".into(), index: Some(0), ..Default::default() },
            ArgSpec { name: "b".into(), ty: "string".into(), index: Some(0), ..Default::default() },
        ];
        assert!(has_err(&check(&prog1(c), &known(&["print"])), "collides"));
    }

    #[test]
    fn let_capture_flows() {
        let mut u = op("upper", json!({"_0": "hi"}));
        u.capture_into = "U".into();
        let p = prog1(cmd(vec![u, op("print", json!({"msg": "got ${U}"}))]));
        assert_eq!(n_err(&check(&p, &known(&["upper", "print"]))), 0);
    }

    #[test]
    fn block_op_recurses() {
        let mut i = op("if", json!({"op": "eq", "lhs": "os", "rhs": "darwin"}));
        i.body = vec![op("print", json!({"msg": "${nope}"}))];
        let p = prog1(cmd(vec![i]));
        assert!(has_err(&check(&p, &known(&["if", "print"])), "${nope}"));
    }

    #[test]
    fn requires_static_enforcement() {
        let mut c = cmd(vec![
            op("shell", json!({"cmd": "curl https://evil.example.com"})),
            Op { capture_into: "b".into(), ..op("http_get", json!({"_0": "https://untrusted.org/x"})) },
            Op { capture_into: "k".into(), ..op("get_env", json!({"_0": "AWS_SECRET_ACCESS_KEY"})) },
            op("shell", json!({"cmd": "echo ok"})),
            Op { capture_into: "c".into(), ..op("http_get", json!({"_0": "https://api.github.com/x"})) },
            Op { capture_into: "h".into(), ..op("get_env", json!({"_0": "HOME"})) },
            op("shell", json!({"cmd": "${os}"})),
        ]);
        c.name = "x".into();
        let mut p = prog1(c);
        p.requirements = Requirements {
            declared: true,
            bins: vec![BinReq { name: "echo".into(), ..Default::default() }],
            hosts: vec![HostReq { name: "api.github.com".into(), ..Default::default() }],
            envs: vec![EnvReq { name: "HOME".into(), ..Default::default() }],
            ..Default::default()
        };
        let issues = check(&p, &known(&["shell", "http_get", "get_env"]));
        assert!(has_err(&issues, "bin \"curl\""));
        assert!(has_err(&issues, "host \"untrusted.org\""));
        assert!(has_err(&issues, "\"AWS_SECRET_ACCESS_KEY\""));
        assert_eq!(n_err(&issues), 3, "{issues:?}");
    }

    #[test]
    fn requires_empty_manifest_enforcement() {
        let mut p = prog1(cmd(vec![op("shell", json!({"cmd": "curl https://anywhere.com"}))]));
        p.requirements = Requirements { declared: true, ..Default::default() };
        assert!(has_err(&check(&p, &known(&["shell"])), "not declared"));
    }

    // T-06: `finally` placement.
    #[test]
    fn t06_finally_placement() {
        let k = known(&["print", "try", "_catch", "_finally"]);
        // Misplaced: a bare `_finally` divider at command level / in a non-try block.
        let p = prog1(cmd(vec![op("print", json!({"msg": "a"})), op("_finally", json!({})), op("print", json!({"msg": "b"}))]));
        assert!(has_err(&check(&p, &k), "`finally` divider outside a `try` block"));
        let mut nested = op("print", json!({}));
        nested.body = vec![op("_finally", json!({}))];
        assert!(has_err(&check(&prog1(cmd(vec![nested])), &k), "outside a `try` block"));
        // Two finally sections in one try; rescue after finally.
        let mut t = op("try", json!({}));
        t.body = vec![op("_finally", json!({})), op("_finally", json!({}))];
        assert!(has_err(&check(&prog1(cmd(vec![t])), &k), "only one `finally`"));
        let mut t = op("try", json!({}));
        t.body = vec![op("_finally", json!({})), op("_catch", json!({}))];
        assert!(has_err(&check(&prog1(cmd(vec![t])), &k), "before `finally`"));
        // Well-formed: try body, rescue, finally; and a command-level wrap.
        let mut t = op("try", json!({}));
        t.body = vec![op("print", json!({"msg": "a"})), op("_catch", json!({})), op("_finally", json!({})), op("print", json!({"msg": "c"}))];
        assert_eq!(n_err(&check(&prog1(cmd(vec![t])), &k)), 0);
    }

    // T-31 (validate half): prefix on a built-in op; T-32: malformed name.
    #[test]
    fn t31_t32_env_prefix_misuse() {
        let k = known(&["print", "exec"]);
        let p = prog1(cmd(vec![op("print", json!({"msg": "hi", "env_prefix": {"K": "v"}}))]));
        assert!(has_err(&check(&p, &k), "only valid on a declared-bin call or `exec`"));
        for bad in ["1K", "K-V", ""] {
            let mut o = op("exec", json!({"bin": "tool", "env_prefix": {}}));
            o.args.insert("env_prefix".into(), json!({ bad: "v" }));
            let mut p = prog1(cmd(vec![o]));
            p.requirements = Requirements { declared: true, bins: vec![BinReq { name: "tool".into(), ..Default::default() }], ..Default::default() };
            assert!(has_err(&check(&p, &k), "malformed env assignment"), "{bad}");
        }
        // A good prefix: binding ref and declared env are clean; undeclared env warns.
        let mut o = op("exec", json!({"bin": "tool", "env_prefix": {"A": "${cfg}", "B": "${DECLARED}", "C": "${SECRET}"}}));
        o.capture_into = String::new();
        let mut c = cmd(vec![o]);
        c.env.insert("cfg".into(), "x".into());
        let mut p = prog1(c);
        p.requirements = Requirements {
            declared: true,
            bins: vec![BinReq { name: "tool".into(), ..Default::default() }],
            envs: vec![EnvReq { name: "DECLARED".into(), ..Default::default() }],
            ..Default::default()
        };
        let issues = check(&p, &k);
        assert_eq!(n_err(&issues), 0, "{issues:?}");
        assert!(issues.iter().any(|i| i.severity == "warning" && i.message.contains("SECRET")));
        assert!(!issues.iter().any(|i| i.message.contains("DECLARED")));
    }

    #[test]
    fn placeholder_scanner() {
        assert_eq!(placeholders("a ${x} \\${y} ${${z} ${1a}"), vec!["x", "z"]);
    }

    #[test]
    fn report_text() {
        let imp = Impl {
            load: Box::new(|_| Ok(prog1(cmd(vec![op("print", json!({"msg": "hi"}))])))),
            known_ops: Box::new(|| known(&["print"])),
        };
        let mut out = Vec::new();
        imp.execute_to("f.perch", &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "✓ f.perch: 1 command, 0 catch, 0 bindings — no issues\n");
    }
}
