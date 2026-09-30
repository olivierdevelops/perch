//! Auto-generated reference for everything the perch CLI exposes: top-level
//! flags, subcommands, and key concepts. Two surfaces:
//!
//! ```text
//! perch help            top-level index, grouped, human-readable
//! perch help TOPIC      detail on one flag / concept / command
//! perch help --json     full machine-readable dump (agents, tooling)
//! ```
//!
//! Topic names match the actual CLI tokens (including the leading "--") so a
//! user can copy any flag from a banner or error message and paste it into
//! `perch help`.
//!
//! Op-level help (the ~140 cross-platform built-ins) lives in
//! docs/op-reference.md, linked from here. We don't duplicate it: that would
//! double the maintenance burden every time an op is added.
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{json, Value};
use std::io::Write;

/// The hand-curated catalog, dumped verbatim from the Go `catalog` var (the
/// Go source stays the source of truth until the port reaches parity).
const CATALOG_JSON: &str = include_str!("catalog.json");

/// Go marshals a nil slice as `null`; the catalog only ever has nil-or-non-empty
/// slices, so an empty vec is serialized as `null` to keep JSON byte-identical.
fn null_if_empty<S: Serializer>(v: &[String], s: S) -> Result<S::Ok, S::Error> {
    if v.is_empty() {
        s.serialize_none()
    } else {
        v.serialize(s)
    }
}

fn null_as_empty<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(d)?.unwrap_or_default())
}

/// One entry in the help catalog.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Topic {
    /// "--no-shell" / "--check" / "shebang"
    pub name: String,
    /// "flag" / "subcommand" / "concept"
    pub kind: String,
    /// "Security" / "Authoring" / "Execution" / "Concepts"
    pub group: String,
    /// One-liner.
    pub synopsis: String,
    /// Syntax skeleton — `perch --foo BAR`.
    pub usage: String,
    /// 1-3 paragraphs.
    pub description: String,
    /// Bash snippets.
    #[serde(serialize_with = "null_if_empty", deserialize_with = "null_as_empty")]
    pub examples: Vec<String>,
    /// Canonical doc page.
    pub doc_url: String,
    /// Related topic names.
    #[serde(serialize_with = "null_if_empty", deserialize_with = "null_as_empty")]
    pub see_also: Vec<String>,
}

/// Error returned by [`Impl::execute`]; the message matches the Go error text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpError(pub String);

impl std::fmt::Display for HelpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for HelpError {}

pub struct Impl {
    /// perch version, surfaced in JSON dumps.
    pub version: String,
}

impl Impl {
    /// Writes to `out` / `err` (Go: stdout / stderr).
    pub fn execute(
        &self,
        topic: &str,
        as_json: bool,
        out: &mut dyn Write,
        err: &mut dyn Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cat = catalog();
        if as_json {
            return print_json(out, &self.version, &cat, topic);
        }
        if topic.is_empty() {
            print_index(out, &cat);
            return Ok(());
        }
        let matches = find(&cat, topic);
        match matches.len() {
            0 => {
                let _ = writeln!(err, "no help topic matches {}.", go_quote(topic));
                let _ = writeln!(err, "try: perch help                      (top-level index)");
                let _ = writeln!(err, "     perch help --json               (full catalog as JSON)");
                let _ = writeln!(err, "     perch help <fragment>           (fuzzy search)");
                Err(Box::new(HelpError("no match".into())))
            }
            1 => {
                print_topic(out, &matches[0]);
                Ok(())
            }
            _ => {
                let _ = write!(out, "{} matches for {}:\n\n", matches.len(), go_quote(topic));
                for t in &matches {
                    let _ = writeln!(out, "  {:<22} {}", t.name, t.synopsis);
                }
                let _ = writeln!(out, "\nRefine: `perch help <exact-name>`");
                Ok(())
            }
        }
    }
}

/// Go's `%q` for the strings we print here (double-quoted with escapes).
fn go_quote(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => o.push_str(&format!("\\x{:02x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Returns all topics whose Name contains the query (case-insensitive). Used by
/// both the CLI fuzzy match and the "did you mean…" error hints.
pub fn find(cat: &[Topic], query: &str) -> Vec<Topic> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    // Exact match first.
    if let Some(t) = cat.iter().find(|t| t.name.to_lowercase() == q) {
        return vec![t.clone()];
    }
    // Substring match.
    cat.iter().filter(|t| t.name.to_lowercase().contains(&q)).cloned().collect()
}

// ─── output ──────────────────────────────────────────────────────────

fn print_json(
    w: &mut dyn Write,
    version: &str,
    cat: &[Topic],
    topic: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Go marshals map[string]any with sorted keys.
    let v: Value = if topic.is_empty() {
        json!({
            "doc_root": "https://olivierdevelops.github.io/perch/",
            "perch_version": version,
            "topics": cat,
        })
    } else {
        let matches = find(cat, topic);
        // Find returns a nil slice (JSON null) only for a blank query.
        let m = if topic.trim().is_empty() { Value::Null } else { serde_json::to_value(&matches)? };
        json!({ "matches": m, "query": topic })
    };
    let s = serde_json::to_string_pretty(&v)?;
    writeln!(w, "{s}")?;
    Ok(())
}

fn print_index(w: &mut dyn Write, cat: &[Topic]) {
    let _ = writeln!(w, "perch help — auto-generated reference");
    let _ = writeln!(w);
    let _ = writeln!(w, "Usage:");
    let _ = writeln!(w, "  perch help                 list all topics (this page)");
    let _ = writeln!(w, "  perch help <TOPIC>         detail on one flag / subcommand / concept");
    let _ = writeln!(w, "  perch help --json          dump the full catalog (for agents / tooling)");
    let _ = writeln!(w, "  perch help --json <TOPIC>  one topic as JSON");
    let _ = writeln!(w);

    let mut order: Vec<&str> = Vec::new();
    for t in cat {
        if !order.contains(&t.group.as_str()) {
            order.push(&t.group);
        }
    }
    for g in order {
        let _ = writeln!(w, "{g}");
        for t in cat.iter().filter(|t| t.group == g) {
            let _ = writeln!(w, "  {:<26} {}", t.name, t.synopsis);
        }
        let _ = writeln!(w);
    }
    let _ = writeln!(w, "Op reference (~140 built-in ops):");
    let _ = writeln!(w, "  https://olivierdevelops.github.io/perch/op-reference/");
    let _ = writeln!(w);
}

fn print_topic(w: &mut dyn Write, t: &Topic) {
    let _ = writeln!(w, "{} — {}", t.name, t.synopsis);
    let _ = writeln!(w, "{}", "─".repeat(60));
    if !t.usage.is_empty() {
        let _ = write!(w, "\nUSAGE\n  {}\n", t.usage);
    }
    if !t.description.is_empty() {
        let _ = writeln!(w, "\nDESCRIPTION");
        for line in t.description.trim().split('\n') {
            let _ = writeln!(w, "  {line}");
        }
    }
    if !t.examples.is_empty() {
        let _ = writeln!(w, "\nEXAMPLES");
        for ex in &t.examples {
            let _ = writeln!(w, "  $ {ex}");
        }
    }
    if !t.see_also.is_empty() {
        let _ = write!(w, "\nSEE ALSO\n  {}\n", t.see_also.join(", "));
    }
    if !t.doc_url.is_empty() {
        let _ = write!(w, "\nMORE\n  {}\n", t.doc_url);
    }
    let _ = writeln!(w);
}

// ─── catalog ─────────────────────────────────────────────────────────

/// Returns the full list of topics, sorted within each group.
pub fn catalog() -> Vec<Topic> {
    let mut cat: Vec<Topic> =
        serde_json::from_str(CATALOG_JSON).expect("help catalog.json is valid");
    cat.sort_by(|a, b| {
        group_order(&a.group)
            .cmp(&group_order(&b.group))
            .then_with(|| a.group.cmp(&b.group).then(a.name.cmp(&b.name)))
    });
    cat
}

fn group_order(g: &str) -> usize {
    ["Execution", "Authoring", "Security", "Scripts", "Build", "Agents", "Concepts"]
        .iter()
        .position(|n| *n == g)
        .unwrap_or(99)
}

/// Returns a one-line "→ run `perch help X` for details" hint that error sites
/// can append to their messages. Exported so the interpreter and use cases can
/// call it without re-importing the catalog.
pub fn help_hint(topic: &str) -> String {
    format!("→ run `perch help {topic}` for details")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(topic: &str, json: bool) -> (Result<(), String>, String, String) {
        let (mut o, mut e) = (Vec::new(), Vec::new());
        let r = Impl { version: "v9".into() }
            .execute(topic, json, &mut o, &mut e)
            .map_err(|e| e.to_string());
        (r, String::from_utf8(o).unwrap(), String::from_utf8(e).unwrap())
    }

    #[test]
    fn catalog_loads_sorted_by_group() {
        let cat = catalog();
        assert_eq!(cat.len(), 43);
        assert_eq!(cat[0].group, "Execution");
        assert!(cat.windows(2).all(|w| group_order(&w[0].group) <= group_order(&w[1].group)));
    }

    #[test]
    fn find_exact_then_substring() {
        let cat = catalog();
        assert_eq!(find(&cat, "  --INIT ").len(), 1);
        assert!(find(&cat, "--").len() > 1);
        assert!(find(&cat, "  ").is_empty());
    }

    #[test]
    fn no_match_errors() {
        let (r, _, e) = run("zzzz", false);
        assert_eq!(r.unwrap_err(), "no match");
        assert!(e.starts_with("no help topic matches \"zzzz\".\n"));
    }

    #[test]
    fn json_shapes() {
        let (_, o, _) = run("", true);
        assert!(o.starts_with("{\n  \"doc_root\": \"https://olivierdevelops.github.io/perch/\",\n  \"perch_version\": \"v9\",\n  \"topics\": [\n"));
        assert!(o.contains("\"examples\": null"));
        let (_, o, _) = run("--init", true);
        assert!(o.starts_with("{\n  \"matches\": [\n"));
        assert!(o.ends_with("  \"query\": \"--init\"\n}\n"));
    }

    #[test]
    fn hint() {
        assert_eq!(help_hint("x"), "→ run `perch help x` for details");
    }
}
