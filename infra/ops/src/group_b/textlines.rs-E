//! Line-oriented pure text ops (textlines.go): grep / reject / cut / head /
//! tail / sort_lines / uniq_lines / count_lines.
use crate::group_b::util::*;
use perch_interpreter::{err, Args, Handler, Result};
use serde_json::Value;
use std::collections::HashMap;

pub fn register(m: &mut HashMap<String, Handler>) {
    m.insert("grep".into(), pure(op_grep));
    m.insert("reject".into(), pure(op_reject));
    m.insert("cut".into(), pure(op_cut));
    m.insert("head".into(), pure(op_head));
    m.insert("tail".into(), pure(op_tail));
    m.insert("sort_lines".into(), pure(op_sort_lines));
    m.insert("uniq_lines".into(), pure(op_uniq_lines));
    m.insert("count_lines".into(), pure(op_count_lines));
}

/// Splits on `\n` and drops trailing newlines (a value ending in "\n" yields N
/// lines, not N+1). Empty input yields no lines.
fn split_lines(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    let t = s.trim_end_matches('\n');
    if t.is_empty() {
        return vec![String::new()];
    }
    t.split('\n').map(str::to_string).collect()
}

fn filter_lines(a: &Args<'_>, keep_matching: bool) -> Result<Value> {
    let pat = arg_string(a, &["_0", "pattern"]);
    let text = arg_string(a, &["_1", "text"]);
    let re = go_regex(&pat).map_err(err)?;
    let keep: Vec<String> = split_lines(&text).into_iter().filter(|l| re.is_match(l) == keep_matching).collect();
    Ok(Value::String(keep.join("\n")))
}

/// Keeps lines matching the regex. `grep PAT TEXT`.
pub fn op_grep(a: &Args<'_>) -> Result<Value> {
    filter_lines(a, true)
}

/// grep's inverse. `reject PAT TEXT`.
pub fn op_reject(a: &Args<'_>) -> Result<Value> {
    filter_lines(a, false)
}

/// Nth whitespace-delimited field (1-indexed) of each line. `cut N TEXT`.
pub fn op_cut(a: &Args<'_>) -> Result<Value> {
    let mut n = f2i(to_float(a.map.get("_0")));
    let text = arg_string(a, &["_1", "text"]);
    if n < 1 {
        n = 1;
    }
    let out: Vec<String> = split_lines(&text)
        .iter()
        .map(|l| l.split_whitespace().nth((n - 1) as usize).unwrap_or("").to_string())
        .collect();
    Ok(Value::String(out.join("\n")))
}

fn clamp_n(a: &Args<'_>, len: usize) -> usize {
    let n = f2i(to_float(a.map.get("_0"))).max(0);
    (n as usize).min(len)
}

/// First N lines. `head N TEXT`.
pub fn op_head(a: &Args<'_>) -> Result<Value> {
    let lines = split_lines(&arg_string(a, &["_1", "text"]));
    let n = clamp_n(a, lines.len());
    Ok(Value::String(lines[..n].join("\n")))
}

/// Last N lines. `tail N TEXT`.
pub fn op_tail(a: &Args<'_>) -> Result<Value> {
    let lines = split_lines(&arg_string(a, &["_1", "text"]));
    let n = clamp_n(a, lines.len());
    Ok(Value::String(lines[lines.len() - n..].join("\n")))
}

/// Sorts lines lexicographically. `sort_lines TEXT`.
pub fn op_sort_lines(a: &Args<'_>) -> Result<Value> {
    let mut lines = split_lines(&arg_string(a, &["_0", "text"]));
    lines.sort();
    Ok(Value::String(lines.join("\n")))
}

/// Collapses ADJACENT duplicate lines. `uniq_lines TEXT`.
pub fn op_uniq_lines(a: &Args<'_>) -> Result<Value> {
    let lines = split_lines(&arg_string(a, &["_0", "text"]));
    let mut out: Vec<&String> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if i == 0 || *l != lines[i - 1] {
            out.push(l);
        }
    }
    Ok(Value::String(out.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n")))
}

/// Number of lines as a string-encoded int. `count_lines TEXT`.
pub fn op_count_lines(a: &Args<'_>) -> Result<Value> {
    Ok(Value::String(split_lines(&arg_string(a, &["_0", "text"])).len().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    const SAMPLE: &str = "apple\nbanana\napple\ncherry\n";

    fn args(v: Value) -> Args<'static> {
        let map: Map<String, Value> = v.as_object().unwrap().clone();
        Args { map, body: &[] }
    }

    #[test]
    fn grep_reject() {
        let got = op_grep(&args(json!({"_0": "a", "_1": SAMPLE}))).unwrap();
        assert_eq!(got, json!("apple\nbanana\napple"), "grep a");
        let got2 = op_reject(&args(json!({"_0": "a", "_1": SAMPLE}))).unwrap();
        assert_eq!(got2, json!("cherry"), "reject a");
    }

    #[test]
    fn head_tail() {
        let h = op_head(&args(json!({"_0": 2, "_1": SAMPLE}))).unwrap();
        assert_eq!(h, json!("apple\nbanana"), "head 2");
        let t = op_tail(&args(json!({"_0": 2, "_1": SAMPLE}))).unwrap();
        assert_eq!(t, json!("apple\ncherry"), "tail 2");
    }

    #[test]
    fn sort_uniq_count() {
        let s = op_sort_lines(&args(json!({"_0": SAMPLE}))).unwrap();
        assert_eq!(s, json!("apple\napple\nbanana\ncherry"), "sort");
        let u = op_uniq_lines(&args(json!({"_0": s}))).unwrap();
        assert_eq!(u, json!("apple\nbanana\ncherry"), "uniq after sort");
        let n = op_count_lines(&args(json!({"_0": SAMPLE}))).unwrap();
        assert_eq!(n, json!("4"), "count");
    }

    #[test]
    fn cut() {
        let text = "alice 30 nyc\nbob 25 sf\n";
        let c = op_cut(&args(json!({"_0": 2, "_1": text}))).unwrap();
        assert_eq!(c, json!("30\n25"), "cut 2");
    }
}
