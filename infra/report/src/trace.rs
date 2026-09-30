use crate::{arg_preview, format_dur};
use perch_domain::Op;
use perch_interpreter::{Error, Tracer};
use serde_json::{Map, Value};
use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

/// The human-readable real-time counterpart to the audit NDJSON stream and the
/// after-run span report. It implements [`Tracer`] and prints each op to a
/// writer the moment it starts (`before`) and again when it finishes
/// (`after`), interleaved with the op's own stdout/stderr.
///
/// Wire via `--trace` from the CLI. Composes with --audit and --report: all
/// three write from the same hook order, just to different sinks with
/// different shapes.
///
/// Indentation reflects nesting — a `parallel` block prints, then each child
/// indents one level under it, then the block's After prints at the parent
/// level. Lets you eyeball "we're now inside the third branch of this retry"
/// without ceremony.
///
/// The stream looks like:
///
/// ```text
/// ▸ shell                  "docker ps -q -f name=…"
/// ▸ if                     lhs=running op=truthy
///   ▸ print                msg="✓ already running"
///   ✓                                                       0µs
/// ✓                                                         12ms
/// ```
///
/// Errors render `✗` and include the error message on the same line.
pub struct LiveTracer {
    inner: Mutex<Inner>,
}

struct Inner {
    w: Box<dyn Write + Send>,
    depth: usize,
}

impl LiveTracer {
    /// A Tracer that prints to `w` as ops fire.
    pub fn new(w: impl Write + Send + 'static) -> LiveTracer {
        LiveTracer { inner: Mutex::new(Inner { w: Box::new(w), depth: 0 }) }
    }
}

impl Tracer for LiveTracer {
    /// Prints the op header.
    fn before(&self, op: &Op, args: &Map<String, Value>) {
        let mut t = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let indent = "  ".repeat(t.depth);
        let _ = writeln!(t.w, "{}▸ {:<20} {}", indent, op.kind, arg_preview(args));
        t.depth += 1;
    }

    /// Prints the outcome with the op's wall-clock duration.
    fn after(&self, op: &Op, _result: &Value, err: Option<&Error>, dur: Duration) {
        let mut t = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        t.depth = t.depth.saturating_sub(1);
        let indent = "  ".repeat(t.depth);
        if let Some(e) = err {
            let _ = writeln!(t.w, "{}✗  {}   ({})", indent, e, format_dur(dur));
            return;
        }
        // Skip the "✓" line for cheap ops that took <1µs — keeps the trace
        // readable on programs with hundreds of trivial ops. Block ops and
        // anything taking real time still print.
        if dur < Duration::from_micros(1) && op.body.is_empty() {
            return;
        }
        let pad = 32usize.saturating_sub(indent.len() * 2);
        let _ = writeln!(t.w, "{}✓{}({})", indent, " ".repeat(pad), format_dur(dur));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_interpreter::SharedBuf;

    #[test]
    fn live_trace_nesting() {
        let buf = SharedBuf::new();
        let t = LiveTracer::new(buf.writer());
        let block = Op { kind: "if".into(), body: vec![Op::default()], ..Default::default() };
        let inner = Op { kind: "print".into(), ..Default::default() };
        let mut a = Map::new();
        a.insert("msg".into(), Value::String("hi".into()));
        t.before(&block, &Map::new());
        t.before(&inner, &a);
        t.after(&inner, &Value::Null, None, Duration::ZERO);
        t.after(&block, &Value::Null, None, Duration::from_millis(12));
        let out = buf.contents();
        assert_eq!(
            out,
            format!("▸ {:<20} \n  ▸ {:<20} \"hi\"\n✓{}(12ms)\n", "if", "print", " ".repeat(32))
        );
    }

    #[test]
    fn live_trace_error() {
        let buf = SharedBuf::new();
        let t = LiveTracer::new(buf.writer());
        let op = Op { kind: "shell".into(), ..Default::default() };
        t.before(&op, &Map::new());
        let e: Error = "boom".into();
        t.after(&op, &Value::Null, Some(&e), Duration::from_millis(2));
        assert!(buf.contents().ends_with("✗  boom   (2ms)\n"));
    }
}
