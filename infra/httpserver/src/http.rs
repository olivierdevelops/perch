//! A small blocking HTTP/1.1 server that reproduces the parts of Go's
//! `net/http` behaviour the perch UI relies on: keep-alive, `Content-Length`
//! for small bodies vs chunked for large ones, per-write flushing for streams,
//! `Date` header, `100-continue`, and Go's malformed-request replies.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// Go's `bufferBeforeChunkingSize`: bodies up to this size get a Content-Length.
const BUFFER_BEFORE_CHUNKING: usize = 2048;
/// Go's `DefaultMaxHeaderBytes` (1 MiB) plus slack.
const MAX_HEADER_BYTES: usize = (1 << 20) + 4096;

pub struct Request {
    pub method: String,
    /// Percent-decoded URL path.
    pub path: String,
    pub raw_query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub type Headers = Vec<(String, String)>;

pub enum Reply {
    Full { status: u16, headers: Headers, body: Vec<u8> },
    /// A chunked stream: the closure writes through the sink; each write is
    /// flushed to the client immediately.
    Stream { headers: Headers, run: Box<dyn FnOnce(ChunkSink) + Send> },
}

pub type Handler = Arc<dyn Fn(&Request) -> Reply + Send + Sync>;

struct SinkInner {
    stream: TcpStream,
    head: Option<Vec<u8>>,
    discard: bool,
}

/// Writes HTTP/1.1 chunks to the connection, sending the pending response
/// head with the first write.
#[derive(Clone)]
pub struct ChunkSink(Arc<Mutex<SinkInner>>);

impl ChunkSink {
    /// Writes `data` as one chunk and flushes. Errors are ignored by callers
    /// (Go handlers discard write errors too).
    pub fn write(&self, data: &[u8]) -> std::io::Result<()> {
        let mut g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut buf = g.head.take().unwrap_or_default();
        if !data.is_empty() {
            buf.extend_from_slice(format!("{:x}\r\n", data.len()).as_bytes());
            buf.extend_from_slice(data);
            buf.extend_from_slice(b"\r\n");
        }
        if g.discard || buf.is_empty() {
            return Ok(());
        }
        g.stream.write_all(&buf)?;
        g.stream.flush()
    }

    fn finish(&self) -> std::io::Result<()> {
        let mut g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut buf = g.head.take().unwrap_or_default();
        buf.extend_from_slice(b"0\r\n\r\n");
        if g.discard {
            return Ok(());
        }
        g.stream.write_all(&buf)?;
        g.stream.flush()
    }
}

pub fn status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        301 => "Moved Permanently",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        431 => "Request Header Fields Too Large",
        505 => "HTTP Version Not Supported",
        _ => "",
    }
}

fn http_date() -> String {
    chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

enum ReqError {
    Eof,
    Bad(&'static str),
    TooLarge,
    Version,
    Io,
}

fn read_line(r: &mut BufReader<TcpStream>, budget: &mut usize) -> Result<Option<String>, ReqError> {
    let mut line = Vec::new();
    let n = r.read_until(b'\n', &mut line).map_err(|_| ReqError::Io)?;
    if n == 0 {
        return Ok(None);
    }
    if n > *budget {
        return Err(ReqError::TooLarge);
    }
    *budget -= n;
    if !line.ends_with(b"\n") {
        return Err(ReqError::Eof);
    }
    line.pop();
    if line.ends_with(b"\r") {
        line.pop();
    }
    Ok(Some(String::from_utf8_lossy(&line).into_owned()))
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(h, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn header<'a>(h: &'a [(String, String)], name: &str) -> Option<&'a str> {
    h.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn read_request(
    r: &mut BufReader<TcpStream>,
    out: &mut TcpStream,
) -> Result<(Request, bool /* http/1.1 */), ReqError> {
    let mut budget = MAX_HEADER_BYTES;
    let line = loop {
        match read_line(r, &mut budget)? {
            None => return Err(ReqError::Eof),
            Some(l) if l.is_empty() => continue,
            Some(l) => break l,
        }
    };
    let mut parts = line.splitn(3, ' ');
    let (method, target, proto) = match (parts.next(), parts.next(), parts.next()) {
        (Some(m), Some(t), Some(p)) if !m.is_empty() && !t.is_empty() => (m, t, p),
        _ => return Err(ReqError::Bad("malformed HTTP request")),
    };
    if !method.bytes().all(|c| c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c)) {
        return Err(ReqError::Bad("invalid method"));
    }
    let v11 = match proto {
        "HTTP/1.1" => true,
        "HTTP/1.0" => false,
        p if p.starts_with("HTTP/") => return Err(ReqError::Version),
        _ => return Err(ReqError::Bad("malformed HTTP version")),
    };
    let mut headers = Vec::new();
    loop {
        let Some(l) = read_line(r, &mut budget)? else { return Err(ReqError::Eof) };
        if l.is_empty() {
            break;
        }
        let Some((k, v)) = l.split_once(':') else {
            return Err(ReqError::Bad("malformed MIME header line"));
        };
        if k.is_empty() || k.contains(' ') {
            return Err(ReqError::Bad("malformed MIME header line"));
        }
        headers.push((k.to_string(), v.trim().to_string()));
    }
    if v11 && header(&headers, "Host").is_none() {
        return Err(ReqError::Bad("missing required Host header"));
    }
    // Target: origin-form, absolute-form or asterisk-form.
    let (raw_path, raw_query) = {
        let mut t = target;
        if !t.starts_with('/') && t != "*" {
            match t.find("://") {
                Some(i) => {
                    let rest = &t[i + 3..];
                    t = rest.find('/').map(|j| &rest[j..]).unwrap_or("/");
                }
                None => return Err(ReqError::Bad("invalid URI")),
            }
        }
        match t.split_once('?') {
            Some((p, q)) => (p.to_string(), q.to_string()),
            None => (t.to_string(), String::new()),
        }
    };
    let path = percent_decode(&raw_path).ok_or(ReqError::Bad("invalid URI"))?;

    if header(&headers, "Expect").is_some_and(|v| v.eq_ignore_ascii_case("100-continue")) && v11 {
        let _ = out.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
    }
    let mut body = Vec::new();
    if header(&headers, "Transfer-Encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        loop {
            let Some(sz) = read_line(r, &mut budget)? else { return Err(ReqError::Eof) };
            let hex = sz.split(';').next().unwrap_or("").trim();
            let n = usize::from_str_radix(hex, 16).map_err(|_| ReqError::Bad("invalid chunk size"))?;
            if n == 0 {
                // trailers until blank line
                while let Some(t) = read_line(r, &mut budget)? {
                    if t.is_empty() {
                        break;
                    }
                }
                break;
            }
            let start = body.len();
            body.resize(start + n, 0);
            r.read_exact(&mut body[start..]).map_err(|_| ReqError::Eof)?;
            let mut crlf = [0u8; 2];
            r.read_exact(&mut crlf).map_err(|_| ReqError::Eof)?;
        }
    } else if let Some(cl) = header(&headers, "Content-Length") {
        let n: usize = cl.trim().parse().map_err(|_| ReqError::Bad("bad Content-Length"))?;
        body.resize(n, 0);
        r.read_exact(&mut body).map_err(|_| ReqError::Eof)?;
    }
    Ok((Request { method: method.to_string(), path, raw_query, headers, body }, v11))
}

fn write_head(status: u16, v11: bool, headers: &Headers, extra: &[(&str, String)]) -> Vec<u8> {
    let mut h = format!("HTTP/1.{} {} {}\r\n", if v11 { 1 } else { 0 }, status, status_text(status));
    let mut sorted: Vec<&(String, String)> = headers.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in sorted {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    for (k, v) in extra {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str("\r\n");
    h.into_bytes()
}

fn serve_conn(stream: TcpStream, handler: Handler) {
    let _ = stream.set_nodelay(true);
    let Ok(read_half) = stream.try_clone() else { return };
    let mut out = stream;
    let mut reader = BufReader::new(read_half);
    loop {
        let (req, v11) = match read_request(&mut reader, &mut out) {
            Ok(r) => r,
            Err(ReqError::Eof) | Err(ReqError::Io) => return,
            Err(ReqError::Bad(why)) => {
                let body = if why == "missing required Host header" {
                    format!("400 Bad Request: {why}")
                } else {
                    "400 Bad Request".to_string()
                };
                let _ = write!(
                    out,
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n{body}"
                );
                return;
            }
            Err(ReqError::TooLarge) => {
                let _ = out.write_all(
                    b"HTTP/1.1 431 Request Header Fields Too Large\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n431 Request Header Fields Too Large",
                );
                return;
            }
            Err(ReqError::Version) => {
                let _ = out.write_all(
                    b"HTTP/1.1 505 HTTP Version Not Supported\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\n505 HTTP Version Not Supported",
                );
                return;
            }
        };
        let conn_hdr = header(&req.headers, "Connection").unwrap_or("").to_ascii_lowercase();
        let has = |t: &str| conn_hdr.split(',').any(|x| x.trim() == t);
        let keep = if v11 { !has("close") } else { has("keep-alive") };
        let is_head = req.method == "HEAD";
        let reply = handler(&req);
        match reply {
            Reply::Full { status, headers, body } => {
                let chunked = body.len() > BUFFER_BEFORE_CHUNKING && v11;
                // Go order: user headers (sorted), Date, then either
                // Content-Length + Connection or Connection + Transfer-Encoding.
                let mut extra: Vec<(&str, String)> = vec![("Date", http_date())];
                let conn = ("Connection", "close".to_string());
                if chunked {
                    if !keep {
                        extra.push(conn);
                    }
                    if !is_head {
                        extra.push(("Transfer-Encoding", "chunked".to_string()));
                    }
                } else {
                    extra.push(("Content-Length", body.len().to_string()));
                    if !keep {
                        extra.push(conn);
                    }
                }
                let mut buf = write_head(status, v11, &headers, &extra);
                if !is_head {
                    if chunked {
                        for piece in body.chunks(BUFFER_BEFORE_CHUNKING) {
                            buf.extend_from_slice(format!("{:x}\r\n", piece.len()).as_bytes());
                            buf.extend_from_slice(piece);
                            buf.extend_from_slice(b"\r\n");
                        }
                        buf.extend_from_slice(b"0\r\n\r\n");
                    } else {
                        buf.extend_from_slice(&body);
                    }
                }
                if out.write_all(&buf).is_err() || out.flush().is_err() {
                    return;
                }
            }
            Reply::Stream { headers, run } => {
                // HTTP/1.0 clients can't take chunked bodies: close-delimited instead.
                let mut all: Vec<(&str, String)> = vec![("Date", http_date())];
                if !keep || !v11 {
                    all.push(("Connection", "close".into()));
                }
                if v11 {
                    all.push(("Transfer-Encoding", "chunked".into()));
                }
                let head = write_head(200, v11, &headers, &all);
                let Ok(wr) = out.try_clone() else { return };
                let sink = ChunkSink(Arc::new(Mutex::new(SinkInner { stream: wr, head: Some(head), discard: is_head })));
                let s2 = sink.clone();
                run(sink);
                if v11 {
                    if s2.finish().is_err() {
                        return;
                    }
                } else {
                    return;
                }
            }
        }
        if !keep {
            let _ = out.shutdown(std::net::Shutdown::Write);
            return;
        }
    }
}

/// Accepts connections forever, one thread per connection.
pub fn serve_listener(listener: TcpListener, handler: Handler) -> std::io::Result<()> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let h = handler.clone();
                std::thread::spawn(move || serve_conn(stream, h));
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}
