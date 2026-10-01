//! Audits a perch program for what it actually needs and what posture it could
//! be run under safely. Static analysis only — no execution. Produces:
//!
//!   - CAPABILITIES NEEDED: shell? subprocess? network? writes? — with the
//!     specific binaries / hosts / paths it touches.
//!   - ENV VARS REFERENCED: every ${UPPER_SNAKE} in any string arg.
//!   - RISK FINDINGS: a ranked list of patterns worth a human's attention
//!     (sudo, shell injection on unvalidated args, catch-→shell passthroughs,
//!     downloads + chmod + exec, etc.).
//!   - RECOMMENDED INVOCATION: the tightest CLI flag combination that should
//!     still let the script run.
//!
//! The goal is to make reviewing a stranger's .perch file (or your own, before
//! shipping it) something you do in ~30 seconds instead of ~30 minutes.
use perch_domain::{Op, Program};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::OnceLock;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type LoadFn = Box<dyn Fn(&str) -> Result<Program, Error>>;

pub struct Impl {
    pub load: LoadFn,
}

impl Impl {
    /// Loads, analyzes and prints the report to `out`. `format` is `"text"`
    /// (or empty) for the human report, `"json"` for the structured one.
    pub fn execute(&self, path: &str, format: &str, out: &mut dyn Write) -> Result<(), Error> {
        if !matches!(format, "" | "text" | "json") {
            return Err(format!("unknown scan format {format:?} (want text or json)").into());
        }
        let p = (self.load)(path)?;
        let r = analyze(&p);
        if format == "json" {
            out.write_all(json_report(&p, path, &r).as_bytes())?;
        } else {
            print_report(out, &p, path, &r)?;
        }
        Ok(())
    }
}

/// The result of analyzing a program. Maps record how many times each thing
/// appears, which makes "you have one shell call to git and twelve to docker"
/// actionable rather than a binary yes/no. (Sorted maps: Go sorts keys before
/// every use.)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub needs_shell: bool,
    /// bash first-token → count
    pub shell_bins: BTreeMap<String, usize>,
    pub has_shell_sudo: bool,
    /// Any shell has |, >, $(, ;, &&, ...
    pub has_shell_pipe: bool,
    pub needs_subprocess: bool,
    pub subprocess_ops: BTreeMap<String, usize>,
    pub needs_network: bool,
    pub hosts: BTreeMap<String, usize>,
    pub needs_write: bool,
    pub write_roots: BTreeMap<String, usize>,
    pub needs_read: bool,
    pub read_roots: BTreeMap<String, usize>,
    /// The file's `requires` block declares a write scope / hosts / read
    /// scope. A capability covered by a declared scope is not advised away.
    pub declared_write: bool,
    pub declared_network: bool,
    pub declared_read: bool,
    pub env_vars: BTreeMap<String, usize>,
    /// Declared-bin / `exec` call bin → count (subprocess class).
    pub exec_bins: BTreeMap<String, usize>,
    /// One entry per `exec` op with an `env_prefix`: (bin, names in source order).
    pub env_prefix: Vec<(String, Vec<String>)>,
    pub has_proxy_args: bool,
    pub has_catch: bool,
    /// Catch contains a shell op (open passthrough).
    pub catch_forwards: bool,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Finding {
    /// "high" | "med" | "low" | "info"
    pub severity: String,
    pub where_: String,
    pub issue: String,
    pub fix: String,
}

/// Walks the program and produces a [`Report`]. Pure — no IO.
pub fn analyze(p: &Program) -> Report {
    let mut r = Report {
        declared_write: !p.requirements.write_roots.is_empty(),
        declared_network: !p.requirements.hosts.is_empty(),
        declared_read: !p.requirements.read_roots.is_empty() || !p.requirements.write_roots.is_empty(),
        ..Default::default()
    };

    if let Some(catch) = &p.catch {
        r.has_catch = true;
        walk_ops(&catch.ops, "catch", &mut r, true);
    }
    // BTreeMap iterates commands sorted, for deterministic finding order.
    for (n, c) in &p.commands {
        if c.modifiers.proxy_args {
            r.has_proxy_args = true;
        }
        walk_ops(&c.ops, &format!("command {n}"), &mut r, false);
    }
    // Globals' string values can reference env vars too.
    for g in &p.globals.bindings {
        if let Value::String(s) = &g.value {
            record_env(s, &mut r);
        }
    }
    r
}

/// The recursive scanner. `in_catch` flags catch-block context so we can report
/// "catch forwards to shell" as its own finding.
fn walk_ops(ops: &[Op], where_: &str, r: &mut Report, in_catch: bool) {
    for (i, op) in ops.iter().enumerate() {
        let op_where = format!("{} op #{} ({})", where_, i + 1, op.kind);

        // Harvest env-var references from every string arg.
        for v in op.args.values() {
            if let Value::String(s) = v {
                record_env(s, r);
            }
        }

        match op.kind.as_str() {
            "shell" | "shell_output" | "shell_detached" | "shell_in" | "try_shell" => {
                r.needs_shell = true;
                if in_catch {
                    r.catch_forwards = true;
                }
                classify_shell(op, r);
            }
            "exec" => {
                // Declared-bin call (`docker ps`, `K=v tool a`): a subprocess, not a
                // shell. Same sudo rule as shell bins; env_prefix is reported.
                r.needs_subprocess = true;
                *r.subprocess_ops.entry("exec".to_string()).or_default() += 1;
                let bin = op.args.get("bin").and_then(|v| v.as_str()).unwrap_or("");
                let base = bin.rsplit('/').next().unwrap_or(bin);
                if !base.is_empty() {
                    *r.exec_bins.entry(base.to_string()).or_default() += 1;
                }
                if base == "sudo" {
                    r.has_shell_sudo = true;
                }
                if let Some(Value::Object(m)) = op.args.get("env_prefix") {
                    r.env_prefix.push((base.to_string(), m.keys().cloned().collect()));
                    for v in m.values() {
                        if let Value::String(s) = v {
                            record_env(s, r);
                        }
                    }
                }
            }
            "pkg_install" | "pkg_uninstall" | "kill_by_name" | "process_running" | "bin_version" | "os_version" => {
                r.needs_subprocess = true;
                *r.subprocess_ops.entry(op.kind.clone()).or_default() += 1;
            }
            "http_get" | "http_post" | "http_put" | "http_delete" | "http_status" | "download" => {
                r.needs_network = true;
                record_host(op, r);
            }
            "dns_lookup" | "port_check" | "port_free" | "find_free_port" | "wait_for_url" | "wait_for_port"
            | "public_ip" | "local_ip" | "mac_address" | "interfaces" => {
                r.needs_network = true;
            }
            "write_file" | "append_file" | "append_line" | "ensure_line_in_file" | "replace_in_file"
            | "backup_file" | "cp" | "mv" | "rm" | "mkdir" | "chmod" | "touch" | "copy_dir" | "ensure_dir"
            | "make_executable" | "symlink" | "link_into_path" | "mktemp_file" | "mktemp_dir" | "add_to_path"
            | "tar_create" | "tar_extract" | "gzip" | "ungzip" | "zip_create" | "zip_extract"
            | "bundle_extract" | "bundle_dir" => {
                r.needs_write = true;
                record_write(op, r);
            }
            "read_file" | "read_link" | "list_dir" | "glob" | "sha256_file" => {
                r.needs_read = true;
                let p = first_string_arg(op, &["path", "pattern", "_0", "_1"]);
                if !p.is_empty() {
                    *r.read_roots.entry(path_root(p)).or_default() += 1;
                }
            }
            _ => {}
        }

        // Risk findings checked after classification so the message can
        // reference the same op_where string.
        check_risks(op, &op_where, r, in_catch);

        // Recurse into block bodies.
        if !op.body.is_empty() {
            walk_ops(&op.body, where_, r, in_catch);
        }
    }
}

/// Inspects a shell op's command-line for binary + risk patterns.
fn classify_shell(op: &Op, r: &mut Report) {
    let cmd = first_string_arg(op, &["cmd", "_0", "_1"]);
    if cmd.is_empty() {
        return;
    }
    let bin = first_shell_token(cmd);
    if !bin.is_empty() {
        *r.shell_bins.entry(bin.to_string()).or_default() += 1;
    }
    if bin == "sudo" || cmd.trim().starts_with("sudo ") {
        r.has_shell_sudo = true;
    }
    if ["|", ">", "<", "&", ";", "`", "$("].iter().any(|ch| cmd.contains(ch)) {
        r.has_shell_pipe = true;
    }
}

/// Extracts host from a URL-like arg.
fn record_host(op: &Op, r: &mut Report) {
    let url = first_string_arg(op, &["url", "_0"]);
    if url.is_empty() {
        return;
    }
    if let Some(h) = extract_host(url) {
        *r.hosts.entry(h.to_string()).or_default() += 1;
    }
}

/// Captures the target path-or-root of a write op.
fn record_write(op: &Op, r: &mut Report) {
    let p = first_string_arg(op, &["path", "dst", "link", "_0", "_1"]);
    if p.is_empty() {
        return;
    }
    *r.write_roots.entry(path_root(p)).or_default() += 1;
}

fn push(r: &mut Report, severity: &str, where_: &str, issue: &str, fix: &str) {
    r.findings.push(Finding {
        severity: severity.into(),
        where_: where_.into(),
        issue: issue.into(),
        fix: fix.into(),
    });
}

fn check_risks(op: &Op, where_: &str, r: &mut Report, in_catch: bool) {
    match op.kind.as_str() {
        "shell" | "shell_output" | "shell_detached" | "shell_in" | "try_shell" => {
            let cmd = first_string_arg(op, &["cmd", "_0", "_1"]);
            if cmd.contains("sudo ") {
                push(
                    r,
                    "high",
                    where_,
                    "shell command uses `sudo` (privilege escalation)",
                    "drop sudo, or guard with `if is_admin ... end` and run perch itself elevated",
                );
            }
            if in_catch && cmd.contains("${proxy_args}") {
                push(
                    r,
                    "med",
                    where_,
                    "catch forwards `${proxy_args}` to a shell — any input becomes a shell command",
                    "intentional for `extend an existing tool` patterns; document it and pin `--allow-bin` to the wrapped binary only",
                );
            }
            // Crude shell-injection heuristic: a non-validated ${var} inside a
            // shell string is hard to bound. Flag as low — the user often has
            // done validation we can't see.
            if has_unvalidated_interp(cmd) {
                push(
                    r,
                    "low",
                    where_,
                    "shell command interpolates `${var}` with no obvious validation",
                    "add a `regex_match` guard, or promote to a native op (which receives args structurally and can't be shell-injected)",
                );
            }
        }
        "make_executable" => {
            push(
                r,
                "med",
                where_,
                "`make_executable` flips the +x bit — downstream `shell` could then run unverified code",
                "pair with `verify_sha256` against a known hash before flipping +x",
            );
        }
        _ => {}
    }
}

// ─── helpers ─────────────────────────────────────────────────────────

fn first_string_arg<'a>(op: &'a Op, names: &[&str]) -> &'a str {
    for n in names {
        if let Some(Value::String(s)) = op.args.get(*n) {
            return s;
        }
    }
    ""
}

/// Returns the basename of the first non-env-assignment token. Mirrors the
/// runtime --allow-bin matcher so the suggestion is directly actionable.
fn first_shell_token(s: &str) -> &str {
    for f in s.split_whitespace() {
        if f.contains('=') {
            continue; // FOO=bar style assignment, skip
        }
        // Strip leading ./ or path prefix.
        if let Some(idx) = f.rfind(['/', '\\']) {
            return &f[idx + 1..];
        }
        return f;
    }
    ""
}

struct Res {
    url_host: Regex,
    env_var: Regex,
    interp: Regex,
}

fn res() -> &'static Res {
    static R: OnceLock<Res> = OnceLock::new();
    R.get_or_init(|| Res {
        url_host: Regex::new(r"^[a-z]+://([^/:?#]+)").unwrap(),
        env_var: Regex::new(r"\$\{([A-Z][A-Z0-9_]*)\}").unwrap(),
        interp: Regex::new(r"\$\{[a-z_][a-zA-Z0-9_]*\}").unwrap(),
    })
}

fn extract_host(url: &str) -> Option<&str> {
    res().url_host.captures(url).map(|m| m.get(1).unwrap().as_str())
}

fn record_env(s: &str, r: &mut Report) {
    for m in res().env_var.captures_iter(s) {
        *r.env_vars.entry(m[1].to_string()).or_default() += 1;
    }
}

/// Summarises a write target — full absolute paths and ${anchor}/sub paths get
/// collapsed so the report doesn't repeat the same prefix dozens of times.
/// Best-effort.
fn path_root(p: &str) -> String {
    let p = p.trim();
    if p.starts_with("${") {
        if let Some(end) = p.find('}') {
            let anchor = &p[..end + 1];
            // Include first path segment after the anchor, if any.
            let after = &p[end + 1..];
            let rest = after.strip_prefix('/').unwrap_or(after);
            if let Some(i) = rest.find('/') {
                if i > 0 {
                    return format!("{}/{}/…", anchor, &rest[..i]);
                }
            }
            if !rest.is_empty() {
                return format!("{anchor}/{rest}");
            }
            return anchor.to_string();
        }
    }
    if let Some(tail) = p.strip_prefix('/') {
        let parts: Vec<&str> = tail.splitn(3, '/').collect();
        if parts.len() >= 2 {
            return format!("/{}/{}/…", parts[0], parts[1]);
        }
        return format!("/{}", parts[0]);
    }
    p.to_string()
}

/// A quick heuristic: any lowercase ${ident} in a shell string. Real validation
/// would track guards (regex_match etc.) in the surrounding scope — out of
/// scope for this static pass.
fn has_unvalidated_interp(s: &str) -> bool {
    res().interp.is_match(s)
}

// ─── pretty-print ────────────────────────────────────────────────────

/// Writes a human-readable scan report to `w`.
pub fn print_report(w: &mut dyn Write, p: &Program, path: &str, r: &Report) -> std::io::Result<()> {
    write!(w, "\n  ── {path} ──────────────────────────────────────────────\n\n")?;
    writeln!(
        w,
        "  {} command(s), {} catch, {} binding(s)",
        p.commands.len(),
        bool_str(p.catch.is_some()),
        p.globals.bindings.len()
    )?;
    writeln!(w)?;

    // RISK SCORE — a one-glance summary for non-experts. Computed from declared
    // capabilities + finding severities. See score_report.
    let (score, reasons) = score_report(r);
    writeln!(w, "  RISK: {}", risk_badge(score))?;
    for why in &reasons {
        writeln!(w, "    · {why}")?;
    }
    writeln!(w)?;

    // CAPABILITIES
    writeln!(w, "  CAPABILITIES NEEDED")?;
    cap_line(w, "shell", r.needs_shell, &summarise_shell(r))?;
    cap_line(w, "subprocess", r.needs_subprocess, &summarise_subprocess(r))?;
    cap_line(w, "network", r.needs_network, &summarise_net(r))?;
    cap_line(w, "writes", r.needs_write, &summarise_writes(r))?;
    writeln!(w)?;

    // ENV VARS
    if !r.env_vars.is_empty() {
        writeln!(w, "  ENV VARS REFERENCED")?;
        write!(w, "    {}\n\n", keys(&r.env_vars).join(", "))?;
    }

    if !r.env_prefix.is_empty() {
        writeln!(w, "  ENV PREFIX (per-call env on declared-bin calls)")?;
        for (bin, names) in &r.env_prefix {
            writeln!(w, "    {bin}: {}", names.join(", "))?;
        }
        writeln!(w)?;
    }

    // FINDINGS
    if !r.findings.is_empty() {
        writeln!(w, "  RISK FINDINGS")?;
        for sev in ["high", "med", "low", "info"] {
            for f in &r.findings {
                if f.severity != sev {
                    continue;
                }
                let tag = sev.to_uppercase();
                writeln!(w, "    [{:<4}] {}", tag, f.where_)?;
                writeln!(w, "             {}", f.issue)?;
                if !f.fix.is_empty() {
                    writeln!(w, "         →   {}", f.fix)?;
                }
                writeln!(w)?;
            }
        }
    } else {
        writeln!(w, "  RISK FINDINGS")?;
        writeln!(w, "    (none)")?;
        writeln!(w)?;
    }

    // RECOMMENDED INVOCATION
    writeln!(w, "  RECOMMENDED INVOCATION")?;
    for line in recommended_invocation(path, r) {
        writeln!(w, "    {line}")?;
    }
    writeln!(w)?;
    writeln!(w, "  Compared to bare `perch -f {path}`:")?;
    for line in delta_summary(r) {
        writeln!(w, "    {line}")?;
    }
    writeln!(w)?;
    Ok(())
}

/// Synthesises the tightest CLI command the script should still run under.
/// Returned as a list of lines so the printer can backslash-wrap nicely.
pub fn recommended_invocation(path: &str, r: &Report) -> Vec<String> {
    let mut lines = vec!["perch \\".to_string()];
    let mut add = |s: &str| lines.push(format!("  {s} \\"));

    // Negative caps the script doesn't need.
    if !r.needs_shell {
        add("--no-shell");
    }
    if !r.needs_subprocess {
        add("--no-subprocess");
    }
    if !r.needs_network && !r.declared_network {
        add("--no-network");
    }
    if !r.needs_write && !r.declared_write {
        add("--no-write");
    }

    // Positive scoping where the script does need a thing.
    if r.needs_shell && !r.shell_bins.is_empty() && r.shell_bins.len() <= 8 {
        add(&format!("--allow-bin {}", keys(&r.shell_bins).join(",")));
    }
    if r.needs_shell && !r.has_shell_pipe {
        add("--no-shell-metachars");
    }
    if !r.env_vars.is_empty() && r.env_vars.len() <= 20 {
        add(&format!("--env {}", keys(&r.env_vars).join(",")));
    }

    // Belt-and-braces.
    add("--max-runtime 600");
    // Audit path uses the script's basename so the suggestion is portable
    // across directories (avoid leaking the user's full path).
    let mut script_name = path.strip_suffix(".perch").unwrap_or(path);
    if let Some(idx) = script_name.rfind(['/', '\\']) {
        script_name = &script_name[idx + 1..];
    }
    add(&format!("--audit /var/log/perch-{script_name}.ndjson"));
    lines.push(format!("  -f {path}"));
    lines
}

fn delta_summary(r: &Report) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if !r.needs_shell {
        out.push("- shell subprocess entirely disabled".into());
    } else if !r.shell_bins.is_empty() && r.shell_bins.len() <= 8 {
        out.push(format!("- shell pinned to {{{}}}", keys(&r.shell_bins).join(", ")));
        if !r.has_shell_pipe {
            out.push("- no pipes / redirects / `$()` allowed in shell args".into());
        }
    }
    if !r.needs_network && !r.declared_network {
        out.push("- network access entirely disabled".into());
    }
    if !r.needs_write && !r.declared_write {
        out.push("- filesystem mutation entirely disabled".into());
    }
    if !r.needs_subprocess {
        out.push("- pkg_install/kill_by_name/etc. disabled".into());
    }
    if !r.env_vars.is_empty() {
        out.push(format!(
            "- host env scoped to {} declared var(s) (rest scrubbed from subprocesses)",
            r.env_vars.len()
        ));
    }
    out.push("- 10-minute wall-clock cap; structured audit trail".into());
    out
}

fn cap_line(w: &mut dyn Write, name: &str, on: bool, detail: &str) -> std::io::Result<()> {
    let marker = if on { "✓" } else { "✗" };
    writeln!(w, "    {marker} {name:<12} {detail}")
}

fn summarise_shell(r: &Report) -> String {
    if !r.needs_shell {
        return "— add `--no-shell` for free".into();
    }
    if r.shell_bins.is_empty() {
        return "(no binary parsed)".into();
    }
    let bins = keys(&r.shell_bins);
    let mut tag = String::new();
    if r.has_shell_sudo {
        tag = "  ⚠ uses sudo".into();
    }
    if r.has_shell_pipe {
        tag.push_str("  ⚠ pipes/redirects");
    }
    format!("({} call(s), binaries: {}){}", r.shell_bins.values().sum::<usize>(), bins.join(", "), tag)
}

fn summarise_subprocess(r: &Report) -> String {
    if !r.needs_subprocess {
        return "— add `--no-subprocess` for free".into();
    }
    format!("({})", keys(&r.subprocess_ops).join(", "))
}

fn summarise_net(r: &Report) -> String {
    if !r.needs_network {
        if r.declared_network {
            return "— none seen in ops; declared `host` scope covers any use by spawned binaries".into();
        }
        return "— add `--no-network` for free".into();
    }
    if r.hosts.is_empty() {
        return "(unknown hosts — only `${var}` URLs found)".into();
    }
    format!("({} host(s): {})", r.hosts.len(), keys(&r.hosts).join(", "))
}

fn summarise_writes(r: &Report) -> String {
    if !r.needs_write {
        if r.declared_write {
            return "— none seen in ops; declared `write` scope covers any use by spawned binaries".into();
        }
        return "— add `--no-write` for free".into();
    }
    if r.write_roots.is_empty() {
        return "(paths from `${var}` only)".into();
    }
    format!("(roots: {})", keys(&r.write_roots).join(", "))
}

fn bool_str(b: bool) -> &'static str {
    if b {
        "1"
    } else {
        "0"
    }
}

/// Sorted keys (the maps are already ordered).
fn keys(m: &BTreeMap<String, usize>) -> Vec<&str> {
    m.keys().map(|s| s.as_str()).collect()
}

// ─── risk scoring ────────────────────────────────────────────────────
//
// Converts a Report into a HIGH / MED / LOW / SAFE summary that non-experts
// can read at a glance. Used in the --scan output and (eventually) in the web
// UI's Scan tab.
//
// The score is intentionally coarse:
//
//   HIGH  — at least one HIGH-severity finding (sudo, ${proxy_args} into
//           shell, unvalidated interp into shell, etc.) OR the program needs
//           both shell + network without any allowlist.
//   MED   — at least one MED-severity finding OR the program does anything in
//           two or more "powerful" categories without explicit allowlists.
//   LOW   — needs only one category (shell, network, writes), or has only
//           LOW-severity findings.
//   SAFE  — pure ops; no shell, no network, no subprocess, no writes.
//
// This is intentionally NOT a security guarantee — it's a UI affordance for
// "is this worth carefully reading before I run?"

/// The coarse classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskScore {
    Safe,
    Low,
    Med,
    High,
}

impl std::fmt::Display for RiskScore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RiskScore::Safe => "SAFE",
            RiskScore::Low => "LOW",
            RiskScore::Med => "MED",
            RiskScore::High => "HIGH",
        })
    }
}

/// Computes a coarse risk score plus the reasons that pushed it up from SAFE.
/// The reasons are human-readable strings shown to users; they're not a stable
/// API.
pub fn score_report(r: &Report) -> (RiskScore, Vec<String>) {
    let mut score = RiskScore::Safe;
    let mut reasons: Vec<String> = Vec::new();

    // Capability-driven baseline.
    let mut cats = 0;
    if r.needs_shell {
        cats += 1;
        reasons.push("executes shell".into());
    }
    if r.needs_subprocess {
        cats += 1;
        reasons.push("spawns subprocesses (pkg_install / kill / process_running)".into());
    }
    if r.needs_network {
        cats += 1;
        let n = r.hosts.len();
        let hosts = if n > 0 { format!(" ({} host{})", n, plural(n)) } else { String::new() };
        reasons.push(format!("network access{hosts}"));
    }
    if r.needs_write {
        cats += 1;
        let n = r.write_roots.len();
        let hosts = if n > 0 { format!(" ({} root{})", n, plural(n)) } else { String::new() };
        reasons.push(format!("writes the filesystem{hosts}"));
    }

    score = match cats {
        0 => score, // Stays SAFE.
        1 => RiskScore::Low,
        _ => RiskScore::Med,
    };

    // Specific high-signal patterns push the score up.
    if r.has_shell_sudo {
        score = RiskScore::High;
        reasons.push("uses `sudo` (privilege escalation)".into());
    }
    if r.has_shell_pipe {
        if score < RiskScore::Med {
            score = RiskScore::Med;
        }
        reasons.push("uses shell metacharacters (pipe / && / ; / $())".into());
    }
    if r.catch_forwards {
        score = RiskScore::High;
        reasons.push("catch-all forwards ${proxy_args} to shell (any unknown verb → shell)".into());
    }

    // Findings escalate.
    for f in &r.findings {
        match f.severity.as_str() {
            "high" => {
                if score < RiskScore::High {
                    score = RiskScore::High;
                }
            }
            "med" => {
                if score < RiskScore::Med {
                    score = RiskScore::Med;
                }
            }
            _ => {}
        }
    }

    if reasons.is_empty() {
        reasons = vec!["no privileged operations — pure ops only".into()];
    }
    (score, reasons)
}

/// Renders the score as a short colored-by-letter label. Terminal coloring is
/// intentionally NOT applied here (the caller's shell may or may not be a TTY);
/// the badge is plain text.
pub fn risk_badge(s: RiskScore) -> &'static str {
    match s {
        RiskScore::Safe => "🟢 SAFE  (pure ops — no shell, no network, no writes)",
        RiskScore::Low => "🟡 LOW   (limited surface — review the capabilities below)",
        RiskScore::Med => "🟠 MED   (multiple capabilities or shell metachars — review carefully)",
        RiskScore::High => "🔴 HIGH  (sudo / proxy_args / privileged ops — read every command before running)",
    }
}

// ─── structured (JSON) report ────────────────────────────────────────
//
// Field order in every struct below is alphabetical on purpose: serde emits
// fields in declaration order, so the output is stable and key-sorted.

/// Schema version of the JSON report. Bump on any breaking change.
pub const JSON_SCHEMA: u32 = 1;

#[derive(Debug, Serialize)]
pub struct JsonReport {
    pub declared: JsonDeclared,
    pub file: String,
    pub inferred: JsonInferred,
    pub risk: String,
    pub risk_reasons: Vec<String>,
    pub schema: u32,
}

#[derive(Debug, Serialize)]
pub struct JsonDeclared {
    pub arch: Vec<String>,
    pub bin: Vec<JsonBin>,
    /// Whether the file has a `requires` block at all.
    pub declared: bool,
    pub env: Vec<JsonNamed>,
    pub host: Vec<JsonNamed>,
    pub os: Vec<String>,
    pub read: Vec<String>,
    pub write: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct JsonBin {
    pub alias: String,
    pub hash: String,
    pub hash_file: String,
    pub name: String,
    pub optional: bool,
}

#[derive(Debug, Serialize)]
pub struct JsonNamed {
    pub name: String,
    pub optional: bool,
}

#[derive(Debug, Serialize)]
pub struct JsonInferred {
    pub catch_forwards: bool,
    pub env: Vec<String>,
    pub env_prefix: Vec<JsonEnvPrefix>,
    pub exec_bins: Vec<String>,
    pub hosts: Vec<String>,
    pub network: bool,
    pub read: bool,
    pub read_roots: Vec<String>,
    pub shell: JsonShell,
    pub subprocess: bool,
    pub subprocess_ops: Vec<String>,
    pub write: bool,
    pub write_roots: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct JsonEnvPrefix {
    pub bin: String,
    pub names: Vec<String>,
    pub op: String,
}

#[derive(Debug, Serialize)]
pub struct JsonShell {
    pub bins: Vec<String>,
    pub calls: usize,
    pub metachars: bool,
    pub sudo: bool,
}

fn owned_keys(m: &BTreeMap<String, usize>) -> Vec<String> {
    m.keys().cloned().collect()
}

fn sorted(v: &[String]) -> Vec<String> {
    let mut o = v.to_vec();
    o.sort();
    o
}

/// Risk label for the JSON report: `safe` | `low` | `med` | `high`, the same
/// classification as the text badge.
pub fn risk_label(s: RiskScore) -> &'static str {
    match s {
        RiskScore::Safe => "safe",
        RiskScore::Low => "low",
        RiskScore::Med => "med",
        RiskScore::High => "high",
    }
}

/// Builds the structured report (pure).
pub fn build_json_report(p: &Program, path: &str, r: &Report) -> JsonReport {
    let q = &p.requirements;
    let (score, reasons) = score_report(r);
    JsonReport {
        declared: JsonDeclared {
            arch: sorted(&q.arch),
            bin: q
                .bins
                .iter()
                .map(|b| JsonBin {
                    alias: b.alias.clone(),
                    hash: b.hash.clone(),
                    hash_file: b.hash_file.clone(),
                    name: b.name.clone(),
                    optional: b.optional,
                })
                .collect(),
            declared: q.declared,
            env: q.envs.iter().map(|e| JsonNamed { name: e.name.clone(), optional: e.optional }).collect(),
            host: q.hosts.iter().map(|h| JsonNamed { name: h.name.clone(), optional: h.optional }).collect(),
            os: sorted(&q.os),
            read: q.read_roots.clone(),
            write: q.write_roots.clone(),
        },
        file: path.to_string(),
        inferred: JsonInferred {
            catch_forwards: r.catch_forwards,
            env: owned_keys(&r.env_vars),
            env_prefix: r
                .env_prefix
                .iter()
                .map(|(b, n)| JsonEnvPrefix { bin: b.clone(), names: n.clone(), op: "exec".into() })
                .collect(),
            exec_bins: owned_keys(&r.exec_bins),
            hosts: owned_keys(&r.hosts),
            network: r.needs_network,
            read: r.needs_read,
            read_roots: owned_keys(&r.read_roots),
            shell: JsonShell {
                bins: owned_keys(&r.shell_bins),
                calls: r.shell_bins.values().sum(),
                metachars: r.has_shell_pipe,
                sudo: r.has_shell_sudo,
            },
            subprocess: r.needs_subprocess,
            subprocess_ops: owned_keys(&r.subprocess_ops),
            write: r.needs_write,
            write_roots: owned_keys(&r.write_roots),
        },
        risk: risk_label(score).to_string(),
        risk_reasons: reasons,
        schema: JSON_SCHEMA,
    }
}

/// Pretty-printed (2-space) JSON with a trailing newline.
pub fn json_report(p: &Program, path: &str, r: &Report) -> String {
    let mut s = serde_json::to_string_pretty(&build_json_report(p, path, r)).expect("report serializes");
    s.push('\n');
    s
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_domain::{Catch, Command, GlobalBinding};
    use serde_json::json;

    fn op(kind: &str, args: Value) -> Op {
        Op { kind: kind.into(), args: args.as_object().cloned().unwrap_or_default(), ..Default::default() }
    }

    fn prog(ops: Vec<Op>) -> Program {
        let mut p = Program::default();
        p.commands.insert("go".into(), Command { name: "go".into(), ops, ..Default::default() });
        p
    }

    #[test]
    fn pure_program_is_safe() {
        let p = prog(vec![op("print", json!({"msg": "hi"}))]);
        let r = analyze(&p);
        assert_eq!(score_report(&r), (RiskScore::Safe, vec!["no privileged operations — pure ops only".to_string()]));
        let mut out = Vec::new();
        print_report(&mut out, &p, "x/y.perch", &r).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("  1 command(s), 0 catch, 0 binding(s)\n"));
        assert!(s.contains("    ✗ shell        — add `--no-shell` for free\n"));
        assert!(s.contains("  RISK FINDINGS\n    (none)\n"));
        assert!(s.contains("    --audit /var/log/perch-y.ndjson \\\n      -f x/y.perch\n") || s.contains("--audit /var/log/perch-y.ndjson \\\n"));
    }

    #[test]
    fn shell_sudo_and_findings() {
        let mut p = prog(vec![
            op("shell", json!({"cmd": "FOO=1 /usr/bin/sudo rm -rf ${target} | tee x", "extra": "${HOME}"})),
            op("http_get", json!({"url": "https://example.com:8080/x"})),
            op("write_file", json!({"path": "${HOME}/a/b/c"})),
            op("make_executable", json!({"path": "/usr/local/bin/x"})),
        ]);
        p.catch = Some(Catch { ops: vec![op("shell", json!({"cmd": "git ${proxy_args}"}))], ..Default::default() });
        p.globals.bindings.push(GlobalBinding { name: "g".into(), ty: "string".into(), value: json!("${TOKEN}") });
        let r = analyze(&p);
        assert!(r.needs_shell && r.has_shell_pipe && r.catch_forwards && r.has_catch);
        assert_eq!(r.shell_bins.get("sudo"), Some(&1));
        assert!(r.has_shell_sudo);
        assert_eq!(r.hosts.get("example.com"), Some(&1));
        assert_eq!(r.write_roots.get("${HOME}/a/…"), Some(&1));
        assert_eq!(r.write_roots.get("/usr/local/…"), Some(&1));
        assert_eq!(r.env_vars.keys().cloned().collect::<Vec<_>>(), vec!["HOME", "TOKEN"]);
        // catch is walked first, then commands in name order.
        assert_eq!(r.findings[0].where_, "catch op #1 (shell)");
        assert_eq!(r.findings[0].severity, "med");
        assert_eq!(score_report(&r).0, RiskScore::High);
        let inv = recommended_invocation("dir/app.perch", &r);
        assert_eq!(inv[0], "perch \\");
        assert!(inv.contains(&"  --allow-bin git,sudo \\".to_string()));
        assert_eq!(inv.last().unwrap(), "  -f dir/app.perch");
        assert!(!inv.contains(&"  --no-shell-metachars \\".to_string()));
    }

    fn exec_op(bin: &str, prefix: Option<Value>, capture: &str) -> Op {
        let mut o = op("exec", json!({"bin": bin, "_0": "ps"}));
        if let Some(p) = prefix {
            o.args.insert("env_prefix".into(), p);
        }
        o.capture_into = capture.into();
        o
    }

    #[test]
    fn exec_plain_prefixed_and_capture() {
        let p = prog(vec![
            exec_op("docker", None, ""),
            exec_op("kubectl", Some(json!({"KUBECONFIG": "${CFG}", "Z": "lit"})), ""),
            exec_op("/usr/bin/sudo", None, "out"),
        ]);
        let r = analyze(&p);
        assert!(r.needs_subprocess && !r.needs_shell);
        assert_eq!(r.subprocess_ops.get("exec"), Some(&3));
        assert_eq!(r.exec_bins.keys().cloned().collect::<Vec<_>>(), vec!["docker", "kubectl", "sudo"]);
        assert_eq!(r.env_prefix, vec![("kubectl".to_string(), vec!["KUBECONFIG".to_string(), "Z".to_string()])]);
        assert_eq!(r.env_vars.keys().cloned().collect::<Vec<_>>(), vec!["CFG"]);
        assert!(r.has_shell_sudo);
        assert_eq!(score_report(&r).0, RiskScore::High);
        let v: Value = serde_json::from_str(&json_report(&p, "t.perch", &r)).unwrap();
        assert_eq!(v["schema"], 1);
        assert_eq!(v["inferred"]["exec_bins"], json!(["docker", "kubectl", "sudo"]));
        assert_eq!(
            v["inferred"]["env_prefix"],
            json!([{"op": "exec", "bin": "kubectl", "names": ["KUBECONFIG", "Z"]}])
        );
        assert_eq!(v["inferred"]["env"], json!(["CFG"]));
        let mut out = Vec::new();
        print_report(&mut out, &p, "t.perch", &r).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("ENV PREFIX") && s.contains("    kubectl: KUBECONFIG, Z\n"), "{s}");
        assert!(s.contains("subprocess") && s.contains("exec"));
    }

    #[test]
    fn exec_chain_body_is_walked() {
        let mut chain = op("exec_chain", json!({}));
        chain.body = vec![exec_op("git", Some(json!({"A": "1"})), ""), exec_op("git", None, "")];
        let r = analyze(&prog(vec![chain]));
        assert_eq!(r.exec_bins.get("git"), Some(&2));
        assert_eq!(score_report(&r).0, RiskScore::Low);
        assert_eq!(r.env_prefix.len(), 1);
    }

    #[test]
    fn helpers() {
        assert_eq!(path_root("/a/b/c/d"), "/a/b/…");
        assert_eq!(path_root("/a/b"), "/a/b/…");
        assert_eq!(path_root("/a"), "/a");
        assert_eq!(path_root("${X}"), "${X}");
        assert_eq!(path_root("${X}/y"), "${X}/y");
        assert_eq!(path_root("rel/p"), "rel/p");
        assert_eq!(first_shell_token("A=b B=c ./bin/tool x"), "tool");
        assert_eq!(extract_host("ftp://h.io/p"), Some("h.io"));
        assert_eq!(extract_host("nope"), None);
        assert!(has_unvalidated_interp("echo ${x}") && !has_unvalidated_interp("echo ${X}"));
    }

    // ── R01 (T-01..T-04) ──────────────────────────────────────────────

    /// The request.md example: declares `bin "sh"` and `write "./allowed"`,
    /// its shell call writes.
    fn request_example() -> Program {
        let mut p = prog(vec![op("shell", json!({"cmd": "sh -c 'echo hi > allowed/x'"}))]);
        p.requirements.declared = true;
        p.requirements.bins.push(perch_domain::BinReq { name: "sh".into(), ..Default::default() });
        p.requirements.write_roots.push("./allowed".into());
        p
    }

    fn run_scan(p: Program, format: &str) -> Result<String, Error> {
        let imp = Impl { load: Box::new(move |_| Ok(p.clone())) };
        let mut out = Vec::new();
        imp.execute("t.perch", format, &mut out)?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[test]
    fn t01_json_golden() {
        let got = run_scan(request_example(), "json").unwrap();
        let want = r#"{
  "declared": {
    "arch": [],
    "bin": [
      {
        "alias": "",
        "hash": "",
        "hash_file": "",
        "name": "sh",
        "optional": false
      }
    ],
    "declared": true,
    "env": [],
    "host": [],
    "os": [],
    "read": [],
    "write": [
      "./allowed"
    ]
  },
  "file": "t.perch",
  "inferred": {
    "catch_forwards": false,
    "env": [],
    "env_prefix": [],
    "exec_bins": [],
    "hosts": [],
    "network": false,
    "read": false,
    "read_roots": [],
    "shell": {
      "bins": [
        "sh"
      ],
      "calls": 1,
      "metachars": true,
      "sudo": false
    },
    "subprocess": false,
    "subprocess_ops": [],
    "write": false,
    "write_roots": []
  },
  "risk": "med",
  "risk_reasons": [
    "executes shell",
    "uses shell metacharacters (pipe / && / ; / $())"
  ],
  "schema": 1
}
"#;
        assert_eq!(got, want);
        // deterministic
        assert_eq!(got, run_scan(request_example(), "json").unwrap());
    }

    #[test]
    fn t02_json_risk_matches_badge_and_undeclared_file() {
        let p = prog(vec![op("print", json!({"msg": "hi"}))]);
        let v: Value = serde_json::from_str(&run_scan(p, "json").unwrap()).unwrap();
        assert_eq!(v["risk"], "safe");
        assert_eq!(v["declared"]["declared"], false);
        assert_eq!(v["declared"]["bin"], json!([]));
        assert_eq!(v["schema"], 1);
        let v: Value = serde_json::from_str(&run_scan(request_example(), "json").unwrap()).unwrap();
        assert_eq!(v["risk"], "med");
    }

    #[test]
    fn t03_unknown_format_is_an_error_and_text_default_unchanged() {
        let e = run_scan(request_example(), "yaml").unwrap_err().to_string();
        assert!(e.contains("unknown scan format") && e.contains("yaml"), "{e}");
        assert_eq!(run_scan(request_example(), "").unwrap(), run_scan(request_example(), "text").unwrap());
        assert!(run_scan(request_example(), "text").unwrap().contains("CAPABILITIES NEEDED"));
    }

    #[test]
    fn t04_declared_scope_suppresses_free_advice() {
        let text = run_scan(request_example(), "text").unwrap();
        assert!(!text.contains("--no-write` for free"), "{text}");
        assert!(!text.contains("      --no-write \\\n"), "{text}");
        assert!(text.contains("declared `write` scope"));
        // other undeclared capabilities keep the advice
        assert!(text.contains("add `--no-network` for free"));
        // network scope likewise
        let mut p = request_example();
        p.requirements.hosts.push(perch_domain::HostReq { name: "a.io".into(), optional: false });
        let text = run_scan(p, "text").unwrap();
        assert!(!text.contains("--no-network` for free"));
        // with no declaration the advice stays
        let text = run_scan(prog(vec![op("print", json!({}))]), "text").unwrap();
        assert!(text.contains("add `--no-write` for free"));
    }
}
