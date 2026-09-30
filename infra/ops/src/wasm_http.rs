//! Host-provided HTTP for WASM modules (wasm_http.go).
//!
//! WASI Preview 1 has no network, so perch exports a small `perch` import
//! module (`http_get`, `http_status`, `http_body_len`, `http_read_body`,
//! `http_close`) — same names and ABI as the Go host module, so guests built
//! against `wasm-sdk/perchhttp` run unchanged.
//!
//! Gating composes: `wasm_allow_host` declarations form a per-module
//! allowlist (none = no network, every call returns -1); the outer
//! interpreter `HTTPPolicy` (--allow-host, redirects, SSRF guard) applies to
//! every call and redirect hop. The intersection wins. The module only ever
//! sees -1 on refusal; the reason stays host-side by design.
use super::HostState;
use crate::group_b::http::wasm_get;
use perch_interpreter::{HTTPPolicy, Interpreter};
use std::collections::HashMap;
use wasmtime::{Caller, Extern, Linker, Memory};

struct Handle {
    status_code: i32,
    body: Vec<u8>,
    pos: usize,
}

/// Per-call HTTP state (Go `httpCallState`).
pub struct CallState {
    policy: HTTPPolicy,
    /// false -> every http_get returns -1.
    enabled: bool,
    handles: HashMap<i32, Handle>,
    next_id: i32,
}

impl CallState {
    fn put(&mut self, h: Handle) -> i32 {
        self.next_id += 1;
        self.handles.insert(self.next_id, h);
        self.next_id
    }
}

/// Builds the per-call state. Empty `module_allowed` disables HTTP.
pub fn build_call_state(i: &Interpreter, module_allowed: &[String]) -> CallState {
    let mut s = CallState {
        policy: HTTPPolicy { max_redirects: 5, allow_private_ips: false, allow_scheme_downgrade: false, allowed_hosts: Vec::new() },
        enabled: false,
        handles: HashMap::new(),
        next_id: 0,
    };
    if module_allowed.is_empty() {
        return s;
    }
    s.enabled = true;
    s.policy = i.http_policy.clone().unwrap_or(s.policy);
    if s.policy.allowed_hosts.is_empty() {
        s.policy.allowed_hosts = module_allowed.to_vec();
    } else {
        let inter = intersect_hosts(&s.policy.allowed_hosts, module_allowed);
        // Empty intersection: the module asks for hosts the outer policy
        // forbids. Disable rather than silently allow nothing.
        if inter.is_empty() {
            s.enabled = false;
        }
        s.policy.allowed_hosts = inter;
    }
    s
}

/// Hosts of `b` that also appear in `a`, in `b`'s order (Go `intersectHosts`).
fn intersect_hosts(a: &[String], b: &[String]) -> Vec<String> {
    b.iter().filter(|y| a.contains(y)).cloned().collect()
}

fn memory(caller: &mut Caller<'_, HostState>) -> Option<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(m)) => Some(m),
        _ => None,
    }
}

/// Installs the `perch` host module on the linker.
pub fn install_perch_host_module(linker: &mut Linker<HostState>) -> wasmtime::Result<()> {
    linker.func_wrap("perch", "http_get", |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> i32 {
        if !caller.data().http.enabled {
            return -1;
        }
        let Some(mem) = memory(&mut caller) else { return -1 };
        let raw = {
            let data = mem.data(&caller);
            let (start, end) = (ptr as usize, ptr as usize + len as usize);
            if end > data.len() {
                return -1;
            }
            String::from_utf8_lossy(&data[start..end]).into_owned()
        };
        let policy = caller.data().http.policy.clone();
        match wasm_get(&policy, &raw) {
            Ok((body, status)) => caller.data_mut().http.put(Handle { status_code: status as i32, body, pos: 0 }),
            Err(_) => -1,
        }
    })?;
    linker.func_wrap("perch", "http_status", |caller: Caller<'_, HostState>, h: i32| -> i32 {
        caller.data().http.handles.get(&h).map_or(0, |r| r.status_code)
    })?;
    linker.func_wrap("perch", "http_body_len", |caller: Caller<'_, HostState>, h: i32| -> i32 {
        caller.data().http.handles.get(&h).map_or(-1, |r| r.body.len() as i32)
    })?;
    linker.func_wrap(
        "perch",
        "http_read_body",
        |mut caller: Caller<'_, HostState>, h: i32, dst: u32, cap: u32| -> i32 {
            let Some(mem) = memory(&mut caller) else { return -1 };
            let (data, host) = mem.data_and_store_mut(&mut caller);
            let Some(rec) = host.http.handles.get_mut(&h) else { return -1 };
            let remaining = &rec.body[rec.pos..];
            let n = remaining.len().min(cap as usize);
            let start = dst as usize;
            if start.checked_add(n).is_none_or(|end| end > data.len()) {
                return -1;
            }
            data[start..start + n].copy_from_slice(&remaining[..n]);
            rec.pos += n;
            n as i32
        },
    )?;
    linker.func_wrap("perch", "http_close", |mut caller: Caller<'_, HostState>, h: i32| -> i32 {
        if caller.data_mut().http.handles.remove(&h).is_some() {
            0
        } else {
            -1
        }
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersect_keeps_module_order() {
        let a = vec!["a.com".to_string(), "b.com".to_string()];
        let b = vec!["b.com".to_string(), "c.com".to_string(), "a.com".to_string()];
        assert_eq!(intersect_hosts(&a, &b), vec!["b.com", "a.com"]);
    }
}
