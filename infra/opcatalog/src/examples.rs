use crate::catalog::Arg;
use crate::examples_data::{OP_EXAMPLES, PER_ARG_SAMPLES};

fn lookup(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

pub(crate) fn example_for_op(kind: &str, args: &[Arg], signature: &str) -> String {
    if let Some(ex) = lookup(OP_EXAMPLES, kind) {
        return ex.to_string();
    }
    if returns_value(signature) {
        return format!("result = {}", call_expr(kind, args, signature));
    }
    call_expr(kind, args, signature)
}

pub(crate) fn example_for_var(name: &str, typ: &str) -> String {
    match typ {
        "bool" => format!("if {name}\n    print \"yes\"\nend"),
        _ => format!("print \"${{{name}}}\""),
    }
}

fn returns_value(signature: &str) -> bool {
    signature.contains('→')
}

fn call_expr(kind: &str, args: &[Arg], signature: &str) -> String {
    if signature.contains("()") || args.is_empty() {
        return kind.to_string();
    }
    let parts: Vec<String> = args.iter().filter(|a| a.ty != "tail").map(|a| arg_sample(kind, a)).collect();
    if parts.is_empty() {
        return kind.to_string();
    }
    format!("{kind} {}", parts.join(" "))
}

fn arg_sample(kind: &str, a: &Arg) -> String {
    if let Some(v) = lookup(PER_ARG_SAMPLES, &format!("{kind}.{}", a.name)) {
        return v.to_string();
    }
    if let Some(v) = lookup(PER_ARG_SAMPLES, &a.name) {
        return v.to_string();
    }
    match a.ty.as_str() {
        "word" => sample_word(&a.name),
        "ident" => sample_ident(&a.name),
        "int" => sample_int(&a.name),
        "tail" => String::new(),
        _ => sample_string(&a.name),
    }
}

fn sample_word(name: &str) -> String {
    if name == "bin" { "git" } else { "arg" }.to_string()
}

fn sample_ident(name: &str) -> String {
    match name {
        "target" | "command" => "deploy",
        "name" | "ident" | "item" | "line" => name,
        _ => "name",
    }
    .to_string()
}

fn sample_int(name: &str) -> String {
    match name {
        "code" => "1",
        "n" | "count" | "max" | "attempts" | "secs" | "seconds" | "delay" => "3",
        _ => "1",
    }
    .to_string()
}

fn sample_string(name: &str) -> String {
    if name.ends_with("_path") || name == "path" {
        return r#""./path""#.into();
    }
    if name.contains("url") {
        return r#""https://example.com""#.into();
    }
    if name.contains("host") {
        return r#""localhost""#.into();
    }
    if name.contains("file") {
        return r#""./file.txt""#.into();
    }
    if name.contains("dir") {
        return r#""${script_dir}""#.into();
    }
    r#""value""#.into()
}

#[cfg(test)]
mod tests {
    use crate::build;

    #[test]
    fn every_op_has_example() {
        let c = build(&perch_ops::builtin_kinds());
        for o in &c.ops {
            assert!(!o.example.trim().is_empty(), "op {:?} missing example", o.name);
        }
    }

    #[test]
    fn every_var_has_example() {
        let c = build(&["print".to_string()]);
        for v in &c.vars {
            assert!(!v.example.trim().is_empty(), "var {:?} missing example", v.name);
        }
    }

    #[test]
    fn example_for_known_ops() {
        let kinds: Vec<String> = ["print", "exec", "write_file", "if", "http_get"].iter().map(|s| s.to_string()).collect();
        let c = build(&kinds);
        let ex = |n: &str| c.ops.iter().find(|o| o.name == n).unwrap().example.clone();
        assert!(ex("print").contains("print \""), "print: {:?}", ex("print"));
        assert!(ex("exec").contains("exec git"));
        assert!(ex("if").contains("if count"));
        assert!(ex("http_get").contains("http_get \"https://"));
    }

    #[test]
    fn example_for_var() {
        let c = build(&["print".to_string()]);
        for v in &c.vars {
            if v.name == "script_dir" {
                assert!(v.example.contains("${script_dir}"), "{:?}", v.example);
            }
            if v.ty == "bool" && v.name == "is_linux" {
                assert!(v.example.starts_with("if is_linux"), "{:?}", v.example);
            }
        }
    }
}
