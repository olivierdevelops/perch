//! Network ops (network.go). Interface enumeration uses `getifaddrs` (Unix).
use crate::group_b::gate::{check_host_declared, check_net_declared, host_of_url};
use crate::group_b::http::run_http;
use crate::group_b::util::*;
use perch_interpreter::{err, handler, Args, Bindings, Handler, Interpreter, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

type H = fn(&Interpreter, &mut Bindings, &Args<'_>) -> Result<Value>;

pub fn register(m: &mut HashMap<String, Handler>) {
    let mut add = |k: &str, f: H| {
        m.insert(k.to_string(), handler(f));
    };
    add("hostname", op_hostname);
    add("dns_lookup", op_dns_lookup);
    add("port_check", op_port_check);
    add("port_free", op_port_free);
    add("find_free_port", op_find_free_port);
    add("wait_for_port", op_wait_for_port);
    add("wait_for_url", op_wait_for_url);
    add("http_status", op_http_status);
    add("local_ip", op_local_ip);
    add("public_ip", op_public_ip);
    add("interfaces", op_interfaces);
    add("mac_address", op_mac_address);
}

fn op_hostname(_i: &Interpreter, _b: &mut Bindings, _a: &Args<'_>) -> Result<Value> {
    super::fsx::hostname()
        .map(Value::String)
        .map_err(|e| err(format!("hostname: {}", go_io_msg(&e))))
}

fn op_dns_lookup(i: &Interpreter, _b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let host = arg_string(a, &["host", "_0"]);
    check_host_declared(i, &host)?;
    let addrs = (host.as_str(), 0u16)
        .to_socket_addrs()
        .map_err(|_| err(format!("lookup {host}: no such host")))?;
    let mut ips: Vec<String> = Vec::new();
    for sa in addrs {
        let s = sa.ip().to_string();
        if !ips.contains(&s) {
            ips.push(s);
        }
    }
    Ok(Value::String(ips.join("\n")))
}

/// `net.DialTimeout("tcp", addr, d)` success?
fn dial(addr: &str, timeout: Duration) -> bool {
    let Ok(addrs) = addr.to_socket_addrs() else { return false };
    addrs.into_iter().any(|sa| TcpStream::connect_timeout(&sa, timeout).is_ok())
}

fn op_port_check(i: &Interpreter, _b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let host = arg_string(a, &["host", "_0"]);
    check_host_declared(i, &host)?;
    let addr = format!("{host}:{}", arg_string(a, &["port", "_1"]));
    Ok(Value::Bool(dial(&addr, Duration::from_secs(2))))
}

/// `net.Listen("tcp", ":"+port)` — dual-stack wildcard, falling back to IPv4.
fn listen(port: u16) -> std::io::Result<TcpListener> {
    match TcpListener::bind(SocketAddr::new(IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED), port)) {
        Ok(l) => Ok(l),
        Err(e) if e.kind() == std::io::ErrorKind::AddrNotAvailable || super::fsx::is_af_unsupported(&e) => {
            TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port))
        }
        Err(e) => Err(e),
    }
}

fn op_port_free(i: &Interpreter, _b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    check_net_declared(i)?;
    let Ok(port) = arg_string(a, &["port", "_0"]).parse::<u16>() else { return Ok(Value::Bool(false)) };
    Ok(Value::Bool(listen(port).is_ok()))
}

fn op_find_free_port(i: &Interpreter, _b: &mut Bindings, _a: &Args<'_>) -> Result<Value> {
    check_net_declared(i)?;
    let l = listen(0).map_err(|e| err(format!("listen tcp :0: {}", go_io_msg(&e))))?;
    let port = l.local_addr().map_err(|e| err(go_io_msg(&e)))?.port();
    Ok(Value::from(i64::from(port)))
}

/// Go `strconv.Atoi` on a possibly-invalid string (0 on error).
fn atoi(s: &str) -> i64 {
    s.parse::<i64>().unwrap_or(0)
}

// wait_for_port "127.0.0.1" 6379 30 -> true when reachable, false on timeout.
fn op_wait_for_port(i: &Interpreter, _b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let host = arg_string(a, &["host", "_0"]);
    check_host_declared(i, &host)?;
    let port = arg_string(a, &["port", "_1"]);
    let mut secs = atoi(&arg_string(a, &["timeout", "_2"]));
    if secs <= 0 {
        secs = 30;
    }
    let deadline = Instant::now() + Duration::from_secs(secs as u64);
    let addr = format!("{host}:{port}");
    while Instant::now() < deadline {
        if dial(&addr, Duration::from_millis(500)) {
            return Ok(Value::Bool(true));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(Value::Bool(false))
}

// wait_for_url "http://localhost:8080/health" 30 -> true when it returns 2xx.
fn op_wait_for_url(i: &Interpreter, _b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let url = arg_string(a, &["url", "_0"]);
    check_host_declared(i, &host_of_url(&url))?;
    let mut secs = atoi(&arg_string(a, &["timeout", "_1"]));
    if secs <= 0 {
        secs = 30;
    }
    // Plain client (no policy), 2s timeout, default redirect following.
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(2)).user_agent("Go-http-client/1.1").build();
    let deadline = Instant::now() + Duration::from_secs(secs as u64);
    while Instant::now() < deadline {
        if let Ok(resp) = agent.get(&url).call() {
            if (200..300).contains(&resp.status()) {
                return Ok(Value::Bool(true));
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(Value::Bool(false))
}

fn op_http_status(i: &Interpreter, _b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let url = arg_string(a, &["url", "_0"]);
    // Same policy-aware path as http_get: SSRF check, redirect cap, downgrade guard.
    match run_http(i, "HEAD", &url, None) {
        Ok(resp) => Ok(Value::from(i64::from(resp.status()))),
        Err(_) => Ok(Value::from(0)),
    }
}

// ── interfaces ────────────────────────────────────────────────────────────

struct Iface {
    name: String,
    index: u32,
    up: bool,
    loopback: bool,
    mac: Vec<u8>,
    ipv4: Vec<Ipv4Addr>,
}

#[cfg(not(unix))]
fn list_ifaces() -> std::io::Result<Vec<Iface>> {
    // No getifaddrs on Windows; interface ops report no interfaces.
    Ok(Vec::new())
}

#[cfg(unix)]
fn list_ifaces() -> std::io::Result<Vec<Iface>> {
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs allocates a list we free with freeifaddrs below.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut out: Vec<Iface> = Vec::new();
    let mut cur = head;
    while !cur.is_null() {
        // SAFETY: cur is a valid node of the list returned by getifaddrs.
        let ifa = unsafe { &*cur };
        cur = ifa.ifa_next;
        // SAFETY: ifa_name is a NUL-terminated string owned by the list.
        let name = unsafe { std::ffi::CStr::from_ptr(ifa.ifa_name) }.to_string_lossy().into_owned();
        let pos = match out.iter().position(|f| f.name == name) {
            Some(p) => p,
            None => {
                let cname = std::ffi::CString::new(name.clone()).unwrap_or_default();
                // SAFETY: cname is a valid C string.
                let index = unsafe { libc::if_nametoindex(cname.as_ptr()) };
                out.push(Iface {
                    name,
                    index,
                    up: ifa.ifa_flags & (libc::IFF_UP as u32) != 0,
                    loopback: ifa.ifa_flags & (libc::IFF_LOOPBACK as u32) != 0,
                    mac: Vec::new(),
                    ipv4: Vec::new(),
                });
                out.len() - 1
            }
        };
        if ifa.ifa_addr.is_null() {
            continue;
        }
        // SAFETY: ifa_addr is non-null and points at a sockaddr of at least sa_family bytes.
        let family = unsafe { (*ifa.ifa_addr).sa_family } as i32;
        if family == libc::AF_INET {
            // SAFETY: family says this is a sockaddr_in.
            let sin = unsafe { &*(ifa.ifa_addr as *const libc::sockaddr_in) };
            out[pos].ipv4.push(Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr)));
        } else {
            out[pos].mac = link_addr(ifa.ifa_addr, family).unwrap_or_else(|| std::mem::take(&mut out[pos].mac));
        }
    }
    // SAFETY: head came from getifaddrs and is freed exactly once.
    unsafe { libc::freeifaddrs(head) };
    out.sort_by_key(|f| f.index);
    Ok(out)
}

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
fn link_addr(sa: *const libc::sockaddr, family: i32) -> Option<Vec<u8>> {
    if family != libc::AF_LINK {
        return None;
    }
    // SAFETY: AF_LINK sockaddrs are sockaddr_dl; the data window is bounds-checked below.
    unsafe {
        let sdl = &*(sa as *const libc::sockaddr_dl);
        let nlen = sdl.sdl_nlen as usize;
        let alen = sdl.sdl_alen as usize;
        if alen == 0 || nlen + alen > sdl.sdl_data.len() {
            return None;
        }
        Some(sdl.sdl_data[nlen..nlen + alen].iter().map(|&c| c as u8).collect())
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn link_addr(sa: *const libc::sockaddr, family: i32) -> Option<Vec<u8>> {
    if family != libc::AF_PACKET {
        return None;
    }
    // SAFETY: AF_PACKET sockaddrs are sockaddr_ll.
    unsafe {
        let sll = &*(sa as *const libc::sockaddr_ll);
        let n = (sll.sll_halen as usize).min(sll.sll_addr.len());
        if n == 0 {
            return None;
        }
        Some(sll.sll_addr[..n].to_vec())
    }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "linux",
    target_os = "android"
)))]
#[allow(dead_code)]
fn link_addr(_sa: *const libc::sockaddr, _family: i32) -> Option<Vec<u8>> {
    None
}

fn op_local_ip(i: &Interpreter, _b: &mut Bindings, _a: &Args<'_>) -> Result<Value> {
    check_net_declared(i)?;
    // Dial a public address (no packets sent) to pick the outbound interface's IP.
    if let Ok(s) = UdpSocket::bind("0.0.0.0:0") {
        if s.connect("8.8.8.8:80").is_ok() {
            if let Ok(la) = s.local_addr() {
                return Ok(Value::String(la.ip().to_string()));
            }
        }
    }
    let ifs = list_ifaces().map_err(|e| err(go_io_msg(&e)))?;
    for f in ifs {
        if f.loopback || !f.up {
            continue;
        }
        if let Some(ip) = f.ipv4.first() {
            return Ok(Value::String(ip.to_string()));
        }
    }
    Ok(Value::String(String::new()))
}

fn op_public_ip(i: &Interpreter, _b: &mut Bindings, _a: &Args<'_>) -> Result<Value> {
    // Contacts api.ipify.org — that host must be declared.
    check_host_declared(i, "api.ipify.org")?;
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(5)).user_agent("Go-http-client/1.1").build();
    let resp = match agent.get("https://api.ipify.org").call() {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(ureq::Error::Transport(t)) => return Err(err(format!("Get \"https://api.ipify.org\": {t}"))),
    };
    let mut buf = [0u8; 64];
    let n = resp.into_reader().read(&mut buf).unwrap_or(0);
    Ok(Value::String(String::from_utf8_lossy(&buf[..n]).trim().to_string()))
}

fn op_interfaces(i: &Interpreter, _b: &mut Bindings, _a: &Args<'_>) -> Result<Value> {
    check_net_declared(i)?;
    let ifs = list_ifaces().map_err(|e| err(go_io_msg(&e)))?;
    let names: Vec<String> = ifs.into_iter().filter(|f| f.up).map(|f| f.name).collect();
    Ok(Value::String(names.join("\n")))
}

fn op_mac_address(i: &Interpreter, _b: &mut Bindings, _a: &Args<'_>) -> Result<Value> {
    check_net_declared(i)?;
    let ifs = list_ifaces().map_err(|e| err(go_io_msg(&e)))?;
    for f in ifs {
        if f.up && !f.loopback && !f.mac.is_empty() {
            return Ok(Value::String(f.mac.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":")));
        }
    }
    Ok(Value::String(String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_port_round_trip() {
        let l = listen(0).unwrap();
        let port = l.local_addr().unwrap().port();
        assert!(listen(port).is_err(), "port in use is not free");
        assert!(dial(&format!("127.0.0.1:{port}"), Duration::from_millis(500)));
        drop(l);
    }

    #[test]
    fn interfaces_include_loopback() {
        let ifs = list_ifaces().unwrap();
        assert!(ifs.iter().any(|f| f.loopback));
    }
}
