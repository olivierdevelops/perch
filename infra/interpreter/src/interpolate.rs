use crate::bindings::Bindings;
use crate::cliargs::go_quote;
use crate::interpreter::{err, Result};
use serde_json::{Map, Value};

/// Substitutes `${name}` placeholders in `s` with values from `b`. Unknown
/// names produce an error. Use `\${name}` (literal backslash) to emit a literal
/// `${name}` without substitution — useful for shell variables that should
/// reach bash untouched.
pub fn interpolate(s: &str, b: &Bindings) -> Result<String> {
    if !s.contains("${") {
        return Ok(s.to_string());
    }
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        // `\${` → emit literal `${`, skip both bytes of the escape and the `{`.
        if i + 2 < bytes.len() && bytes[i] == b'\\' && bytes[i + 1] == b'$' && bytes[i + 2] == b'{' {
            out.extend_from_slice(b"${");
            i += 3;
            continue;
        }
        if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
            let end = match bytes[i + 2..].iter().position(|&c| c == b'}') {
                Some(e) => e,
                None => return Err(err(format!("unterminated ${{ in {}", go_quote(s)))),
            };
            let name = s[i + 2..i + 2 + end].trim();
            match b.lookup(name) {
                Some(v) => out.extend_from_slice(v.as_bytes()),
                None => {
                    // When the env allowlist is active and the name LOOKS like
                    // a host env var (uppercase / underscores / digits), say so
                    // — that's almost always why it didn't resolve.
                    if b.env_restricted() && looks_like_env_name(name) {
                        return Err(err(format!(
                            "env var ${{{name}}} is not in --env allowlist (declare with --env {name} — run `perch help --env` for details)"
                        )));
                    }
                    return Err(err(format!(
                        "unknown placeholder ${{{}}} in {} — run `perch help interpolation` for resolution order",
                        name,
                        go_quote(s)
                    )));
                }
            }
            i += 2 + end + 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// A fresh map with every string value interpolated against `b`. Non-string
/// values are passed through.
pub fn interpolate_args(args: &Map<String, Value>, b: &Bindings) -> Result<Map<String, Value>> {
    if args.is_empty() {
        return Ok(args.clone());
    }
    let mut out = Map::new();
    for (k, v) in args {
        if let Value::String(s) = v {
            out.insert(k.clone(), Value::String(interpolate(s, b)?));
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    // Universal bare-ident resolution: keys of the form `_NAME_var` carry a
    // binding NAME captured at parse time (via ident-form grammar overloads
    // like `http_get url`). Resolve via `lookup` and write the value to the
    // canonical `NAME` key so the handler reads it via the usual
    // `arg_string(args, "NAME", "_0")` call without modification. This is what
    // makes `http_get url` / `print msg` / `shell cmd` etc. work with zero
    // handler-side changes — adding one grammar overload per op surfaces the
    // bare-ident form universally.
    let pending: Vec<(String, String)> = out
        .iter()
        .filter(|(k, _)| k.starts_with('_') && k.ends_with("_var") && k.len() > 5)
        .filter_map(|(k, v)| v.as_str().map(|n| (k.clone(), n.to_string())))
        .collect();
    for (k, name) in pending {
        // Strip leading `_` and trailing `_var` to derive the canonical key.
        // For named args this is `_msg_var → msg`. For positional args the
        // convention is keys of the form `_0`, `_1` (leading underscore is
        // significant), so we must KEEP it when the inner token starts with a
        // digit: `_0_var → _0`.
        let mut canonical = k[1..k.len() - 4].to_string();
        if canonical.as_bytes().first().is_some_and(|c| c.is_ascii_digit()) {
            canonical = format!("_{canonical}");
        }
        match b.lookup(&name) {
            Some(got) => out.insert(canonical, Value::String(got)),
            // Not a binding — fall back to the literal token text, like a CLI:
            // `git rev-parse HEAD` / `replace text foo bar` pass HEAD / foo /
            // bar through verbatim. A defined binding of the same name
            // resolves (above); an undefined bare word is just its own value.
            None => out.insert(canonical, Value::String(name)),
        };
    }
    Ok(out)
}

/// `${PATH}` / `${HOME}` / `${API_KEY_2}` look like env vars (all uppercase,
/// possibly with underscores and digits). Anything with a lowercase letter is
/// almost certainly a binding name.
fn looks_like_env_name(name: &str) -> bool {
    !name.is_empty() && !name.chars().any(|r| r.is_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::to_string_value;
    use serde_json::json;

    #[test]
    fn passthrough() {
        let b = Bindings::new("/tmp");
        assert_eq!(interpolate("hello world", &b).unwrap(), "hello world");
    }

    #[test]
    fn subst() {
        let mut b = Bindings::new("/tmp");
        b.set("name", "Alice");
        b.set("n", 7);
        assert_eq!(interpolate("Hello ${name} #${n}", &b).unwrap(), "Hello Alice #7");
    }

    #[test]
    fn env_fallback() {
        let b = Bindings::new("/tmp");
        std::env::set_var("PERCH_TEST_XYZ", "FROM_ENV");
        assert_eq!(interpolate("v=${PERCH_TEST_XYZ}", &b).unwrap(), "v=FROM_ENV");
    }

    #[test]
    fn unknown() {
        let b = Bindings::new("/tmp");
        assert!(interpolate("hi ${nope}", &b).is_err());
    }

    #[test]
    fn unterminated() {
        let b = Bindings::new("/tmp");
        assert!(interpolate("hi ${nope", &b).is_err());
    }

    #[test]
    fn escape() {
        // `\${x}` becomes a literal `${x}`. Lets shell vars survive substitution.
        let b = Bindings::new("/tmp");
        assert_eq!(interpolate(r"echo \${SHELL_VAR}", &b).unwrap(), "echo ${SHELL_VAR}");
    }

    #[test]
    fn bare_dollar() {
        // A bare `$` without `{` is left alone (no shell-style $name expansion).
        let b = Bindings::new("/tmp");
        assert_eq!(interpolate("PATH=$PATH", &b).unwrap(), "PATH=$PATH");
    }

    #[test]
    fn to_string_values() {
        let cases = [
            (json!("hi"), "hi"),
            (json!(true), "true"),
            (json!(false), "false"),
            (json!(42), "42"),
            (json!(99i64), "99"),
            (json!(2.5), "2.5"),
            (json!(7.0), "7"),
            (Value::Null, ""),
        ];
        for (v, want) in cases {
            assert_eq!(to_string_value(&v), want, "{v}");
        }
    }

    #[test]
    fn bare_ident_var_keys() {
        let mut b = Bindings::new("/tmp");
        b.set("msg", "hello");
        let mut args = Map::new();
        args.insert("_msg_var".into(), json!("msg"));
        args.insert("_0_var".into(), json!("undefined_word"));
        let out = interpolate_args(&args, &b).unwrap();
        assert_eq!(out["msg"], json!("hello"));
        assert_eq!(out["_0"], json!("undefined_word"));
    }
}
