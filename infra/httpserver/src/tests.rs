use super::*;
use crate::api::Shared;
use std::io::{Read, Write};
use std::net::TcpStream;

const SRC: &str = r#"name "demo"
about "A demo"
version "1.2"

G = "x<y"
N = 3

command hello
    description "Say hi"
    do
        print "hi there"
        eprintln "to stderr"
    end
end

command boom
    do
        fail "kaboom"
    end
end

command secret
    private
    do
        print "x"
    end
end
"#;

fn start(src: &str) -> u16 {
    let p = perch_capyloader::load_from_string(src).unwrap();
    let shared = Arc::new(Shared::new(
        p,
        perch_ops::all_handlers(),
        "/tmp/demo.perch".into(),
        Some(Box::new(|| perch_ops::all_handlers().keys().cloned().collect())),
    ));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let _ = serve_on(listener, shared);
    });
    port
}

fn raw(port: u16, req: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(req.as_bytes()).unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

fn get(port: u16, path: &str) -> String {
    raw(port, &format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"))
}

fn post(port: u16, path: &str, body: &str) -> String {
    raw(
        port,
        &format!("POST {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()),
    )
}

fn body_of(resp: &str) -> &str {
    resp.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("")
}

fn dechunk(b: &str) -> String {
    let mut out = String::new();
    let mut rest = b;
    while let Some((len, tail)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(len, 16).unwrap();
        if n == 0 {
            break;
        }
        out.push_str(&tail[..n]);
        rest = &tail[n + 2..];
    }
    out
}

#[test]
fn program_endpoint_shape() {
    let port = start(SRC);
    let r = get(port, "/api/program");
    assert!(r.starts_with("HTTP/1.1 200 OK\r\n"), "{r}");
    assert!(r.contains("Content-Type: application/json\r\n"));
    assert!(r.contains("Content-Length: "));
    let want = concat!(
        r#"{"commands":[{"name":"boom"},{"name":"hello","description":"Say hi"}],"description":"A demo","#,
        r#""globals":[{"name":"G","type":"string","value":"x\u003cy"},{"name":"N","type":"int","value":"3"}],"#,
        r#""name":"demo","path":"/tmp/demo.perch","version":"1.2"}"#,
        "\n"
    );
    assert_eq!(body_of(&r), want);
}

#[test]
fn check_endpoint() {
    let port = start(SRC);
    let r = post(port, "/api/check", "");
    assert!(body_of(&r).starts_with(r#"{"errors":0,"issues":["#), "{r}");
    assert!(body_of(&r).contains(r#""ok":true"#));
}

#[test]
fn exec_streams_ndjson() {
    let port = start(SRC);
    let r = post(port, "/api/exec", r#"{"command":"hello"}"#);
    assert!(r.contains("Content-Type: application/x-ndjson\r\n"), "{r}");
    assert!(r.contains("Cache-Control: no-cache\r\n"));
    assert!(r.contains("Transfer-Encoding: chunked\r\n"));
    let body = dechunk(body_of(&r));
    assert_eq!(
        body,
        "{\"kind\":\"status\",\"msg\":\"started\"}\n{\"kind\":\"out\",\"msg\":\"hi there\"}\n{\"kind\":\"err\",\"msg\":\"to stderr\"}\n{\"kind\":\"status\",\"msg\":\"ok\"}\n"
    );
}

#[test]
fn exec_failure_reports_error() {
    let port = start(SRC);
    let body = dechunk(body_of(&post(port, "/api/exec", r#"{"command":"boom"}"#)));
    assert!(body.contains(r#"{"kind":"err","msg":"user_fail: kaboom"}"#), "{body}");
    assert!(body.ends_with("{\"kind\":\"status\",\"msg\":\"error\"}\n"));
}

#[test]
fn exec_errors() {
    let port = start(SRC);
    let r = get(port, "/api/exec");
    assert!(r.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));
    assert_eq!(body_of(&r), "POST only\n");
    assert!(r.contains("X-Content-Type-Options: nosniff\r\n"));
    let r = post(port, "/api/exec", r#"{"command":"secret"}"#);
    assert!(r.starts_with("HTTP/1.1 404 Not Found\r\n"));
    assert_eq!(body_of(&r), "command not found\n");
    let r = post(port, "/api/exec", "");
    assert!(r.starts_with("HTTP/1.1 400 Bad Request\r\n"));
    assert_eq!(body_of(&r), "EOF\n");
    let r = post(port, "/api/exec", r#"{"command":5}"#);
    assert_eq!(body_of(&r), "json: cannot unmarshal number into Go struct field execRequest.command of type string\n");
}

#[test]
fn simulate_endpoint() {
    let port = start(SRC);
    let r = post(port, "/api/simulate", r#"{"command":"hello"}"#);
    let want = "{\"ok\":true,\"report\":\"── command hello — Say hi\\n✓ print \\\"hi there\\\"\\n✓ eprintln \\\"to stderr\\\"\\n\\nsummary: 2 will-run · 0 will-fail · 0 uncertain\\n\"}\n";
    assert_eq!(body_of(&r), want);
    let r = post(port, "/api/simulate", r#"{"command":"hello","fixture":{"scenarios":[{"name":"a"}]}}"#);
    assert!(body_of(&r).contains("═══ Scenario: a ═══"), "{r}");
    let r = get(port, "/api/simulate");
    assert!(r.starts_with("HTTP/1.1 405"));
}

#[test]
fn scan_endpoint_shape() {
    let port = start(SRC);
    let r = get(port, "/api/scan");
    let body = if r.contains("Transfer-Encoding: chunked") { dechunk(body_of(&r)) } else { body_of(&r).to_string() };
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["risk"]["score"], "SAFE");
    assert!(v["findings"].is_null());
    assert!(v["capabilities"]["shell_bins"].as_object().unwrap().is_empty());
}

#[test]
fn index_page_and_fallback() {
    let port = start(SRC);
    for path in ["/", "/whatever", "/api/program/"] {
        let r = get(port, path);
        assert!(r.contains("Content-Type: text/html; charset=utf-8\r\n"), "{path}");
        let body = dechunk(body_of(&r));
        assert!(body.contains("<title>demo — perch</title>"));
        assert!(body.contains(r#"data-name="hello""#));
        assert!(!body.contains("secret"));
        assert!(!body.contains("<!--"));
    }
}

#[test]
fn unclean_paths_redirect() {
    let port = start(SRC);
    let r = get(port, "/foo/../api/program?x=1");
    assert!(r.starts_with("HTTP/1.1 301 Moved Permanently\r\n"));
    assert!(r.contains("Location: /api/program?x=1\r\n"));
    assert_eq!(body_of(&r), "<a href=\"/api/program?x=1\">Moved Permanently</a>.\n\n");
    let r = post(port, "/x//y", "");
    assert!(r.starts_with("HTTP/1.1 301"));
    assert_eq!(body_of(&r), "");
}

#[test]
fn keep_alive_serves_multiple_requests() {
    let port = start(SRC);
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    for _ in 0..2 {
        s.write_all(b"GET /api/check HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut buf = vec![0u8; 4096];
        let mut got = Vec::new();
        loop {
            let n = s.read(&mut buf).unwrap();
            got.extend_from_slice(&buf[..n]);
            if String::from_utf8_lossy(&got).contains("\"warnings\":") {
                break;
            }
        }
        assert!(String::from_utf8_lossy(&got).starts_with("HTTP/1.1 200 OK"));
    }
}

#[test]
fn malformed_requests() {
    let port = start(SRC);
    let r = raw(port, "GARBAGE\r\n\r\n");
    assert!(r.starts_with("HTTP/1.1 400 Bad Request\r\n"));
    assert!(r.ends_with("400 Bad Request"));
    let r = raw(port, "GET / HTTP/1.1\r\n\r\n");
    assert!(r.ends_with("400 Bad Request: missing required Host header"));
    let r = get(port, "/a%zz");
    assert!(r.ends_with("400 Bad Request"));
}

#[test]
fn head_has_no_body() {
    let port = start(SRC);
    let r = raw(port, "HEAD /api/program HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
    assert!(r.contains("Content-Length: "));
    assert_eq!(body_of(&r), "");
}

fn render_golden(program_json: &str, path: &str) -> String {
    let p: Program = serde_json::from_str(program_json).unwrap();
    let mut cmds: Vec<&perch_domain::Command> = p.commands.values().filter(|c| !c.modifiers.private).collect();
    cmds.sort_by(|a, b| a.name.cmp(&b.name));
    let root = crate::template::root_data(&p, &cmds, path);
    crate::template::Template::parse(include_str!("../index.html")).execute(&root).0
}

#[test]
fn template_matches_go_html_template() {
    let path = "/tmp/a\"b<c>&d/e.perch";
    let got = render_golden(include_str!("../testdata/program_noargs.json"), path);
    assert_eq!(got, include_str!("../testdata/program_noargs.golden.html"));
}

#[test]
fn template_renders_complete_page_for_arg_commands() {
    // F07 / T-43: the label/input ids use the arg's own name (`{{.Name}}`);
    // the Go original used `{{$.Name}}` (root data, no such field), which made
    // Execute fail and truncated the page right after `<label for="`.
    let path = "/tmp/a\"b<c>&d/e.perch";
    let got = render_golden(include_str!("../testdata/program_args.json"), path);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/program_args.golden.html");
        std::fs::write(p, &got).unwrap();
    }
    assert!(got.trim_end().ends_with("</html>"), "page truncated: ...{}", &got[got.len().saturating_sub(120)..]);
    for arg in ["n", "f", "v", "rest", "s"] {
        assert!(got.contains(&format!("<label for=\"{arg}\">-{arg}</label>")), "missing label for {arg}");
        assert!(got.contains(&format!("id=\"{arg}\"")), "missing input id {arg}");
    }
    assert!(!got.contains("{{"), "unrendered template action");
    assert_eq!(got, include_str!("../testdata/program_args.golden.html"));
}
