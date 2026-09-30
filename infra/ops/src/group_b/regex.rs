//! Regex ops (regex.go).
use crate::group_b::util::*;
use perch_interpreter::{err, Args, Handler, Result};
use serde_json::Value;
use std::collections::HashMap;

pub fn register(m: &mut HashMap<String, Handler>) {
    let mut add = |k: &str, f: fn(&Args<'_>) -> Result<Value>| {
        m.insert(k.to_string(), pure(f));
    };
    add("regex_match", |a| {
        let re = go_regex(&arg_string(a, &["pattern", "_0"])).map_err(err)?;
        Ok(Value::Bool(re.is_match(&arg_string(a, &["value", "_1"]))))
    });
    add("regex_replace", |a| {
        let re = go_regex(&arg_string(a, &["pattern", "_0"])).map_err(err)?;
        let v = arg_string(a, &["value", "_1"]);
        let rep = arg_string(a, &["replacement", "_2"]);
        Ok(Value::String(re.replace_all(&v, rep.as_str()).into_owned()))
    });
    add("regex_find_all", |a| {
        let re = go_regex(&arg_string(a, &["pattern", "_0"])).map_err(err)?;
        let v = arg_string(a, &["value", "_1"]);
        Ok(Value::Array(re.find_iter(&v).map(|m| Value::String(m.as_str().to_string())).collect()))
    });
}
