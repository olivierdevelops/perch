use crate::argdesc::arg_description;
use crate::catalog::Arg;
use regex::Regex;
use std::collections::{BTreeMap, HashSet};
use std::sync::OnceLock;

struct Res {
    func_header: Regex,
    kind: Regex,
    capture: Regex,
    func_name: Regex,
    first_literal: Regex,
}

fn res() -> &'static Res {
    static R: OnceLock<Res> = OnceLock::new();
    R.get_or_init(|| Res {
        func_header: Regex::new(r"(?m)^function[\t\n\f\r ]+[A-Za-z0-9_]+").unwrap(),
        kind: Regex::new(r#""kind":"([^"]+)""#).unwrap(),
        capture: Regex::new(r"(?m)^[\t\n\f\r ]*arg capture ([A-Za-z0-9_]+) ([A-Za-z0-9_]+)").unwrap(),
        func_name: Regex::new(r"(?m)^function[\t\n\f\r ]+([A-Za-z0-9_]+)").unwrap(),
        first_literal: Regex::new(r#"(?m)^[\t\n\f\r ]*arg literal "([^"]+)""#).unwrap(),
    })
}

/// Extracts grammar arg captures grouped by emitted op kind. Multiple grammar
/// functions for one kind (e.g. exec / exec_args) are merged.
pub fn parse_capy_args() -> BTreeMap<String, Vec<Arg>> {
    let re = res();
    let mut out: BTreeMap<String, Vec<Arg>> = BTreeMap::new();
    let src = perch_capyloader::library_source();
    let headers: Vec<(usize, usize)> = re.func_header.find_iter(src).map(|m| (m.start(), m.end())).collect();
    for (i, h) in headers.iter().enumerate() {
        let end = headers.get(i + 1).map(|n| n.0).unwrap_or(src.len());
        let block = &src[h.0..end];
        let Some(km) = re.kind.captures(block) else { continue };
        let kind = km[1].to_string();
        if kind.starts_with('_') {
            continue;
        }
        if let Some(fnm) = re.func_name.captures(block) {
            if fnm[1].ends_with("_ident") {
                continue;
            }
        }
        // Skip `let NAME = op ...` grammar variants: they share the op kind but
        // carry capture-specific args (name) that aren't part of the statement form.
        if let Some(lit) = re.first_literal.captures(block) {
            if &lit[1] == "let" {
                continue;
            }
        }
        let mut seen: HashSet<String> = out.get(&kind).map(|v| v.iter().map(|a| a.name.clone()).collect()).unwrap_or_default();
        for cap in re.capture.captures_iter(block) {
            let name = cap[1].to_string();
            let mut typ = cap[2].to_string();
            if !seen.insert(name.clone()) {
                continue;
            }
            // `word` is the capture used for quote-optional string values; to
            // authors it's just a string argument.
            if typ == "word" {
                typ = "string".into();
            }
            let description = arg_description(&kind, &name, &typ);
            out.entry(kind.clone()).or_default().push(Arg { name, ty: typ, description });
        }
    }
    out
}
