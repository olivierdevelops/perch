//! Produces a structured NDJSON trace of every op the interpreter dispatches.
//! One line per op call, plus a session-start and session-end record. Designed
//! for two consumers:
//!
//!   1. Security review — every action a `.perch` file took, in order, with
//!      timestamps, durations, args, errors. Same shape as Linux auditd but at
//!      the perch-op level rather than the syscall level.
//!
//!   2. AI-agent supervision — when a `perch-mcp` server runs an agent's
//!      requests, the audit stream is what your monitoring stack sees. Pipe it
//!      into Loki / Datadog / CloudWatch / whatever.
//!
//! Each event is one self-contained JSON object on its own line (keys sorted,
//! as Go's encoder emits maps):
//!
//! ```text
//! {"cli_args":["-target=prod"],"cmd":"deploy","event":"session_start","ts":"2024-…"}
//! {"args":{"_0":"docker …"},"cmd":"deploy","dur_ms":1842,"event":"op","kind":"shell","ok":true,"ts":"…"}
//! {"args":{...},"cmd":"deploy","dur_ms":3,"error":"op disabled by --no-write","event":"op","kind":"write_file","ok":false,"ts":"…"}
//! {"cmd":"deploy","dur_ms":2104,"error":"","event":"session_end","ok":false,"ts":"…"}
//! ```
//!
//! The file is opened with O_APPEND so multiple invocations append to the same
//! log. JSON-encoded so a downstream tool can grep / jq / ingest.
use perch_interpreter::{Error, Interpreter};
use serde_json::{Map, Value};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The destination for audit events. Construct via [`open`] and pass into the
/// interpreter via [`Sink::wire_into`]. Concurrent-safe (a single mutex
/// serialises writes — we want one line per record regardless of caller
/// threads).
/// Closure returned by [`Sink::wire_into`]; call it with the command's final error.
pub type Finalise = Box<dyn Fn(Option<&Error>) + Send + Sync>;

pub struct Sink {
    w: Mutex<Box<dyn Write + Send>>,
}

/// Opens (or creates+appends to) the named file for audit output. "-" means
/// stdout. The file closes when the sink drops.
pub fn open(path: &str) -> Result<Sink, Error> {
    if path == "-" {
        return Ok(Sink::new(Box::new(std::io::stdout())));
    }
    let f = std::fs::OpenOptions::new().append(true).create(true).open(path).map_err(|e| {
        let why = match e.kind() {
            std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
            std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
            _ => e.to_string(),
        };
        Error::from(format!("audit: open {path}: open {path}: {why}"))
    })?;
    Ok(Sink::new(Box::new(f)))
}

impl Sink {
    /// A sink over any writer (tests, in-memory capture).
    pub fn new(w: Box<dyn Write + Send>) -> Sink {
        Sink { w: Mutex::new(w) }
    }

    /// Attaches the sink to an interpreter, returning a `finalise` closure to
    /// call when the command finishes. Records:
    ///
    ///   - one "session_start" event up front (with cmd + cli_args)
    ///   - one "op" event per dispatched op (with kind, args, duration, error)
    ///   - one "session_end" event at finish (with total duration + error)
    pub fn wire_into(
        self: &Arc<Self>,
        i: &mut Interpreter,
        cmd_name: &str,
        cli_args: &[String],
    ) -> Finalise {
        let start = Instant::now();
        self.emit(rec(vec![
            ("event", "session_start".into()),
            ("ts", now_ts().into()),
            ("cmd", cmd_name.into()),
            ("cli_args", Value::Array(cli_args.iter().map(|s| Value::String(s.clone())).collect())),
        ]));
        let sink = self.clone();
        let cmd = cmd_name.to_string();
        i.after_op = Some(Arc::new(move |op, args, _b, _val, err, dur| {
            let mut r = rec(vec![
                ("event", "op".into()),
                ("ts", now_ts().into()),
                ("cmd", cmd.as_str().into()),
                ("kind", op.kind.as_str().into()),
                // The block-op body no longer rides in args (it's a separate
                // parameter), so there is no `_body` sentinel to strip.
                ("args", Value::Object(args.clone())),
                ("dur_ms", Value::from(dur.as_millis() as u64)),
                ("ok", Value::Bool(err.is_none())),
            ]);
            if !op.capture_into.is_empty() {
                r.insert("capture".into(), Value::String(op.capture_into.clone()));
            }
            if let Some(e) = err {
                r.insert("error".into(), Value::String(e.to_string()));
            }
            sink.emit(r);
        }));
        let sink = self.clone();
        let cmd = cmd_name.to_string();
        Box::new(move |final_err| {
            sink.emit(rec(vec![
                ("event", "session_end".into()),
                ("ts", now_ts().into()),
                ("cmd", cmd.as_str().into()),
                ("dur_ms", Value::from(start.elapsed().as_millis() as u64)),
                ("ok", Value::Bool(final_err.is_none())),
                ("error", Value::String(final_err.map(|e| e.to_string()).unwrap_or_default())),
            ]));
        })
    }

    /// Writes one record as a single line (Go's `json.Encoder` shape: sorted
    /// keys, no HTML escaping, trailing newline).
    pub fn emit(&self, record: Map<String, Value>) {
        let mut line = serde_json::to_string(&sorted(Value::Object(record))).unwrap_or_default();
        line = line.replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029");
        line.push('\n');
        let mut w = self.w.lock().unwrap_or_else(|e| e.into_inner());
        let _ = w.write_all(line.as_bytes());
        let _ = w.flush();
    }
}

fn rec(pairs: Vec<(&str, Value)>) -> Map<String, Value> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

/// Rebuilds every object with keys in sorted order (Go marshals maps sorted).
fn sorted(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut entries: Vec<(String, Value)> = m.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
        other => other,
    }
}

fn now_ts() -> String {
    rfc3339_nano_utc(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO))
}

/// Go's `time.RFC3339Nano` in UTC: fractional seconds with trailing zeros
/// trimmed (omitted entirely when zero).
pub fn rfc3339_nano_utc(since_epoch: Duration) -> String {
    let secs = since_epoch.as_secs() as i64;
    let nanos = since_epoch.subsec_nanos();
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let mut s = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", y, m, d, rem / 3600, rem % 3600 / 60, rem % 60);
    if nanos > 0 {
        let frac = format!("{nanos:09}");
        s.push('.');
        s.push_str(frac.trim_end_matches('0'));
    }
    s.push('Z');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_interpreter::{handler, SharedBuf};
    use perch_domain::{Command, Op, Program};
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn timestamp_format() {
        assert_eq!(rfc3339_nano_utc(Duration::new(0, 0)), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_nano_utc(Duration::new(1_700_000_000, 120_000_000)), "2023-11-14T22:13:20.12Z");
    }

    #[test]
    fn records_session_and_ops() {
        let buf = SharedBuf::new();
        let sink = Arc::new(Sink::new(Box::new(buf.writer())));
        let mut handlers = HashMap::new();
        handlers.insert("ok".to_string(), handler(|_, _, _| Ok(Value::Null)));
        handlers.insert("bad".to_string(), handler(|_, _, _| Err("boom".into())));
        let mut p = Program::default();
        p.commands.insert(
            "go".into(),
            Command {
                name: "go".into(),
                ops: vec![
                    Op { kind: "ok".into(), args: json!({"z": "1", "a": "2"}).as_object().unwrap().clone(), ..Default::default() },
                    Op { kind: "bad".into(), ..Default::default() },
                ],
                ..Default::default()
            },
        );
        let mut i = Interpreter::new(handlers, p);
        let finalise = sink.wire_into(&mut i, "go", &["-x=1".to_string()]);
        let res = i.run("go", &[]);
        finalise(res.as_ref().err());
        let lines: Vec<Value> = buf.contents().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0]["event"], "session_start");
        assert_eq!(lines[0]["cli_args"], json!(["-x=1"]));
        assert_eq!(lines[1]["args"], json!({"a": "2", "z": "1"}));
        assert_eq!(lines[1]["ok"], true);
        assert_eq!(lines[2]["error"], "boom");
        assert_eq!(lines[3]["event"], "session_end");
        assert_eq!(lines[3]["ok"], false);
        // Keys are sorted, like Go's map encoding.
        assert!(buf.contents().lines().nth(1).unwrap().starts_with("{\"args\":{\"a\":\"2\",\"z\":\"1\"},\"cmd\":\"go\""));
    }
}
