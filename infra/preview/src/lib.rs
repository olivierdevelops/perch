//! Interpreter BeforeOp hooks that let users inspect a command's ops before
//! they run.
//!
//! Two modes:
//!
//!   - DryRun: walk the ops, print each one with its interpolated args, skip
//!     the actual handler. Captures get set to "" so subsequent ${x}
//!     interpolation still works. Side-effect-free preview.
//!
//!   - Ask: like DryRun but interactive — for each op, prompt y/n/a/q (yes /
//!     no / all / quit). 'y' runs THIS op, 'n' skips, 'a' runs everything else
//!     without further asking, 'q' stops.
//!
//! The hook gets the interpolated args, so what the user sees is what the
//! handler would actually receive — no surprises.
use perch_domain::Op;
use perch_interpreter::{go_quote, to_string_value, BeforeOp, OpAction, SharedWriter};
use serde_json::{Map, Value};
use std::io::{BufRead, BufReader, Read};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// A BeforeOp that prints every op and returns `Skip`, so nothing actually
/// executes. Use with --dry-run.
///
/// Block ops (`if`, `parallel`, `retry`, `cache`, `sandbox`, …) have their
/// bodies expanded inline as an indented sub-tree so the user sees EVERY op
/// that could fire, not just the block headers. The displayed args are the
/// already-interpolated values for THIS op; nested ops' args are shown
/// verbatim (their interpolation hasn't run yet since the body wasn't
/// dispatched).
pub fn dry_run_hook(out: SharedWriter) -> BeforeOp {
    let step = AtomicUsize::new(0);
    Arc::new(move |op, args, _b| {
        let n = step.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = out.write_str(&format!("  [{}] {}\n", n, format_op(op, args)));
        if op.is_block() && !op.body.is_empty() {
            for child in &op.body {
                print_tree(&out, child, "        ");
            }
        }
        OpAction::Skip
    })
}

/// Renders one op + its (possibly nested) body without interpolation. Used by
/// --dry-run to expand block bodies that the interpreter would otherwise skip
/// past.
fn print_tree(out: &SharedWriter, op: &Op, indent: &str) {
    let _ = out.write_str(&format!("{}{}\n", indent, format_op_raw(op)));
    for child in &op.body {
        print_tree(out, child, &format!("{indent}   "));
    }
}

/// `format_op` without the step number prefix, for nested body printing where
/// steps don't apply (the body might never fire).
fn format_op_raw(op: &Op) -> String {
    format_op(op, &op.args)
}

/// A BeforeOp that prints each op and prompts y/n/a/q. Reads lines from
/// `input`, writes prompts to `out`.
pub fn ask_hook(input: Box<dyn Read + Send>, out: SharedWriter) -> BeforeOp {
    let reader = Mutex::new(BufReader::new(input));
    let step = AtomicUsize::new(0);
    Arc::new(move |op, args, _b| {
        let n = step.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = out.write_str(&format!("  [{}] {}\n", n, format_op(op, args)));
        loop {
            let _ = out.write_str("       run? [y/n/a/q] > ");
            let mut line = String::new();
            let read = reader.lock().unwrap_or_else(|e| e.into_inner()).read_line(&mut line);
            // Go's ReadString errors when the delimiter is missing (EOF).
            if !matches!(read, Ok(n) if n > 0) || !line.ends_with('\n') {
                return OpAction::Quit;
            }
            match line.trim().to_lowercase().as_str() {
                "" | "y" | "yes" => return OpAction::Run,
                "n" | "no" | "skip" => {
                    let _ = out.write_str("       (skipped)\n");
                    return OpAction::Skip;
                }
                "a" | "all" => {
                    let _ = out.write_str("       (running all remaining)\n");
                    return OpAction::RunAll;
                }
                "q" | "quit" => return OpAction::Quit,
                _ => {
                    let _ = out.write_str("       y = run, n = skip, a = run all remaining, q = quit\n");
                }
            }
        }
    })
}

/// Renders one op + args in a readable single-line form. Args are sorted for
/// deterministic output; bodies are summarised by length. The positional
/// helpers (_0, _1, …) render as "X" (without the key).
pub fn format_op(op: &Op, args: &Map<String, Value>) -> String {
    let mut keys: Vec<&String> = args.keys().filter(|k| !matches!(k.as_str(), "_body" | "env_prefix")).collect();
    keys.sort();
    let mut parts: Vec<String> = vec![];
    for k in keys {
        let mut v = to_string_value(&args[k]);
        if v.is_empty() {
            continue;
        }
        // Trim very long values to keep the preview readable. Go cuts at byte
        // 77, which can split a multi-byte char; %q then renders the stray
        // bytes as \xNN escapes, so do the same.
        let mut stray = String::new();
        if v.len() > 80 {
            let cut = floor_boundary(&v, 77);
            for b in &v.as_bytes()[cut..77] {
                stray.push_str(&format!("\\x{b:02x}"));
            }
            v = format!("{}…", &v[..cut]);
        }
        let mut q = go_quote(&v);
        if !stray.is_empty() {
            // Insert the escapes just before the trailing "…\"" of the quoted value.
            let tail = "…\"";
            q.truncate(q.len() - tail.len());
            q.push_str(&stray);
            q.push_str(tail);
        }
        if k.starts_with('_') {
            parts.push(q);
        } else {
            parts.push(format!("{}={}", k, q));
        }
    }
    let mut suffix = String::new();
    if !op.capture_into.is_empty() {
        suffix = format!("   → ${{{}}}", op.capture_into);
    }
    let n = op.body.len();
    if n > 0 {
        suffix += &format!("   {{{} body op{}}}", n, plural(n));
    }
    format!("{}{} {}{}", env_prefix_text(args), op.kind, parts.join(" "), suffix)
}

/// R05: an inline env prefix renders shell-style in front of the op
/// (`K="v" exec bin="tool" …`). Values are shown as written (`${REF}`s
/// unresolved), so a preview never prints a host secret.
fn env_prefix_text(args: &Map<String, Value>) -> String {
    let Some(Value::Object(m)) = args.get("env_prefix") else { return String::new() };
    m.iter().map(|(k, v)| format!("{k}={} ", go_quote(&to_string_value(v)))).collect()
}

fn floor_boundary(s: &str, n: usize) -> usize {
    let mut n = n.min(s.len());
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    n
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_interpreter::{Bindings, SharedBuf};
    use serde_json::json;

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn format_op_shapes() {
        let op = Op {
            kind: "shell".into(),
            capture_into: "out".into(),
            body: vec![Op::default(), Op::default()],
            ..Default::default()
        };
        let args = map(json!({"_0": "echo hi", "cwd": "/tmp", "empty": ""}));
        assert_eq!(
            format_op(&op, &args),
            "shell \"echo hi\" cwd=\"/tmp\"   → ${out}   {2 body ops}"
        );
    }

    // R05: the prefix renders shell-style before the op, unresolved.
    #[test]
    fn format_op_shows_env_prefix() {
        let op = Op { kind: "exec".into(), ..Default::default() };
        let args = map(json!({"bin": "kubectl", "_0": "get", "env_prefix": {"KUBECONFIG": "${CFG}", "A": "b c"}}));
        assert_eq!(
            format_op(&op, &args),
            "KUBECONFIG=\"${CFG}\" A=\"b c\" exec \"get\" bin=\"kubectl\""
        );
    }

    #[test]
    fn dry_run_prints_and_skips() {
        let buf = SharedBuf::new();
        let hook = dry_run_hook(buf.writer());
        let child = Op { kind: "print".into(), args: map(json!({"msg": "x"})), ..Default::default() };
        let op = Op { kind: "if".into(), body: vec![child], ..Default::default() };
        let mut b = Bindings::new("/");
        assert_eq!(hook(&op, &Map::new(), &mut b), OpAction::Skip);
        assert_eq!(buf.contents(), "  [1] if    {1 body op}\n        print msg=\"x\"\n");
    }

    #[test]
    fn ask_answers() {
        let buf = SharedBuf::new();
        let input = "\nn\nzzz\na\nq\n";
        let hook = ask_hook(Box::new(std::io::Cursor::new(input.as_bytes().to_vec())), buf.writer());
        let op = Op { kind: "print".into(), ..Default::default() };
        let mut b = Bindings::new("/");
        let none = Map::new();
        assert_eq!(hook(&op, &none, &mut b), OpAction::Run);
        assert_eq!(hook(&op, &none, &mut b), OpAction::Skip);
        assert_eq!(hook(&op, &none, &mut b), OpAction::RunAll); // "zzz" reprompts, then "a"
        assert_eq!(hook(&op, &none, &mut b), OpAction::Quit);
        assert_eq!(hook(&op, &none, &mut b), OpAction::Quit); // EOF
        assert!(buf.contents().contains("y = run, n = skip, a = run all remaining, q = quit"));
    }
}
