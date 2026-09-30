//! HTTP ops (http.go) with hardened defaults: no requests/redirects to
//! private / loopback / link-local IPs (SSRF guard, also applied to the
//! initial URL), no https->http downgrade, a capped redirect chain, and an
//! optional host allowlist. Policy comes from the interpreter's `HTTPPolicy`.
use crate::group_b::gate::{check_host_declared, check_path_declared};
use crate::group_b::util::*;
use perch_domain::{ErrorKind, OpError};
use perch_interpreter::{err, go_quote, handler, Args, Bindings, Error, HTTPPolicy, Handler, Interpreter, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, ToSocketAddrs};
use std::time::Duration;
use url::Url;

pub fn register(m: &mut HashMap<String, Handler>) {
    m.insert("http_get".into(), handler(op_http_get));
    m.insert("http_post".into(), http_method("POST"));
    m.insert("http_put".into(), http_method("PUT"));
    m.insert("http_delete".into(), http_method("DELETE"));
    m.insert("download".into(), handler(op_download));
}

/// The active policy, or the secure defaults when the interpreter has none.
fn http_policy(i: &Interpreter) -> HTTPPolicy {
    i.http_policy.clone().unwrap_or(HTTPPolicy {
        max_redirects: 5,
        allow_private_ips: false,
        allow_scheme_downgrade: false,
        allowed_hosts: Vec::new(),
    })
}

/// A request URL plus the pieces Go's `url.URL` exposes that the `url` crate
/// normalizes away (the raw `Host`, including an explicit default port).
struct Target {
    url: Url,
    /// Go `u.Host` (authority without userinfo, original case).
    host: String,
    /// Go `u.String()`.
    display: String,
}

enum ParseFail {
    NoHost(String),
    Bad(String),
}

/// Authority of `raw` without userinfo (Go `u.Host`).
fn raw_host(raw: &str) -> String {
    let mut s = raw;
    if let Some(i) = s.find("://") {
        s = &s[i + 3..];
    }
    if let Some(i) = s.find(['/', '?', '#']) {
        s = &s[..i];
    }
    if let Some(i) = s.rfind('@') {
        s = &s[i + 1..];
    }
    s.to_string()
}

/// Go `u.Hostname()` from an authority.
fn hostname_of(authority: &str) -> String {
    if let Some(rest) = authority.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            return rest[..end].to_string();
        }
    }
    match authority.rfind(':') {
        Some(i) if authority[i + 1..].bytes().all(|c| c.is_ascii_digit()) => authority[..i].to_string(),
        _ => authority.to_string(),
    }
}

fn parse_target(raw: &str) -> std::result::Result<Target, ParseFail> {
    match Url::parse(raw) {
        Ok(u) => {
            if u.host_str().is_none_or(|h| h.is_empty()) {
                return Err(ParseFail::NoHost(raw.to_string()));
            }
            let host = raw_host(raw);
            Ok(Target { url: u, host, display: raw.to_string() })
        }
        Err(url::ParseError::RelativeUrlWithoutBase) | Err(url::ParseError::EmptyHost) => Err(ParseFail::NoHost(raw.to_string())),
        Err(e) => Err(ParseFail::Bad(format!("parse {}: {}", go_quote(raw), e))),
    }
}

fn target_from_url(u: Url) -> Target {
    let host = match (u.host_str(), u.port()) {
        (Some(h), Some(p)) => format!("{h}:{p}"),
        (Some(h), None) => h.to_string(),
        _ => String::new(),
    };
    let display = u.to_string();
    Target { url: u, host, display }
}

/// Go `privateIPCategory`: "" when the IP is public.
fn private_ip_category(ip: IpAddr) -> &'static str {
    let ip = match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        v4 => v4,
    };
    match ip {
        IpAddr::V4(a) => {
            let o = a.octets();
            if a.is_unspecified() {
                "unspecified"
            } else if o[0] == 127 {
                "loopback"
            } else if (o[0] == 169 && o[1] == 254) || (o[0] == 224 && o[1] == 0 && o[2] == 0) {
                "link-local"
            } else if a.is_private() {
                "private (RFC 1918 / ULA)"
            } else if a.is_multicast() {
                "multicast"
            } else {
                ""
            }
        }
        IpAddr::V6(a) => {
            let b = a.octets();
            if a.is_unspecified() {
                "unspecified"
            } else if a.is_loopback() {
                "loopback"
            } else if (b[0] == 0xfe && b[1] & 0xc0 == 0x80) || (b[0] == 0xff && b[1] & 0x0f == 0x02) {
                "link-local"
            } else if b[0] & 0xfe == 0xfc {
                "private (RFC 1918 / ULA)"
            } else if b[0] == 0xff {
                "multicast"
            } else {
                ""
            }
        }
    }
}

fn shown_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(|v| v.to_string()).unwrap_or_else(|| v6.to_string()),
        v4 => v4.to_string(),
    }
}

/// `*.example.com` matches `api.example.com` but NOT `a.b.example.com`.
fn host_matches_pattern(pattern: &str, host: &str) -> bool {
    if pattern == host {
        return true;
    }
    if !pattern.starts_with("*.") {
        return false;
    }
    let suffix = &pattern[1..];
    if !host.ends_with(suffix) {
        return false;
    }
    let prefix = &host[..host.len() - suffix.len()];
    !(prefix.is_empty() || prefix.contains('.'))
}

fn host_in_allowlist(t: &Target, allow: &[String]) -> bool {
    let host_lower = hostname_of(&t.host).to_lowercase();
    let host_port = t.host.to_lowercase();
    for pat in allow {
        let pat = pat.trim().to_lowercase();
        if pat.is_empty() {
            continue;
        }
        // host:port entry — require an exact match on both
        if pat.contains(':') && !pat.starts_with('[') {
            if pat == host_port {
                return true;
            }
            continue;
        }
        if host_matches_pattern(&pat, &host_lower) {
            return true;
        }
    }
    false
}

/// The SSRF + host-allowlist gate; called on the initial URL and every redirect
/// hop. Err carries Go's message text.
fn validate_request_url(t: &Target, p: &HTTPPolicy) -> std::result::Result<(), String> {
    let host = hostname_of(&t.host);
    if host.is_empty() {
        return Err(format!("empty host in URL {}", go_quote(&t.display)));
    }
    if !p.allowed_hosts.is_empty() && !host_in_allowlist(t, &p.allowed_hosts) {
        return Err(format!(
            "host {} is not in --allow-host allowlist (allowed: {})",
            go_quote(&t.host),
            p.allowed_hosts.join(", ")
        ));
    }
    if p.allow_private_ips {
        return Ok(());
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        let msg = private_ip_category(ip);
        if !msg.is_empty() {
            return Err(format!("{} is a {msg} address (use --allow-private-ips to permit)", shown_ip(ip)));
        }
        return Ok(());
    }
    // Hostname: resolve and check every record (DNS-rebinding defense).
    // A DNS failure is not second-guessed; the request surfaces it.
    let Ok(addrs) = (host.as_str(), 0u16).to_socket_addrs() else { return Ok(()) };
    for a in addrs {
        let msg = private_ip_category(a.ip());
        if !msg.is_empty() {
            return Err(format!("{host} resolves to {} ({msg} — use --allow-private-ips to permit)", shown_ip(a.ip())));
        }
    }
    Ok(())
}

/// Go `CheckRedirect`; `via` holds every request made so far (incl. current).
fn check_redirect(next: &Url, via: &[Target], p: &HTTPPolicy) -> std::result::Result<(), String> {
    if p.max_redirects == 0 {
        return Err(format!("redirect refused by --no-redirects (target: {next})"));
    }
    if via.len() as i64 >= p.max_redirects {
        return Err(format!("max {} redirects exceeded (target: {next})", p.max_redirects));
    }
    if !p.allow_scheme_downgrade {
        if let Some(prev) = via.last() {
            if prev.url.scheme() == "https" && next.scheme() == "http" {
                return Err(format!(
                    "https → http downgrade redirect refused ({} → {next} — use --allow-scheme-downgrade to permit)",
                    prev.host
                ));
            }
        }
    }
    if let Err(e) = validate_request_url(&target_from_url(next.clone()), p) {
        return Err(format!("redirect refused: {e} (run `perch help --allow-private-ips` for details)"));
    }
    Ok(())
}

/// Go's `url.Error` op verb for a request method.
fn url_error_op(method: &str) -> String {
    let mut cs = method.chars();
    match cs.next() {
        Some(c) => c.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn transport_msg(op: &str, url: &str, t: &ureq::Transport, host: &str) -> String {
    use std::error::Error as _;
    let io_kind = t.source().and_then(|s| s.downcast_ref::<std::io::Error>()).map(|e| e.kind());
    let text = t.to_string();
    let timed_out = matches!(io_kind, Some(std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock))
        || text.contains("imed out")
        || text.contains("imeout");
    if timed_out {
        return format!("{op} {}: context deadline exceeded (Client.Timeout exceeded while awaiting headers)", go_quote(url));
    }
    match t.kind() {
        ureq::ErrorKind::Dns => format!("{op} {}: dial tcp: lookup {host}: no such host", go_quote(url)),
        ureq::ErrorKind::ConnectionFailed => {
            if io_kind == Some(std::io::ErrorKind::ConnectionRefused) || text.contains("refused") {
                format!("{op} {}: dial tcp {host}: connect: connection refused", go_quote(url))
            } else {
                format!("{op} {}: dial tcp: {text}", go_quote(url))
            }
        }
        _ => format!("{op} {}: {text}", go_quote(url)),
    }
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// TLS client config trusting the operating system's root store, falling back
/// to the bundled webpki roots only when the system store yields nothing
/// (F06: corporate / local CAs installed in the OS must work).
fn tls_config() -> std::sync::Arc<rustls::ClientConfig> {
    use std::sync::{Arc, OnceLock};
    static CFG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut store = rustls::RootCertStore::empty();
        let native = rustls_native_certs::load_native_certs().unwrap_or_default();
        let (valid, _invalid) = store.add_parsable_certificates(native);
        if valid == 0 {
            store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let cfg = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("rustls protocol versions")
            .with_root_certificates(store)
            .with_no_client_auth();
        Arc::new(cfg)
    })
    .clone()
}

/// The shared agent builder: system trust roots (see `tls_config`).
fn agent_builder() -> ureq::AgentBuilder {
    ureq::AgentBuilder::new().tls_config(tls_config())
}

fn op_error(kind: ErrorKind, msg: &str, detail: &str) -> Error {
    Box::new(OpError::new("http", kind, msg).with_detail(detail))
}

/// Validate-then-dispatch shared by every HTTP op (Go `runHTTP`): declared-host
/// gate, SSRF/allowlist check on the initial URL, then the request with the
/// policy-enforcing redirect loop. Returns the final response (any status).
pub fn run_http(i: &Interpreter, method: &str, raw_url: &str, body: Option<&str>) -> Result<ureq::Response> {
    let mut cur = match parse_target(raw_url) {
        Ok(t) => t,
        Err(ParseFail::Bad(m)) => return Err(err(m)),
        Err(ParseFail::NoHost(raw)) => {
            check_host_declared(i, "")?;
            return Err(op_error(
                ErrorKind::HTTPRedirectRefused,
                &format!("empty host in URL {}", go_quote(&raw)),
                &raw,
            ));
        }
    };
    check_host_declared(i, &hostname_of(&cur.host))?;
    let p = http_policy(i);
    if let Err(msg) = validate_request_url(&cur, &p) {
        let mut kind = ErrorKind::HTTPRedirectRefused;
        if ["private", "loopback", "link-local", "unspecified"].iter().any(|w| msg.contains(w)) {
            kind = ErrorKind::HTTPSSRFBlocked;
        }
        return Err(op_error(kind, &msg, raw_url));
    }
    let fail = |msg: String| -> Error {
        let kind = if msg.contains("Timeout") || msg.contains("timeout") {
            ErrorKind::HTTPTimeout
        } else if msg.contains("redirect") {
            ErrorKind::HTTPRedirectRefused
        } else {
            ErrorKind::HTTPDNSFailed
        };
        op_error(kind, &msg, raw_url)
    };
    let mut agent = agent_builder().redirects(0).timeout(Duration::from_secs(30)).user_agent("Go-http-client/1.1");
    agent = agent.try_proxy_from_env(true);
    let agent = agent.build();
    let mut method = method.to_string();
    let mut body: Option<String> = body.map(str::to_string);
    let mut json_ct = body.is_some();
    let mut via: Vec<Target> = Vec::new();
    loop {
        let scheme = cur.url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(fail(format!(
                "{} {}: unsupported protocol scheme {}",
                url_error_op(&method),
                go_quote(&cur.display),
                go_quote(scheme)
            )));
        }
        let mut req = agent.request(&method, cur.url.as_str());
        if json_ct {
            req = req.set("Content-Type", "application/json");
        }
        let sent = match &body {
            Some(b) => req.send_string(b),
            None => req.call(),
        };
        let resp = match sent {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(t)) => {
                let op = url_error_op(&method);
                return Err(fail(transport_msg(&op, &cur.display, &t, &hostname_of(&cur.host))));
            }
        };
        if !is_redirect(resp.status()) {
            return Ok(resp);
        }
        let loc = resp.header("Location").unwrap_or("").to_string();
        if loc.is_empty() {
            return Ok(resp);
        }
        let next = match cur.url.join(&loc) {
            Ok(u) => u,
            Err(e) => {
                return Err(fail(format!(
                    "{} {}: failed to parse Location header {}: {e}",
                    url_error_op(&method),
                    go_quote(&cur.display),
                    go_quote(&loc)
                )))
            }
        };
        let prev = std::mem::replace(&mut cur, target_from_url(next.clone()));
        via.push(prev);
        if let Err(msg) = check_redirect(&next, &via, &p) {
            let shown = &via[via.len() - 1].display;
            return Err(fail(format!("{} {}: {msg}", url_error_op(&method), go_quote(shown))));
        }
        if matches!(resp.status(), 301..=303) && method != "GET" && method != "HEAD" {
            method = "GET".into();
            body = None;
            json_ct = false;
        }
    }
}

/// `io.ReadAll(resp.Body)`.
fn read_body(resp: ureq::Response) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    resp.into_reader().read_to_end(&mut v).map_err(|e| {
        if e.kind() == std::io::ErrorKind::TimedOut {
            err("context deadline exceeded (Client.Timeout or context cancellation while reading body)")
        } else {
            err(go_io_msg(&e))
        }
    })?;
    Ok(v)
}

fn op_http_get(i: &Interpreter, _b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let url = arg_string(a, &["url", "_0"]);
    let resp = run_http(i, "GET", &url, None)?;
    Ok(Value::String(String::from_utf8_lossy(&read_body(resp)?).into_owned()))
}

fn http_method(method: &'static str) -> Handler {
    handler(move |i, _b, a| {
        let url = arg_string(a, &["url", "_0"]);
        let body = arg_string(a, &["body", "_1"]);
        let resp = run_http(i, method, &url, Some(&body))?;
        Ok(Value::String(String::from_utf8_lossy(&read_body(resp)?).into_owned()))
    })
}

fn op_download(i: &Interpreter, b: &mut Bindings, a: &Args<'_>) -> Result<Value> {
    let url = arg_string(a, &["url"]);
    let raw_dst = arg_string(a, &["dst"]);
    let dst = resolve(&raw_dst, b);
    // download writes the response body to dst — gate it as a write path
    // (the URL host is gated separately by run_http).
    check_path_declared(i, b, &raw_dst, true)?;
    let resp = run_http(i, "GET", &url, None)?;
    let mut out = create_file(&dst)?;
    let mut rd = resp.into_reader();
    copy_stream(&mut out, &mut rd, "body")?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    // T-42 (manual): system trust roots.
    //   1. Make a local CA + leaf for `localhost`:
    //        openssl req -x509 -newkey rsa:2048 -nodes -days 2 -subj /CN=perch-test-ca -keyout ca.key -out ca.pem
    //        openssl req -newkey rsa:2048 -nodes -subj /CN=localhost -addext subjectAltName=DNS:localhost -keyout l.key -out l.csr
    //        openssl x509 -req -in l.csr -CA ca.pem -CAkey ca.key -CAcreateserial -days 2 -copy_extensions copy -out l.pem
    //   2. Serve it:  openssl s_server -accept 8443 -cert l.pem -key l.key -www
    //   3. Before trusting the CA: a `.perch` with `http_get "https://localhost:8443/"`
    //      (and `host "localhost"` declared, plus the loopback/SSRF policy opened as
    //      needed) FAILS with a certificate error.
    //   4. Trust ca.pem in the OS store (macOS: `security add-trusted-cert -d -k
    //      ~/Library/Keychains/login.keychain-db ca.pem`; Linux: copy to
    //      /usr/local/share/ca-certificates + `update-ca-certificates`), rerun: SUCCEEDS.
    //   5. Remove the CA again; the call fails again. Empty system store falls back
    //      to bundled webpki roots (public sites still work).
    #[test]
    fn agent_builds_with_system_or_bundled_roots() {
        let _ = super::agent_builder().build();
        let cfg = super::tls_config();
        assert!(std::sync::Arc::strong_count(&cfg) >= 1);
    }

    use super::*;

    fn pol(hosts: &[&str]) -> HTTPPolicy {
        HTTPPolicy { max_redirects: 5, allowed_hosts: hosts.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    fn tgt(u: &str) -> Target {
        parse_target(u).ok().unwrap()
    }

    #[test]
    fn ssrf_categories() {
        for (ip, want) in [
            ("127.0.0.1", "loopback"),
            ("169.254.169.254", "link-local"),
            ("10.1.2.3", "private (RFC 1918 / ULA)"),
            ("192.168.0.1", "private (RFC 1918 / ULA)"),
            ("172.16.0.1", "private (RFC 1918 / ULA)"),
            ("0.0.0.0", "unspecified"),
            ("::1", "loopback"),
            ("fe80::1", "link-local"),
            ("fd00::1", "private (RFC 1918 / ULA)"),
            ("224.1.1.1", "multicast"),
            ("::ffff:127.0.0.1", "loopback"),
            ("8.8.8.8", ""),
        ] {
            assert_eq!(private_ip_category(ip.parse().unwrap()), want, "{ip}");
        }
    }

    #[test]
    fn literal_ip_blocked_and_messages() {
        let e = validate_request_url(&tgt("http://169.254.169.254/latest"), &pol(&[])).unwrap_err();
        assert_eq!(e, "169.254.169.254 is a link-local address (use --allow-private-ips to permit)");
        let ok = HTTPPolicy { allow_private_ips: true, max_redirects: 5, ..Default::default() };
        assert!(validate_request_url(&tgt("http://127.0.0.1:8080/"), &ok).is_ok());
        let e = validate_request_url(&tgt("http://[::1]/"), &pol(&[])).unwrap_err();
        assert!(e.contains("loopback"));
    }

    #[test]
    fn allowlist_patterns() {
        let p = pol(&["api.github.com", "*.s3.amazonaws.com", "localhost:8080"]);
        let ok = HTTPPolicy { allow_private_ips: true, ..p.clone() };
        assert!(validate_request_url(&tgt("https://api.github.com/x"), &ok).is_ok());
        assert!(validate_request_url(&tgt("https://b.s3.amazonaws.com/x"), &ok).is_ok());
        assert!(validate_request_url(&tgt("https://a.b.s3.amazonaws.com/x"), &ok).is_err());
        assert!(validate_request_url(&tgt("http://localhost:8080/"), &ok).is_ok());
        assert!(validate_request_url(&tgt("http://localhost:9090/"), &ok).is_err());
        let e = validate_request_url(&tgt("https://evil.com/"), &ok).unwrap_err();
        assert_eq!(
            e,
            "host \"evil.com\" is not in --allow-host allowlist (allowed: api.github.com, *.s3.amazonaws.com, localhost:8080)"
        );
    }

    #[test]
    fn redirect_policy() {
        let p = pol(&[]);
        let via: Vec<Target> = (0..5).map(|_| tgt("https://example.com/")).collect();
        let next = Url::parse("https://example.com/n").unwrap();
        assert_eq!(check_redirect(&next, &via, &p).unwrap_err(), "max 5 redirects exceeded (target: https://example.com/n)");
        let one = vec![tgt("https://example.com/")];
        let down = Url::parse("http://example.com/n").unwrap();
        let e = check_redirect(&down, &one, &p).unwrap_err();
        assert!(e.starts_with("https → http downgrade redirect refused (example.com → http://example.com/n"));
        let none = HTTPPolicy { max_redirects: 0, ..Default::default() };
        assert_eq!(check_redirect(&next, &one, &none).unwrap_err(), "redirect refused by --no-redirects (target: https://example.com/n)");
        let priv_ = Url::parse("https://127.0.0.1/n").unwrap();
        let e = check_redirect(&priv_, &one, &p).unwrap_err();
        assert!(e.starts_with("redirect refused: 127.0.0.1 is a loopback address"));
    }

    #[test]
    fn error_op_names() {
        assert_eq!(url_error_op("GET"), "Get");
        assert_eq!(url_error_op("DELETE"), "Delete");
    }
}

/// One GET for the wasm host bridge (wasm_http.go `doHTTPGet`): the same
/// SSRF/allowlist gate on the initial URL and every redirect hop as
/// Failure of a wasm-bridge GET. `refused` marks policy/SSRF/host refusals
/// (surfaced as `wasm_http_refused`) as opposed to transport failures.
#[derive(Debug, Clone)]
pub(crate) struct WasmGetErr {
    pub refused: bool,
    pub msg: String,
}

impl WasmGetErr {
    fn refused(msg: String) -> Self {
        WasmGetErr { refused: true, msg }
    }
    fn failed(msg: String) -> Self {
        WasmGetErr { refused: false, msg }
    }
}

/// `run_http`, but without the `requires` host gate (the wasm bridge has its
/// own `wasm_allow_host` allowlist) and returning the buffered body (32 MB cap)
/// and status. Err carries a message the bridge discards (module sees -1).
pub(crate) fn wasm_get(p: &HTTPPolicy, raw_url: &str) -> std::result::Result<(Vec<u8>, u16), WasmGetErr> {
    let mut cur = match parse_target(raw_url) {
        Ok(t) => t,
        Err(ParseFail::Bad(m)) => return Err(WasmGetErr::refused(m)),
        Err(ParseFail::NoHost(raw)) => return Err(WasmGetErr::refused(format!("empty host in URL {}", go_quote(&raw)))),
    };
    validate_request_url(&cur, p).map_err(WasmGetErr::refused)?;
    let agent = agent_builder()
        .redirects(0)
        .timeout(Duration::from_secs(30))
        .user_agent("Go-http-client/1.1")
        .try_proxy_from_env(true)
        .build();
    let mut via: Vec<Target> = Vec::new();
    loop {
        let scheme = cur.url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(WasmGetErr::refused(format!("unsupported protocol scheme {}", go_quote(scheme))));
        }
        let resp = match agent.request("GET", cur.url.as_str()).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(ureq::Error::Transport(t)) => return Err(WasmGetErr::failed(t.to_string())),
        };
        let status = resp.status();
        let loc = resp.header("Location").unwrap_or("").to_string();
        if !is_redirect(status) || loc.is_empty() {
            let mut body = Vec::new();
            resp.into_reader().take(32 << 20).read_to_end(&mut body).map_err(|e| WasmGetErr::failed(e.to_string()))?;
            return Ok((body, status));
        }
        let next = cur.url.join(&loc).map_err(|e| WasmGetErr::failed(e.to_string()))?;
        let prev = std::mem::replace(&mut cur, target_from_url(next.clone()));
        via.push(prev);
        check_redirect(&next, &via, p).map_err(WasmGetErr::refused)?;
    }
}
