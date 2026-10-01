//! String ops (strings.go).
use crate::group_b::gofmt::sprintf;
use crate::group_b::util::*;
use perch_interpreter::{err, to_string_value, Args, Handler, Result};
use serde_json::Value;
use std::collections::HashMap;

pub fn register(m: &mut HashMap<String, Handler>) {
    let mut add = |k: &str, f: fn(&Args<'_>) -> Result<Value>| {
        m.insert(k.to_string(), pure(f));
    };
    add("trim", |a| Ok(Value::String(arg_string(a, &["value", "_0"]).trim().to_string())));
    add("lower", |a| Ok(Value::String(arg_string(a, &["value", "_0"]).chars().map(simple_lower).collect())));
    add("upper", |a| Ok(Value::String(arg_string(a, &["value", "_0"]).chars().map(simple_upper).collect())));
    add("capitalize", |a| {
        let s = arg_string(a, &["value", "_0"]);
        let mut cs = s.chars();
        Ok(Value::String(match cs.next() {
            None => s,
            Some(c) => simple_upper(c).to_string() + cs.as_str(),
        }))
    });
    add("length", |a| Ok(Value::from(arg_string(a, &["value", "_0"]).len() as i64)));
    add("replace", |a| {
        // let x = replace SUBJECT "OLD,NEW"
        let s = arg_string(a, &["value", "_0"]);
        let spec = arg_string(a, &["pattern", "_1"]);
        Ok(Value::String(match spec.find(',') {
            Some(idx) => s.replace(&spec[..idx], &spec[idx + 1..]),
            None => s,
        }))
    });
    add("split", |a| {
        let s = arg_string(a, &["value", "_0"]);
        let sep = arg_string(a, &["sep", "_1"]);
        Ok(Value::Array(go_split(&s, &sep).into_iter().map(Value::String).collect()))
    });
    add("join", |a| {
        let sep = arg_string(a, &["sep", "_1"]);
        Ok(Value::String(match a.map.get("_0") {
            Some(Value::Array(x)) => x.iter().map(to_string_value).collect::<Vec<_>>().join(&sep),
            Some(Value::String(s)) => s.clone(),
            _ => String::new(),
        }))
    });
    add("contains", |a| Ok(Value::Bool(arg_string(a, &["value", "_0"]).contains(&arg_string(a, &["sub", "_1"])))));
    add("has_prefix", |a| {
        Ok(Value::Bool(arg_string(a, &["value", "_0"]).starts_with(&arg_string(a, &["prefix", "_1"]))))
    });
    add("has_suffix", |a| {
        Ok(Value::Bool(arg_string(a, &["value", "_0"]).ends_with(&arg_string(a, &["suffix", "_1"]))))
    });
    add("repeat", |a| {
        let s = arg_string(a, &["value", "_0"]);
        let n = f2i(to_float(a.map.get("count")));
        if n < 0 {
            return Err(err("repeat: negative count"));
        }
        Ok(Value::String(s.repeat(n as usize)))
    });
    add("format", |a| {
        // `let s = format "Hello %s" "world"`
        let f = arg_string(a, &["format", "_0"]);
        Ok(Value::String(sprintf(&f, a.map.get("_1"))))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn call(name: &str, args: Value) -> Value {
        let mut m = HashMap::new();
        register(&mut m);
        let map: Map<String, Value> = args.as_object().unwrap().clone();
        let a = Args { map, body: &[] };
        let i = perch_interpreter::Interpreter::new(HashMap::new(), perch_domain::Program::default());
        let mut b = perch_interpreter::Bindings::new("/");
        m[name](&i, &mut b, &a).unwrap()
    }

    #[test]
    fn basics() {
        assert_eq!(call("capitalize", json!({"_0": "élan"})), json!("Élan"));
        assert_eq!(call("split", json!({"_0": "a,b", "_1": ","})), json!(["a", "b"]));
        assert_eq!(call("split", json!({"_0": "ab", "_1": ""})), json!(["a", "b"]));
        assert_eq!(call("replace", json!({"_0": "a-b-c", "_1": "-,+"})), json!("a+b+c"));
        assert_eq!(call("length", json!({"_0": "héllo"})), json!(6));
        assert_eq!(call("join", json!({"_0": ["a", 1], "_1": "-"})), json!("a-1"));
        assert_eq!(call("repeat", json!({"_0": "ab", "count": 2})), json!("abab"));
    }
}
