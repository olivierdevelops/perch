use crate::docs::doc_for;
use crate::examples::{example_for_op, example_for_var};
use crate::parse_capy::parse_capy_args;
use crate::requirements::{category_for, requirements_for, Requirements};
use serde::Serialize;

/// One grammar argument for an op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Arg {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub description: String,
}

/// One built-in perch op with docs and capability requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Op {
    pub name: String,
    pub description: String,
    pub example: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub signature: String,
    pub category: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<Arg>,
    pub requirements: Requirements,
}

/// The full export document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Catalog {
    pub schema: String,
    pub version: String,
    pub vars: Vec<Var>,
    pub ops: Vec<Op>,
}

/// An auto-bound `${name}` available in every command body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Var {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub category: String,
    pub description: String,
    pub example: String,
}

/// Assembles the catalog for the given handler kinds.
pub fn build(kinds: &[String]) -> Catalog {
    let mut grammar = parse_capy_args();
    let mut ops: Vec<Op> = kinds
        .iter()
        .map(|k| {
            let (sig, desc) = doc_for(k);
            let args = grammar.get_mut(k).map(|v| v.clone()).unwrap_or_default();
            Op {
                name: k.clone(),
                description: desc.to_string(),
                example: example_for_op(k, &args, sig),
                signature: sig.to_string(),
                category: category_for(k).to_string(),
                args,
                requirements: requirements_for(k),
            }
        })
        .collect();
    ops.sort_by(|a, b| a.name.cmp(&b.name));
    let mut vars = provided_vars();
    vars.sort_by(|a, b| a.name.cmp(&b.name));
    Catalog { schema: "perch.catalog.v1".into(), version: "1".into(), vars, ops }
}

fn provided_vars() -> Vec<Var> {
    perch_interpreter::provided_vars()
        .into_iter()
        .map(|v| Var {
            name: v.name.to_string(),
            ty: v.ty.to_string(),
            category: v.category.to_string(),
            description: v.description.to_string(),
            example: example_for_var(v.name, v.ty),
        })
        .collect()
}

/// Returns indented JSON for the given op kinds, byte-identical to Go's
/// `json.MarshalIndent(c, "", "  ")` (including HTML-safe escaping of
/// `<`, `>`, `&`, U+2028 and U+2029).
pub fn marshal_json(kinds: &[String]) -> Result<Vec<u8>, serde_json::Error> {
    let s = serde_json::to_string_pretty(&build(kinds))?;
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_capy_args;

    fn k(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn build_includes_known_ops() {
        let c = build(&k(&["print", "exec", "http_get", "mkdir"]));
        assert_eq!(c.ops.len(), 4);
        let by = |n: &str| c.ops.iter().find(|o| o.name == n).unwrap().clone();
        assert!(by("print").requirements.pure, "print should be pure");
        assert!(by("exec").requirements.bin, "exec should require bin");
        assert!(by("http_get").requirements.host, "http_get should require host");
        assert!(by("mkdir").requirements.write, "mkdir should require write");
    }

    #[test]
    fn parse_capy_args_print() {
        let args = parse_capy_args().remove("print").unwrap_or_default();
        assert!(!args.is_empty(), "print should have args from grammar");
        assert!(args.iter().any(|a| a.name == "msg" && a.ty == "string"), "print args: {args:?}");
    }

    #[test]
    fn marshal_json_valid() {
        let data = String::from_utf8(marshal_json(&k(&["print", "shell"])).unwrap()).unwrap();
        assert!(data.contains(r#""schema": "perch.catalog.v1""#), "missing schema: {data}");
        assert!(data.contains(r#""vars""#));
        assert!(data.contains(r#""name": "script_dir""#));
        assert!(data.contains(r#""example""#));
    }

    #[test]
    fn json_html_escaped() {
        let data = String::from_utf8(marshal_json(&k(&["exec_chain"])).unwrap()).unwrap();
        assert!(data.contains("\\u0026\\u0026"), "{data}");
    }
}
