//! The `requires` gates group B needs (host / net / path). Private minimal
//! ports of requires.go's CheckHostDeclared / CheckNetDeclared /
//! checkPathDeclared; group A owns the full set.
use crate::group_b::util::{go_clean, go_join, is_abs};
use perch_domain::{ErrorKind, OpError};
use perch_interpreter::{go_quote, interpolate, Bindings, Error, Interpreter, Result};

fn oe(op: &str, kind: ErrorKind, msg: &str, detail: &str) -> Error {
    let mut e = OpError::new(op, kind, msg);
    if !detail.is_empty() {
        e = e.with_detail(detail);
    }
    Box::new(e)
}

pub fn check_host_declared(i: &Interpreter, host: &str) -> Result<()> {
    let r = &i.program.requirements;
    if !r.declared {
        return Ok(());
    }
    let host = host.to_lowercase();
    for h in &r.hosts {
        let want = h.name.to_lowercase();
        if want == host {
            return Ok(());
        }
        if want.starts_with("*.") && host.ends_with(&want[1..]) {
            return Ok(());
        }
    }
    Err(oe("http", ErrorKind::HostNotDeclared, &format!("host {} is not declared in `requires`", go_quote(&host)), &host))
}

pub fn check_net_declared(i: &Interpreter) -> Result<()> {
    let r = &i.program.requirements;
    if !r.declared || !r.hosts.is_empty() {
        return Ok(());
    }
    Err(oe("net", ErrorKind::HostNotDeclared, "network access is not declared in `requires` (declare a `host`)", ""))
}

/// Go `hostOfURL`.
pub fn host_of_url(u: &str) -> String {
    let mut s = u;
    if let Some(idx) = s.find("://") {
        s = &s[idx + 3..];
    }
    if let Some(idx) = s.find(['/', '?', '#']) {
        s = &s[..idx];
    }
    if let Some(idx) = s.rfind('@') {
        s = &s[idx + 1..];
    }
    if let Some(idx) = s.rfind(':') {
        s = &s[..idx];
    }
    s.to_string()
}

fn abs_under(p: &str, cwd: &str) -> String {
    if is_abs(p) {
        go_clean(p)
    } else {
        go_clean(&go_join(&[cwd, p]))
    }
}

fn within_any(abs: &str, roots: &[String], cwd: &str) -> bool {
    roots.iter().any(|root| {
        let r = abs_under(root, cwd);
        abs == r || abs.starts_with(&format!("{r}/"))
    })
}

fn expand_roots(roots: &[String], b: &Bindings) -> Vec<String> {
    roots.iter().filter_map(|r| interpolate(r, b).ok()).collect()
}

pub fn check_path_declared(i: &Interpreter, b: &Bindings, raw: &str, is_write: bool) -> Result<()> {
    let r = &i.program.requirements;
    if !r.declared || raw.is_empty() {
        return Ok(());
    }
    let abs = abs_under(raw, &b.cwd);
    let write_roots = expand_roots(&r.write_roots, b);
    if is_write {
        if within_any(&abs, &write_roots, &b.cwd) {
            return Ok(());
        }
        return Err(oe(
            "fs",
            ErrorKind::WriteNotDeclared,
            &format!("write to {} is outside every declared `write` root in `requires`", go_quote(raw)),
            raw,
        ));
    }
    if within_any(&abs, &expand_roots(&r.read_roots, b), &b.cwd) || within_any(&abs, &write_roots, &b.cwd) {
        return Ok(());
    }
    Err(oe(
        "fs",
        ErrorKind::ReadNotDeclared,
        &format!("read of {} is outside every declared `read` root in `requires`", go_quote(raw)),
        raw,
    ))
}
