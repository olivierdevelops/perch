//! Global-flag extraction. Each function strips the flags it owns from `args`
//! (argv including the program name at index 0) and returns the parsed value,
//! mirroring the Go `extract…Flag` helpers that rewrote `os.Args`.
use perch_interpreter::HTTPPolicy;
use perch_ops::Restrictions;
use std::collections::HashMap;
use std::time::Duration;

pub type Set = HashMap<String, bool>;

fn split_names(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
}

/// `--allow-host`, `--max-redirects`, `--no-redirects`, … Returns None to mean
/// "use the secure defaults" when none were given.
pub fn extract_http_policy(args: &mut Vec<String>) -> Option<HTTPPolicy> {
    let mut out = vec![args[0].clone()];
    let mut pol = HTTPPolicy { max_redirects: 5, ..Default::default() };
    let mut touched = false;
    let mut i = 1;
    let bad = |v: &str| -> ! {
        eprintln!("--max-redirects: bad value {}", go_quote(v));
        std::process::exit(2);
    };
    while i < args.len() {
        let a = args[i].clone();
        if a == "--no-redirects" {
            pol.max_redirects = 0;
            touched = true;
            i += 1;
        } else if a == "--allow-private-ips" {
            pol.allow_private_ips = true;
            touched = true;
            i += 1;
        } else if a == "--allow-scheme-downgrade" {
            pol.allow_scheme_downgrade = true;
            touched = true;
            i += 1;
        } else if a == "--allow-host" {
            if i + 1 < args.len() {
                pol.allowed_hosts.extend(split_names(&args[i + 1]));
                touched = true;
                i += 2;
                continue;
            }
            eprintln!("--allow-host requires a value (HOST[,HOST...])");
            std::process::exit(2);
        } else if let Some(v) = a.strip_prefix("--allow-host=") {
            pol.allowed_hosts.extend(split_names(v));
            touched = true;
            i += 1;
        } else if a == "--max-redirects" {
            if i + 1 < args.len() {
                match go_atoi(&args[i + 1]) {
                    Some(n) if n >= 0 => pol.max_redirects = n,
                    _ => bad(&args[i + 1]),
                }
                touched = true;
                i += 2;
                continue;
            }
            eprintln!("--max-redirects requires a non-negative integer");
            std::process::exit(2);
        } else if let Some(v) = a.strip_prefix("--max-redirects=") {
            match go_atoi(v) {
                Some(n) if n >= 0 => pol.max_redirects = n,
                _ => bad(&a),
            }
            touched = true;
            i += 1;
        } else {
            out.push(a);
            i += 1;
        }
    }
    *args = out;
    if touched {
        Some(pol)
    } else {
        None
    }
}

/// `strconv.Atoi`: optional sign then digits only.
fn go_atoi(s: &str) -> Option<i64> {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i64>().ok()
}

/// Go `%q` for the strings echoed in flag errors.
fn go_quote(s: &str) -> String {
    perch_interpreter::go_quote(s)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AllowFlags {
    pub shell: bool,
    pub subprocess: bool,
    pub network: bool,
    pub write: bool,
    pub trust_stdin: bool,
}

pub fn extract_allow(args: &mut Vec<String>) -> AllowFlags {
    let mut out = vec![args[0].clone()];
    let mut a = AllowFlags::default();
    for arg in &args[1..] {
        match arg.as_str() {
            "--allow-shell" => a.shell = true,
            "--allow-subprocess" => a.subprocess = true,
            "--allow-network" => a.network = true,
            "--allow-write" => a.write = true,
            "--trust-stdin" => a.trust_stdin = true,
            _ => out.push(arg.clone()),
        }
    }
    *args = out;
    a
}

/// `--allow-advisory-scopes`: run spawned binaries unconfined (with a banner)
/// where the platform cannot enforce declared `read`/`write`/`host` scopes.
pub fn extract_advisory_scopes(args: &mut Vec<String>) -> bool {
    let before = args.len();
    let mut first = true;
    args.retain(|a| {
        let keep = first || a != "--allow-advisory-scopes";
        first = false;
        keep
    });
    args.len() != before
}

/// True when the remaining args contain `-f -`.
pub fn is_stdin_invocation(args: &[String]) -> bool {
    (1..args.len().saturating_sub(1)).any(|i| args[i] == "-f" && args[i + 1] == "-")
}

pub fn extract_audit(args: &mut Vec<String>) -> String {
    let mut out = vec![args[0].clone()];
    let mut path = String::new();
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a == "--audit" {
            if i + 1 < args.len() {
                path = args[i + 1].clone();
                i += 2;
                continue;
            }
            eprintln!("--audit requires a path (use - for stdout)");
            std::process::exit(2);
        } else if let Some(v) = a.strip_prefix("--audit=") {
            path = v.to_string();
            i += 1;
        } else {
            out.push(a.clone());
            i += 1;
        }
    }
    *args = out;
    path
}

/// `--trace` / `--trace=PATH`.
pub fn extract_trace(args: &mut Vec<String>) -> (String, bool) {
    extract_bare_or_eq(args, "--trace")
}

/// `--report` / `--report=PATH`.
pub fn extract_report(args: &mut Vec<String>) -> (String, bool) {
    extract_bare_or_eq(args, "--report")
}

fn extract_bare_or_eq(args: &mut Vec<String>, flag: &str) -> (String, bool) {
    let eq = format!("{flag}=");
    let mut out = vec![args[0].clone()];
    let mut path = String::new();
    let mut on = false;
    for a in &args[1..] {
        if a == flag {
            on = true;
        } else if let Some(v) = a.strip_prefix(&eq) {
            on = true;
            path = v.to_string();
        } else {
            out.push(a.clone());
        }
    }
    *args = out;
    (path, on)
}

pub fn extract_max_runtime(args: &mut Vec<String>) -> Duration {
    let mut out = vec![args[0].clone()];
    let mut d = Duration::ZERO;
    let parse = |s: &str| -> Duration {
        match go_atoi(s) {
            Some(n) if n >= 0 => Duration::from_secs(n as u64),
            _ => {
                eprintln!(
                    "--max-runtime: bad value {} (want a non-negative integer of seconds)",
                    go_quote(s)
                );
                std::process::exit(2);
            }
        }
    };
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a == "--max-runtime" {
            if i + 1 < args.len() {
                d = parse(&args[i + 1]);
                i += 2;
                continue;
            }
            eprintln!("--max-runtime requires a value in seconds");
            std::process::exit(2);
        } else if let Some(v) = a.strip_prefix("--max-runtime=") {
            d = parse(v);
            i += 1;
        } else {
            out.push(a.clone());
            i += 1;
        }
    }
    *args = out;
    d
}

/// `--allow-bin NAME[,…]` and `--no-shell-metachars`. Returns (None, false)
/// when neither was given.
pub fn extract_shell_guards(args: &mut Vec<String>) -> (Option<Set>, bool) {
    let mut out = vec![args[0].clone()];
    let mut allow: Option<Set> = None;
    let mut no_meta = false;
    let mut i = 1;
    let add = |allow: &mut Option<Set>, s: &str| {
        let set = allow.get_or_insert_with(Set::new);
        for n in split_names(s) {
            set.insert(n, true);
        }
    };
    while i < args.len() {
        let a = args[i].clone();
        if a == "--no-shell-metachars" {
            no_meta = true;
            i += 1;
        } else if a == "--allow-bin" {
            if i + 1 < args.len() {
                add(&mut allow, &args[i + 1]);
                i += 2;
                continue;
            }
            allow.get_or_insert_with(Set::new);
            i += 1;
        } else if let Some(v) = a.strip_prefix("--allow-bin=") {
            add(&mut allow, v);
            i += 1;
        } else {
            out.push(a);
            i += 1;
        }
    }
    *args = out;
    (allow, no_meta)
}

pub fn extract_restrictions(args: &mut Vec<String>) -> Restrictions {
    let mut out = vec![args[0].clone()];
    let mut r = Restrictions::default();
    for a in &args[1..] {
        match a.as_str() {
            "--no-shell" => r.no_shell = true,
            "--no-subprocess" => r.no_subprocess = true,
            "--no-network" => r.no_network = true,
            "--no-write" => r.no_write = true,
            _ => out.push(a.clone()),
        }
    }
    *args = out;
    r
}

/// `--env A,B,C` / `--env=A,B,C`. None = never given; Some(empty) = given with
/// no names (nothing visible).
pub fn extract_env(args: &mut Vec<String>) -> Option<Set> {
    let mut out = vec![args[0].clone()];
    let mut allow: Option<Set> = None;
    let add = |allow: &mut Option<Set>, s: &str| {
        let set = allow.get_or_insert_with(Set::new);
        for n in split_names(s) {
            set.insert(n, true);
        }
    };
    let mut i = 1;
    while i < args.len() {
        let a = args[i].clone();
        if a == "--env" {
            if i + 1 < args.len() {
                add(&mut allow, &args[i + 1]);
                i += 2;
                continue;
            }
            allow.get_or_insert_with(Set::new);
            i += 1;
        } else if let Some(v) = a.strip_prefix("--env=") {
            add(&mut allow, v);
            i += 1;
        } else {
            out.push(a);
            i += 1;
        }
    }
    *args = out;
    allow
}

/// `--ask` / `--dry-run`: "", "ask" or "dry-run" (ask wins).
pub fn extract_preview(args: &mut Vec<String>) -> String {
    let mut out = vec![args[0].clone()];
    let mut mode = String::new();
    for a in &args[1..] {
        match a.as_str() {
            "--ask" => mode = "ask".into(),
            "--dry-run" => {
                if mode.is_empty() {
                    mode = "dry-run".into();
                }
            }
            _ => out.push(a.clone()),
        }
    }
    *args = out;
    mode
}

#[cfg(test)]
mod advisory_tests {
    use super::*;

    #[test]
    fn extracts_advisory_flag_anywhere_but_not_argv0() {
        let mut a: Vec<String> = ["perch", "--allow-advisory-scopes", "go"].map(String::from).to_vec();
        assert!(extract_advisory_scopes(&mut a));
        assert_eq!(a, vec!["perch", "go"]);
        let mut b: Vec<String> = ["perch", "go"].map(String::from).to_vec();
        assert!(!extract_advisory_scopes(&mut b));
    }
}
