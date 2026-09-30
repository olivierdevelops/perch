use std::collections::BTreeSet;
use std::sync::OnceLock;

use perch_domain::{ErrorKind, Op, OpError, Program, Requirements};
use regex::Regex;

use crate::loader::{go_quote, library_source, op_kinds_source};

/// The load-time gate for the zero-ambient-authority model
/// (docs/sandboxed-by-design.md). It runs once on the fully-merged root
/// program (after imports union their manifests in) and enforces:
///
///  1. A file with no `requires` block is treated as an empty manifest
///     (`requires`/`end` with no entries): declared==true, pure ops only.
///     An explicit empty block and a missing block are equivalent.
///
///  2. Every subprocess bin is declared. Both the explicit `exec BIN …` op
///     and the implicit bare-invocation form (`docker ps`, folded by the
///     grammar into an exec op) resolve their bin against the declared
///     `bin "…"` set. An undeclared bin is bin_not_declared at LOAD time —
///     promoted from the old runtime-only gate so a typo or a forgotten
///     declaration fails before anything runs. The error carries a
///     did-you-mean hint against the op vocabulary (catching `dcoker`/`prnit`
///     style slips) and the declared bins.
///
/// `exec_chain` (a && b) and `pipe` stages carry their child execs in `body`,
/// so the recursive walk reaches them. Pure ops, control flow, and
/// template/catch bodies are all walked.
pub(crate) fn enforce_zero_ambient(prog: &mut Program) -> Result<(), OpError> {
    normalize_requirements(prog);

    let req = &prog.requirements;

    fn walk(ops: &[Op], where_: &str, req: &Requirements) -> Result<(), OpError> {
        for op in ops {
            if op.kind == "exec" {
                let bin = op.args.get("bin").and_then(|v| v.as_str()).unwrap_or("");
                // Skip interpolated bins (`exec ${tool} …`, `exec ${HOME}/bin/x`):
                // their value isn't known until runtime, so the static gate can't
                // resolve them. They defer to the runtime guard, consistent with
                // how requires treats interpolated hosts/paths. Only LITERAL bins
                // are gated at load. A declared alias or path satisfies the gate.
                if !bin.is_empty() && !bin.contains("${") && !req.bin_allowed(bin) {
                    return Err(undeclared_bin_error(bin, op, where_, req));
                }
            }
            if !op.body.is_empty() {
                walk(&op.body, where_, req)?;
            }
        }
        Ok(())
    }

    for (name, cmd) in &prog.commands {
        walk(&cmd.ops, &format!("command {name}"), req)?;
    }
    if let Some(catch) = &prog.catch {
        walk(&catch.ops, "catch", req)?;
    }
    for (name, tpl) in &prog.templates {
        walk(&tpl.ops, &format!("template {name}"), req)?;
    }
    Ok(())
}

/// Treats a missing `requires` block as an empty manifest. After this,
/// `declared` is always true for programs that pass through `load`, and
/// runtime/path/bin gates apply even when the author omitted the block.
pub(crate) fn normalize_requirements(prog: &mut Program) {
    if prog.requirements.declared {
        return;
    }
    prog.requirements.declared = true;
}

/// Builds the bin_not_declared load error with a did-you-mean hint. If the bin
/// name is close to a known op keyword, the user probably typo'd an op
/// (`prnit` → `print`); otherwise they likely forgot to declare a real binary.
/// Both cases suggest the fix.
fn undeclared_bin_error(bin: &str, op: &Op, where_: &str, req: &Requirements) -> OpError {
    let mut hint = String::new();
    let sug = closest(bin, op_vocabulary());
    if !sug.is_empty() {
        hint = format!(" — did you mean the op `{sug}`?");
    } else {
        let mut declared_names: Vec<String> = Vec::with_capacity(req.bins.len() * 2);
        for b in &req.bins {
            declared_names.push(b.name.clone());
            if !b.alias.is_empty() {
                declared_names.push(b.alias.clone());
            }
        }
        let sug = closest(bin, &declared_names);
        if !sug.is_empty() {
            hint = format!(" — did you mean declared bin `{sug}`?");
        }
    }
    OpError::new(
        "exec",
        ErrorKind::BinNotDeclared,
        &format!(
            "{} line {}: `{}` is not a known op and not declared in `requires` \
             (add `bin {}` to the requires block to run it){}",
            where_,
            op.line,
            bin,
            go_quote(bin),
            hint
        ),
    )
}

// ── op vocabulary, extracted from the embedded grammar ──────────────────────

/// Matches a grammar function's FIRST `arg literal "X"` line. We over-collect
/// (every leading literal across the file) which is fine: the set is only used
/// for typo suggestions, never for correctness.
fn first_literal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?m)^\s*arg literal "([a-zA-Z_][a-zA-Z0-9_]*)""#).unwrap())
}

/// The set of grammar keywords (op + config names), parsed once from the
/// embedded `lib.capy` source, sorted. Self-maintaining: adding an op to the
/// grammar automatically extends the suggestion vocabulary.
pub(crate) fn op_vocabulary() -> &'static Vec<String> {
    static VOCAB: OnceLock<Vec<String>> = OnceLock::new();
    VOCAB.get_or_init(|| {
        let set: BTreeSet<String> = first_literal_re()
            .captures_iter(library_source())
            .map(|m| m[1].to_string())
            .collect();
        set.into_iter().collect()
    })
}

/// The canonical built-in op-kind membership set, parsed once from the
/// embedded `opkinds.txt` (generated from `ops.BuiltinKinds()`).
/// `resolve_bare_dispatch` uses it to tell a bare built-in op (`glob`,
/// `sha256`, `file_size`) from a declared subprocess bin when folding
/// implicit-exec captures. Unlike the grammar-derived [`op_vocabulary`], this
/// includes the value-returning ops that have no dedicated grammar keyword.
pub(crate) fn op_set() -> &'static BTreeSet<String> {
    static SET: OnceLock<BTreeSet<String>> = OnceLock::new();
    SET.get_or_init(|| {
        op_kinds_source()
            .split('\n')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_string)
            .collect()
    })
}

/// Returns the nearest candidate within Levenshtein distance 2, or "" if none
/// qualifies. Mirrors the fuzzy command-suggestion threshold.
fn closest<S: AsRef<str>>(s: &str, candidates: &[S]) -> String {
    let mut best = String::new();
    let mut best_d = 3;
    for c in candidates {
        let d = levenshtein(s, c.as_ref());
        if d < best_d {
            best_d = d;
            best = c.as_ref().to_string();
        }
    }
    best
}

fn levenshtein(a: &str, b: &str) -> usize {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (la, lb) = (a.len(), b.len());
    if la == 0 {
        return lb;
    }
    if lb == 0 {
        return la;
    }
    let mut prev: Vec<usize> = (0..=lb).collect();
    let mut cur = vec![0usize; lb + 1];
    for i in 1..=la {
        cur[0] = i;
        for j in 1..=lb {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[lb]
}
