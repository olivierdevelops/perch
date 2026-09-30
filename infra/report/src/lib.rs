//! Builds a span-shaped trace of an interpreter run and renders it as a tree.
//! Block ops (`parallel`, `retry`, `timeout`, `sandbox`, `cache`, `with_env`,
//! `with_cwd`, `if`, `for_each`) naturally nest the children that ran inside
//! their body. A flat audit stream (infra/audit) is the canonical artifact;
//! this is a human renderer derived from the same hook order.
//!
//! Wire via `Interpreter::tracer`:
//!
//! ```ignore
//! let rec = Arc::new(report::Recorder::new());
//! itp.tracer = Some(rec.clone());
//! // ... run ...
//! rec.render(&mut std::io::stderr());  // print tree once the command returns
//! ```
mod trace;

pub use trace::*;

use perch_domain::Op;
use perch_interpreter::{to_string_value, Error, Tracer};
use serde_json::{Map, Value};
use std::io::Write;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One entry in the span tree. Each op produces one Node; block-op nodes also
/// carry their children. Nodes live in the recorder's arena; `children` and
/// `parent` are indices into it.
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: String,
    pub args: Map<String, Value>,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
    pub start: Instant,
    pub dur: Duration,
    pub ok: bool,
    pub error: String,
    /// Carries through from `Op` so template-expanded children render as
    /// `print (from check_bin)` in the tree.
    pub expanded_from: String,
    pub capture: String,
}

struct State {
    nodes: Vec<Node>,
    cur: usize,
}

/// Builds the span tree as ops execute. The state sits behind a mutex so the
/// recorder is `Sync`, but the tree is still a single stack: the `parallel`
/// block-op runs children on threads, so their spans interleave and appear
/// linearised in the rendered tree (in `after` arrival order, which mirrors
/// their wall-clock completion order).
pub struct Recorder {
    state: Mutex<State>,
}

impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Recorder {
    /// A fresh recorder whose root is a synthetic "command" node. The command
    /// name can be set via [`Recorder::set_root`] before `before` fires for the
    /// first real op.
    pub fn new() -> Recorder {
        let root = Node {
            kind: "command".into(),
            args: Map::new(),
            children: vec![],
            parent: None,
            start: Instant::now(),
            dur: Duration::ZERO,
            ok: false,
            error: String::new(),
            expanded_from: String::new(),
            capture: String::new(),
        };
        Recorder { state: Mutex::new(State { nodes: vec![root], cur: 0 }) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Labels the root span with a command name. Optional.
    pub fn set_root(&self, cmd_name: &str) {
        self.lock().nodes[0].kind = cmd_name.to_string();
    }

    /// Closes out the synthetic root span. Call once after the command returns
    /// so the rendered tree shows the total wall-clock.
    pub fn finish(&self, err: Option<&Error>) {
        let mut s = self.lock();
        let root = &mut s.nodes[0];
        root.dur = root.start.elapsed();
        root.ok = err.is_none();
        if let Some(e) = err {
            root.error = e.to_string();
        }
    }

    /// Writes the tree to `w`. Output is a fixed-pitch ASCII tree with
    /// per-node duration and (on failure) the error message; siblings are
    /// printed in the order they completed.
    pub fn render(&self, w: &mut dyn Write) {
        let s = self.lock();
        let _ = writeln!(w, "── perch trace ─────────────────────────────────");
        render_node(w, &s.nodes, 0, "", true, true);
    }
}

impl Tracer for Recorder {
    /// Pushes a new child onto the current node and makes it current.
    fn before(&self, op: &Op, args: &Map<String, Value>) {
        let mut s = self.lock();
        let cur = s.cur;
        let idx = s.nodes.len();
        s.nodes.push(Node {
            kind: op.kind.clone(),
            args: args.clone(),
            children: vec![],
            parent: Some(cur),
            start: Instant::now(),
            dur: Duration::ZERO,
            ok: false,
            error: String::new(),
            expanded_from: op.expanded_from.clone(),
            capture: op.capture_into.clone(),
        });
        s.nodes[cur].children.push(idx);
        s.cur = idx;
    }

    /// Records the outcome and pops back to the parent.
    fn after(&self, _op: &Op, _result: &Value, err: Option<&Error>, dur: Duration) {
        let mut s = self.lock();
        let cur = s.cur;
        let n = &mut s.nodes[cur];
        n.dur = dur;
        n.ok = err.is_none();
        if let Some(e) = err {
            n.error = e.to_string();
        }
        if let Some(p) = n.parent {
            s.cur = p;
        }
    }
}

/// Writes one node and recurses into its children.
///
///   prefix  — the rope of vertical bars / spaces drawn at this depth
///   is_last — whether THIS node is the last among its parent's children
///   is_root — root has no connector glyph
fn render_node(w: &mut dyn Write, nodes: &[Node], idx: usize, prefix: &str, is_last: bool, is_root: bool) {
    let n = &nodes[idx];
    let connector = if is_last { "└─ " } else { "├─ " };
    let header = node_header(n);
    if is_root {
        let _ = writeln!(w, "{header}");
    } else {
        let _ = writeln!(w, "{prefix}{connector}{header}");
    }
    let mut child_prefix = prefix.to_string();
    if !is_root {
        child_prefix.push_str(if is_last { "   " } else { "│  " });
    }
    if !n.error.is_empty() {
        let _ = writeln!(w, "{}   ↳ error: {}", child_prefix, n.error);
    }
    for (i, &c) in n.children.iter().enumerate() {
        render_node(w, nodes, c, &child_prefix, i == n.children.len() - 1, false);
    }
}

/// The one-line span summary: status glyph, kind, key arg, duration, optional
/// template provenance.
fn node_header(n: &Node) -> String {
    let mut status = if n.ok { "✓" } else { "✗" };
    if n.dur.is_zero() && n.children.is_empty() && n.kind == "command" {
        status = "·";
    }
    let summary = format!("{} {}", n.kind, arg_preview(&n.args)).trim().to_string();
    let mut parts = vec![status.to_string(), summary];
    if !n.capture.is_empty() {
        parts.push(format!("→{}", n.capture));
    }
    parts.push(format!("({})", format_dur(n.dur)));
    if !n.expanded_from.is_empty() {
        parts.push(format!("[from template {}]", n.expanded_from));
    }
    parts.join(" ")
}

/// A short one-line summary of args, prioritising common keys (msg, cmd, path,
/// key, duration, name) so the tree stays scannable for the common ops without
/// dumping every JSON field.
pub(crate) fn arg_preview(args: &Map<String, Value>) -> String {
    if args.is_empty() {
        return String::new();
    }
    for k in ["msg", "cmd", "path", "_0", "key", "duration", "name", "flags"] {
        if let Some(v) = args.get(k) {
            return format!("\"{}\"", truncate(&fmt_v(v), 60));
        }
    }
    // Fall back: stable-ordered key=value list, capped.
    let mut keys: Vec<&String> = args.keys().filter(|k| !k.starts_with('_')).collect();
    keys.sort();
    let mut parts = vec![];
    for k in keys {
        parts.push(format!("{}={}", k, fmt_v(&args[k])));
        if parts.len() >= 2 {
            break;
        }
    }
    parts.join(" ")
}

/// Go's `%v` for an arg value (`<nil>` for null at top level).
fn fmt_v(v: &Value) -> String {
    match v {
        Value::Null => "<nil>".to_string(),
        other => to_string_value(other),
    }
}

/// Cuts at a char boundary at or below `n` bytes.
pub(crate) fn floor_boundary(s: &str, n: usize) -> usize {
    let mut n = n.min(s.len());
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    n
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.len() <= max {
        return s;
    }
    format!("{}...", &s[..floor_boundary(&s, max - 3)])
}

/// Renders a duration as one of: 12ms, 4.21s, 1m02s.
pub fn format_dur(d: Duration) -> String {
    if d < Duration::from_millis(1) {
        format!("{}µs", d.as_micros())
    } else if d < Duration::from_secs(1) {
        format!("{}ms", d.as_millis())
    } else if d < Duration::from_secs(60) {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        format!("{}m{:02}s", d.as_secs() / 60, d.as_secs() % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn op(kind: &str) -> Op {
        Op { kind: kind.into(), ..Default::default() }
    }

    #[test]
    fn dur_formats() {
        assert_eq!(format_dur(Duration::from_micros(5)), "5µs");
        assert_eq!(format_dur(Duration::from_millis(12)), "12ms");
        assert_eq!(format_dur(Duration::from_millis(4210)), "4.21s");
        assert_eq!(format_dur(Duration::from_secs(62)), "1m02s");
    }

    #[test]
    fn renders_nested_tree() {
        let r = Recorder::new();
        r.set_root("deploy");
        r.before(&op("if"), &json!({"lhs": "x"}).as_object().unwrap().clone());
        r.before(&op("print"), &json!({"msg": "hi"}).as_object().unwrap().clone());
        r.after(&op("print"), &Value::Null, None, Duration::from_millis(3));
        r.after(&op("if"), &Value::Null, None, Duration::from_millis(4));
        r.before(&op("shell"), &json!({"cmd": "false"}).as_object().unwrap().clone());
        let e: Error = "exit 1".into();
        r.after(&op("shell"), &Value::Null, Some(&e), Duration::from_millis(9));
        r.finish(Some(&e));
        let mut out = Vec::new();
        r.render(&mut out);
        let s = String::from_utf8(out).unwrap();
        assert!(s.starts_with("── perch trace ─────────────────────────────────\n"));
        assert!(s.contains("✗ deploy ("), "{s}");
        assert!(s.contains("├─ ✓ if lhs=x (4ms)\n"), "{s}");
        assert!(s.contains("│  └─ ✓ print \"hi\" (3ms)\n"), "{s}");
        assert!(s.contains("└─ ✗ shell \"false\" (9ms)\n"), "{s}");
        assert!(s.contains("↳ error: exit 1"), "{s}");
    }
}
