//! Restriction flags disable groups of ops by category. Each `--no-X` CLI flag
//! toggles one category; multiple flags compose (additive).
//!
//! A blocked op is replaced with a sentinel handler returning a friendly
//! "disabled by --no-X" error. Other ops are untouched.
use perch_interpreter::{err, go_quote, handler, Args, Bindings, CapMask, Handler, Interpreter, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Restriction names. The value of each constant is the CLI flag minus the
/// leading `--`, so the same string drives the flag, the error message, and
/// the docs.
pub const RESTRICT_NO_SHELL: &str = "no-shell";
pub const RESTRICT_NO_SUBPROCESS: &str = "no-subprocess";
pub const RESTRICT_NO_NETWORK: &str = "no-network";
pub const RESTRICT_NO_WRITE: &str = "no-write";

/// Anything that hands a string to the host shell.
const BLOCK_NO_SHELL: &[&str] = &["shell", "shell_output", "shell_detached", "shell_in", "try_shell"];

/// Process management beyond shell — anything that fork/execs without going
/// through the `shell` op. These can leak host env vars, network access, file
/// access into the spawned process just like `shell` can.
const BLOCK_NO_SUBPROCESS: &[&str] = &[
    "exec", // shell-free direct subprocess (sandboxed-by-design §3.2)
    "pkg_install",
    "pkg_uninstall",
    "kill_by_name",
    "process_running",
    "bin_version", // runs `BIN --version`
    "os_version",  // runs `sw_vers` / `uname -r` / `cmd /c ver`
];

const BLOCK_NO_NETWORK: &[&str] = &[
    "http_get",
    "http_post",
    "http_put",
    "http_delete",
    "http_status",
    "download",
    "dns_lookup",
    "port_check",
    "port_free",
    "find_free_port",
    "wait_for_port",
    "wait_for_url",
    "public_ip",
    // Network introspection also reveals host facts.
    "local_ip",
    "mac_address",
    "interfaces",
];

const BLOCK_NO_WRITE: &[&str] = &[
    // Filesystem mutation.
    "write_file",
    "append_file",
    "append_line",
    "ensure_line_in_file",
    "replace_in_file",
    "backup_file",
    "cp",
    "mv",
    "rm",
    "mkdir",
    "chmod",
    "touch",
    "copy_dir",
    "ensure_dir",
    "make_executable",
    "symlink",
    "link_into_path",
    "mktemp_file",
    "mktemp_dir",
    "add_to_path",
    "tar_extract",
    "zip_extract",
    "gzip",
    "ungzip",
    "tar_create",
    "zip_create",
    "bundle_extract",
    "bundle_dir",
];

/// The op kinds a restriction forbids (`restrictBlocks[name]`).
pub fn restrict_blocks(name: &str) -> &'static [&'static str] {
    match name {
        RESTRICT_NO_SHELL => BLOCK_NO_SHELL,
        RESTRICT_NO_SUBPROCESS => BLOCK_NO_SUBPROCESS,
        RESTRICT_NO_NETWORK => BLOCK_NO_NETWORK,
        RESTRICT_NO_WRITE => BLOCK_NO_WRITE,
        _ => &[],
    }
}

/// The set of active --no-X flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Restrictions {
    pub no_shell: bool,
    pub no_subprocess: bool,
    pub no_network: bool,
    pub no_write: bool,
}

impl Restrictions {
    /// True if any restriction is on.
    pub fn active(&self) -> bool {
        self.no_shell || self.no_subprocess || self.no_network || self.no_write
    }

    /// The active restrictions as CLI-flag strings, e.g. ["--no-shell",
    /// "--no-network"]. Used in error messages so the user sees the exact flag
    /// that blocked their op.
    pub fn as_flags(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.no_shell {
            out.push(format!("--{RESTRICT_NO_SHELL}"));
        }
        if self.no_subprocess {
            out.push(format!("--{RESTRICT_NO_SUBPROCESS}"));
        }
        if self.no_network {
            out.push(format!("--{RESTRICT_NO_NETWORK}"));
        }
        if self.no_write {
            out.push(format!("--{RESTRICT_NO_WRITE}"));
        }
        out
    }
}

/// Mutates handlers in place: every op blocked by one of the active
/// restrictions is replaced with a sentinel that returns `op "X" is disabled by
/// --no-Y`. Iterates flags in canonical order so the FIRST applicable
/// restriction is the one cited in errors (deterministic).
pub fn apply_restrictions(handlers: &mut HashMap<String, Handler>, r: &Restrictions) {
    if !r.active() {
        return;
    }
    let order = [
        (r.no_shell, RESTRICT_NO_SHELL),
        (r.no_subprocess, RESTRICT_NO_SUBPROCESS),
        (r.no_network, RESTRICT_NO_NETWORK),
        (r.no_write, RESTRICT_NO_WRITE),
    ];
    let mut already_blocked: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (on, name) in order {
        if !on {
            continue;
        }
        for op in restrict_blocks(name) {
            if already_blocked.contains(op) {
                continue;
            }
            if handlers.contains_key(*op) {
                handlers.insert(op.to_string(), make_deny(name, op));
                already_blocked.insert(op);
            }
        }
    }
}

fn make_deny(flag: &'static str, op: &'static str) -> Handler {
    handler(move |_i, _b, _args| {
        Err(err(format!(
            "op {} is disabled by --{} — run `perch help --{}` for details",
            go_quote(op),
            flag,
            flag
        )))
    })
}

/// Every known restriction name, sorted (for `perch --restrictions`).
pub fn restriction_list() -> Vec<String> {
    let mut out: Vec<String> =
        [RESTRICT_NO_SHELL, RESTRICT_NO_SUBPROCESS, RESTRICT_NO_NETWORK, RESTRICT_NO_WRITE].map(String::from).to_vec();
    out.sort();
    out
}

/// The ops blocked by ONE restriction (for the `--restrictions` discovery list),
/// sorted for deterministic output.
pub fn blocked_by_restriction(name: &str) -> Vec<String> {
    let mut out: Vec<String> = restrict_blocks(name).iter().map(|s| s.to_string()).collect();
    out.sort();
    out
}

/// A human-readable summary like "--no-shell, --no-network" for error messages
/// and audit logging.
pub fn summarise_restrictions(r: &Restrictions) -> String {
    let flags = r.as_flags();
    if flags.is_empty() {
        return "(none)".to_string();
    }
    flags.join(", ")
}

/// Maps an op kind back to the restriction category that governs it
/// ("no-shell" / "no-network" / etc.). Lets a `sandbox` block check whether a
/// kind is gated without replicating the catalogue.
pub fn op_category() -> HashMap<&'static str, &'static str> {
    let mut out = HashMap::new();
    for cat in [RESTRICT_NO_SHELL, RESTRICT_NO_SUBPROCESS, RESTRICT_NO_NETWORK, RESTRICT_NO_WRITE] {
        for op in restrict_blocks(cat) {
            out.insert(*op, cat);
        }
    }
    out
}

/// Wraps every restrictable handler with a runtime check against `b.cap_mask`.
/// This is what makes `sandbox no_shell ... end` work: the CLI restrictions
/// block ops at the handler registration layer (the outermost, never-narrowed
/// gate), and on top of that this wrapping pass adds an inner check that
/// consults the dynamic mask each call.
///
/// Call AFTER `apply_restrictions` so CLI-blocked handlers are already
/// sentinels — wrapping them is harmless (the mask check runs first).
pub fn apply_mask_gating(handlers: &mut HashMap<String, Handler>) {
    for (kind, cat) in op_category() {
        let Some(inner) = handlers.get(kind).cloned() else { continue };
        handlers.insert(
            kind.to_string(),
            handler(move |i: &Interpreter, b: &mut Bindings, args: &Args<'_>| -> Result<Value> {
                if let Some(mask) = b.cap_mask.clone() {
                    let m = Some(&*mask);
                    let blocked = match cat {
                        RESTRICT_NO_SHELL => CapMask::any_no_shell(m),
                        RESTRICT_NO_SUBPROCESS => CapMask::any_no_subprocess(m),
                        RESTRICT_NO_NETWORK => CapMask::any_no_network(m),
                        RESTRICT_NO_WRITE => CapMask::any_no_write(m),
                        _ => false,
                    };
                    if blocked {
                        return Err(err(format!(
                            "op {} forbidden by sandbox ({} scope) — narrow the body or move the call outside the sandbox block",
                            go_quote(kind),
                            cat
                        )));
                    }
                    // allow_bin narrowing: applies only to shell ops.
                    if cat == RESTRICT_NO_SHELL && mask.allowed_bins.is_some() {
                        if let Some(Value::String(cmd)) = args.get("cmd") {
                            let first = first_token(cmd);
                            if !first.is_empty() && !CapMask::allowed_bin_permitted(m, first) {
                                return Err(err(format!(
                                    "shell binary {} forbidden by sandbox allow_bin",
                                    go_quote(first)
                                )));
                            }
                        }
                    }
                }
                inner(i, b, args)
            }),
        );
    }
}

/// The first whitespace-separated word of `s`. Used for argv[0] checks against
/// allow_bin allowlists.
fn first_token(s: &str) -> &str {
    let s = s.trim();
    match s.find([' ', '\t']) {
        Some(i) => &s[..i],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_message_and_order() {
        let mut m: HashMap<String, Handler> = HashMap::new();
        m.insert("shell".into(), handler(|_, _, _| Ok(Value::Null)));
        m.insert("exec".into(), handler(|_, _, _| Ok(Value::Null)));
        let r = Restrictions { no_shell: true, no_subprocess: true, ..Default::default() };
        apply_restrictions(&mut m, &r);
        let i = Interpreter::new(HashMap::new(), perch_domain::Program::default());
        let mut b = Bindings::new("");
        let a = Args { map: Default::default(), body: &[] };
        let e = m["shell"](&i, &mut b, &a).unwrap_err();
        assert_eq!(e.to_string(), "op \"shell\" is disabled by --no-shell — run `perch help --no-shell` for details");
        let e = m["exec"](&i, &mut b, &a).unwrap_err();
        assert!(e.to_string().contains("--no-subprocess"));
        assert_eq!(summarise_restrictions(&r), "--no-shell, --no-subprocess");
        assert_eq!(summarise_restrictions(&Restrictions::default()), "(none)");
    }

    #[test]
    fn sandbox_mask_gates_ops() {
        let mut m: HashMap<String, Handler> = HashMap::new();
        m.insert("shell".into(), handler(|_, _, _| Ok(Value::Null)));
        apply_mask_gating(&mut m);
        let i = Interpreter::new(HashMap::new(), perch_domain::Program::default());
        let mut b = Bindings::new("");
        let a = Args { map: Default::default(), body: &[] };
        assert!(m["shell"](&i, &mut b, &a).is_ok());
        b.cap_mask = Some(CapMask::push(None, CapMask { no_shell: true, ..Default::default() }));
        let e = m["shell"](&i, &mut b, &a).unwrap_err();
        assert!(e.to_string().contains("forbidden by sandbox (no-shell scope)"), "{e}");
    }
}
