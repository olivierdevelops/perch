//! Group B op handlers: archive, bundle, compression, encoding, files, hash,
//! http, network, paths, regex, strings, textlines, time, version.
use perch_interpreter::Handler;
use std::collections::HashMap;

mod archive;
pub mod bundle;
mod compression;
mod encoding;
mod files;
mod fsx;
mod gate;
mod gofmt;
mod hash;
pub(crate) mod http;
mod network;
mod paths;
mod regex;
mod strings;
mod textlines;
mod time;
mod util;
mod version;

#[allow(unused_imports)]
pub use bundle::{bundle_hash, bundle_read_file, set_bundle};
#[allow(unused_imports)]
pub use version::version_compare;

/// Registers every group-B op (Go's registerFiles … registerTextLines, minus
/// the group-A and wasm registrations).
pub fn register_all(m: &mut HashMap<String, Handler>) {
    files::register(m);
    compression::register(m);
    http::register(m);
    hash::register(m);
    strings::register(m);
    encoding::register(m);
    time::register(m);
    regex::register(m);
    network::register(m);
    archive::register(m);
    bundle::register(m);
    paths::register(m);
    version::register(m);
    textlines::register(m);
}

#[cfg(test)]
mod tests {
    //! End-to-end checks that drive the registered handlers the way the
    //! interpreter does (file ops, archives, HTTP policy against a local server).
    use super::*;
    use perch_domain::Program;
    use perch_interpreter::{Args, Bindings, HTTPPolicy, Interpreter};
    use serde_json::{json, Map, Value};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn interp(policy: Option<HTTPPolicy>) -> Interpreter {
        let mut m = HashMap::new();
        register_all(&mut m);
        let mut i = Interpreter::new(m, Program::default());
        i.http_policy = policy;
        i
    }

    fn call(i: &Interpreter, b: &mut Bindings, op: &str, args: Value) -> Result<Value, String> {
        let map: Map<String, Value> = args.as_object().unwrap().clone();
        let a = Args { map, body: &[] };
        (i.handlers[op])(i, b, &a).map_err(|e| e.to_string())
    }

    fn tmp() -> String {
        crate::group_b::util::mktemp("perch-b-*", true).unwrap()
    }

    #[test]
    fn files_round_trip() {
        let i = interp(None);
        let d = tmp();
        let mut b = Bindings::new(&d);
        call(&i, &mut b, "write_file", json!({"path": "a.txt", "content": "hello\nworld"})).unwrap();
        assert_eq!(call(&i, &mut b, "read_file", json!({"_0": "a.txt"})).unwrap(), json!("hello\nworld"));
        call(&i, &mut b, "append_line", json!({"_0": "a.txt", "_1": "x"})).unwrap();
        assert_eq!(call(&i, &mut b, "read_file", json!({"_0": "a.txt"})).unwrap(), json!("hello\nworld\nx\n"));
        call(&i, &mut b, "replace_in_file", json!({"_0": "a.txt", "_1": "world", "_2": "there"})).unwrap();
        call(&i, &mut b, "cp", json!({"_0": "a.txt", "_1": "sub/b.txt"})).unwrap();
        call(&i, &mut b, "mv", json!({"_0": "sub/b.txt", "_1": "sub/c.txt"})).unwrap();
        assert_eq!(call(&i, &mut b, "list_dir", json!({"_0": "sub"})).unwrap(), json!("c.txt"));
        assert_eq!(call(&i, &mut b, "glob", json!({"_0": "*/*.txt"})).unwrap(), json!(format!("{d}/sub/c.txt")));
        assert_eq!(call(&i, &mut b, "file_size", json!({"_0": "sub/c.txt"})).unwrap(), json!(14));
        assert_eq!(call(&i, &mut b, "is_dir", json!({"_0": "sub"})).unwrap(), json!(true));
        assert_eq!(call(&i, &mut b, "exists", json!({"_0": "nope"})).unwrap(), json!(false));
        assert_eq!(call(&i, &mut b, "sha256_file", json!({"_0": "a.txt"})).unwrap().as_str().unwrap().len(), 64);
        assert_eq!(
            call(&i, &mut b, "read_file", json!({"_0": "nope"})).unwrap_err(),
            format!("open {d}/nope: no such file or directory")
        );
        assert_eq!(call(&i, &mut b, "chmod", json!({"path": "a.txt", "mode": "9"})).unwrap_err(), "chmod: invalid mode \"9\"");
        call(&i, &mut b, "rm", json!({"_0": "sub"})).unwrap();
        assert_eq!(call(&i, &mut b, "exists", json!({"_0": "sub"})).unwrap(), json!(false));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn archives_round_trip() {
        let i = interp(None);
        let d = tmp();
        let mut b = Bindings::new(&d);
        call(&i, &mut b, "mkdir", json!({"_0": "src/inner"})).unwrap();
        call(&i, &mut b, "write_file", json!({"path": "src/x.txt", "content": "X"})).unwrap();
        call(&i, &mut b, "write_file", json!({"path": "src/inner/y.txt", "content": "YY"})).unwrap();
        call(&i, &mut b, "tar_create", json!({"src": "src", "dst": "a.tar.gz"})).unwrap();
        call(&i, &mut b, "tar_extract", json!({"src": "a.tar.gz", "dst": "out_tar"})).unwrap();
        assert_eq!(call(&i, &mut b, "read_file", json!({"_0": "out_tar/inner/y.txt"})).unwrap(), json!("YY"));
        call(&i, &mut b, "zip_create", json!({"src": "src", "dst": "a.zip"})).unwrap();
        call(&i, &mut b, "zip_extract", json!({"src": "a.zip", "dst": "out_zip"})).unwrap();
        assert_eq!(call(&i, &mut b, "read_file", json!({"_0": "out_zip/inner/y.txt"})).unwrap(), json!("YY"));
        call(&i, &mut b, "gzip", json!({"src": "src/x.txt", "dst": "x.gz"})).unwrap();
        call(&i, &mut b, "ungzip", json!({"src": "x.gz", "dst": "x2.txt"})).unwrap();
        assert_eq!(call(&i, &mut b, "read_file", json!({"_0": "x2.txt"})).unwrap(), json!("X"));
        assert_eq!(call(&i, &mut b, "ungzip", json!({"src": "src/x.txt", "dst": "z"})).unwrap_err(), "gzip: invalid header");
        assert_eq!(call(&i, &mut b, "zip_extract", json!({"src": "src/x.txt", "dst": "z"})).unwrap_err(), "zip: not a valid zip file");
        std::fs::remove_dir_all(&d).unwrap();
    }

    /// Reads one full HTTP request (headers plus any Content-Length body) so the
    /// server never closes a socket with unread data, which would RST the client.
    fn read_request(s: &mut std::net::TcpStream) {
        let mut data = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = s.read(&mut buf).unwrap_or(0);
            if n == 0 {
                return;
            }
            data.extend_from_slice(&buf[..n]);
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&data[..end]).to_lowercase();
                let want = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
                    .unwrap_or(0);
                if data.len() >= end + 4 + want {
                    return;
                }
            }
        }
    }

    /// Serves `count` canned responses on a local port.
    fn serve(responses: Vec<String>) -> (u16, std::thread::JoinHandle<()>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            for r in responses {
                let (mut s, _) = l.accept().unwrap();
                read_request(&mut s);
                s.write_all(r.as_bytes()).unwrap();
            }
        });
        (port, h)
    }

    fn ok(body: &str) -> String {
        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
    }

    #[test]
    fn http_blocks_private_by_default_with_kind() {
        let i = interp(None);
        let mut b = Bindings::new("/");
        let e = call(&i, &mut b, "http_get", json!({"_0": "http://169.254.169.254/latest"})).unwrap_err();
        assert_eq!(e, "http_ssrf_blocked: 169.254.169.254 is a link-local address (use --allow-private-ips to permit) (http://169.254.169.254/latest)");
        let e = call(&i, &mut b, "http_get", json!({"_0": "http://localhost:1/"})).unwrap_err();
        assert!(e.starts_with("http_ssrf_blocked:"), "{e}");
    }

    #[test]
    fn http_local_get_post_and_redirects() {
        let pol = HTTPPolicy { max_redirects: 5, allow_private_ips: true, ..Default::default() };
        let i = interp(Some(pol));
        let mut b = Bindings::new("/");
        let (port, h) = serve(vec![ok("pong")]);
        assert_eq!(call(&i, &mut b, "http_get", json!({"_0": format!("http://127.0.0.1:{port}/")})).unwrap(), json!("pong"));
        h.join().unwrap();
        // 302 -> follow to /final
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            for n in 0..2 {
                let (mut s, _) = l.accept().unwrap();
                read_request(&mut s);
                let r = if n == 0 {
                    format!("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{port}/final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                } else {
                    ok("done")
                };
                s.write_all(r.as_bytes()).unwrap();
            }
        });
        assert_eq!(call(&i, &mut b, "http_post", json!({"_0": format!("http://127.0.0.1:{port}/"), "_1": "{}"})).unwrap(), json!("done"));
        h.join().unwrap();
        // refuse any redirect
        let none = interp(Some(HTTPPolicy { max_redirects: 0, allow_private_ips: true, ..Default::default() }));
        let (port, h) = serve(vec![format!("HTTP/1.1 302 Found\r\nLocation: /x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")]);
        let e = call(&none, &mut b, "http_get", json!({"_0": format!("http://127.0.0.1:{port}/")})).unwrap_err();
        assert!(e.starts_with("http_redirect_refused: Get \"http://127.0.0.1:"), "{e}");
        assert!(e.contains("redirect refused by --no-redirects (target: http://127.0.0.1:"), "{e}");
        h.join().unwrap();
        // status
        let (port, h) = serve(vec![ok("")]);
        assert_eq!(call(&i, &mut b, "http_status", json!({"_0": format!("http://127.0.0.1:{port}/")})).unwrap(), json!(200));
        h.join().unwrap();
    }

    #[test]
    fn http_dns_failure_kind() {
        let i = interp(Some(HTTPPolicy { max_redirects: 5, ..Default::default() }));
        let mut b = Bindings::new("/");
        let e = call(&i, &mut b, "http_get", json!({"_0": "http://no-such-host.invalid/"})).unwrap_err();
        assert!(e.starts_with("http_dns_failed:"), "{e}");
    }

    #[test]
    fn pure_ops_via_registry() {
        let i = interp(None);
        let mut b = Bindings::new("/");
        assert_eq!(call(&i, &mut b, "version_ge", json!({"_0": "v1.29.3", "_1": "1.28.0"})).unwrap(), json!("true"));
        assert_eq!(call(&i, &mut b, "version_extract", json!({"_0": "kubectl v1.29.3-rc.1 x"})).unwrap(), json!("1.29.3-rc.1"));
        let e = call(&i, &mut b, "assert_version", json!({"lhs": "1.0", "op": "ge", "rhs": "2.0"})).unwrap_err();
        assert_eq!(e, "assert_failed: version assertion failed: \"1.0\" >= \"2.0\" is not true (got=\"1.0\" op=>= want=\"2.0\")");
        assert_eq!(call(&i, &mut b, "json_get", json!({"_0": "{\"a\":{\"b\":5}}", "_1": "a.b"})).unwrap(), json!(5));
        assert_eq!(call(&i, &mut b, "json_stringify", json!({"_0": "a<b"})).unwrap(), json!("\"a\\u003cb\""));
        assert_eq!(call(&i, &mut b, "regex_find_all", json!({"_0": "\\d+", "_1": "a1b22"})).unwrap(), json!(["1", "22"]));
        assert_eq!(call(&i, &mut b, "regex_replace", json!({"_0": "(a)(b)", "_1": "abab", "_2": "$2$1"})).unwrap(), json!("baba"));
        assert_eq!(call(&i, &mut b, "path_with_ext", json!({"_0": "a/b.txt", "_1": "md"})).unwrap(), json!("a/b.md"));
        assert_eq!(call(&i, &mut b, "path_join", json!({"_0": "a", "_1": "../b", "_2": "c"})).unwrap(), json!("b/c"));
        assert_eq!(call(&i, &mut b, "base64_encode", json!({"_0": "hi"})).unwrap(), json!("aGk="));
        assert_eq!(call(&i, &mut b, "format", json!({"_0": "n=%d", "_1": 7})).unwrap(), json!("n=7"));
        assert!(call(&i, &mut b, "bundle_hash", json!({})).unwrap_err().starts_with("no embedded bundle"));
        assert_eq!(call(&i, &mut b, "unix_to_iso", json!({"_0": 0})).unwrap(), json!("1970-01-01T00:00:00Z"));
    }
}
