//! Prints rich, per-command help (description, args table, defaults, env,
//! examples). Triggered by `perch <cmd> --help`.
use perch_domain::{ArgSpec, Command, Program};
use serde_json::Value;
use std::io::Write;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type LoadFn = Box<dyn Fn(&str) -> Result<Program, Error>>;

pub struct Impl {
    pub load: LoadFn,
}

impl Impl {
    /// Renders help for `command_name` into `out` (Go: `Out`, nil → stdout).
    pub fn execute(&self, config_path: &str, command_name: &str, out: &mut dyn Write) -> Result<(), Error> {
        let p = (self.load)(config_path)?;
        match p.commands.get(command_name) {
            None => {
                // Treat as unknown — surface suggestions.
                let suggestions = suggest(command_name, &visible(&p));
                let _ = writeln!(out, "Unknown command: {}", go_quote(command_name));
                if !suggestions.is_empty() {
                    let _ = writeln!(out, "Did you mean: {}?", suggestions.join(", "));
                }
                Err("command not found".into())
            }
            Some(cmd) => {
                render(out, &p, cmd, config_path);
                Ok(())
            }
        }
    }
}

/// Go's `%q` for the strings we print here.
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

/// Go's `%v` for a decoded JSON value.
fn go_v(v: &Value) -> String {
    match v {
        Value::Null => "<nil>".into(),
        Value::Bool(b) => b.to_string(),
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                go_float(n.as_f64().unwrap_or(0.0))
            }
        }
        Value::Array(a) => format!("[{}]", a.iter().map(go_v).collect::<Vec<_>>().join(" ")),
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            format!(
                "map[{}]",
                keys.iter().map(|k| format!("{}:{}", k, go_v(&m[*k]))).collect::<Vec<_>>().join(" ")
            )
        }
    }
}

/// Go's `%v` for float64 (`%g` with shortest repr, exponent below -4 / from 21).
fn go_float(f: f64) -> String {
    if f == 0.0 {
        return "0".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "+Inf".into() } else { "-Inf".into() };
    }
    let e = format!("{:e}", f); // e.g. "1.5e-7"
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if exp < -4 || exp >= 21 {
        format!("{}e{}{:02}", mant, if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        format!("{}", f)
    }
}

/// Writes the formatted help block for one command.
pub fn render(out: &mut dyn Write, p: &Program, cmd: &Command, config_path: &str) {
    let prog_name = if p.name.is_empty() { "perch" } else { p.name.as_str() };

    let header = if cmd.description.is_empty() {
        cmd.name.clone()
    } else {
        format!("{} — {}", cmd.name, cmd.description)
    };
    let _ = writeln!(out, "{header}");
    let _ = writeln!(out, "{}", "─".repeat(70.min(header.len())));
    let _ = writeln!(out);

    // Usage line.
    let mut usage = format!("{} {}", prog_name, cmd.name);
    for a in &cmd.args {
        if a.index.is_some() {
            if a.has_default || a.optional {
                usage.push_str(&format!(" [{}]", a.name));
            } else {
                usage.push_str(&format!(" <{}>", a.name));
            }
        }
    }
    for a in &cmd.args {
        if a.index.is_some() {
            continue;
        }
        if a.has_default || a.optional {
            usage.push_str(&format!(" [-{}=…]", a.name));
        } else {
            usage.push_str(&format!(" -{}=…", a.name));
        }
    }
    let _ = writeln!(out, "USAGE");
    let _ = writeln!(out, "  {usage}");
    let _ = writeln!(out);

    // Args table.
    if !cmd.args.is_empty() {
        let _ = writeln!(out, "ARGUMENTS");
        // Sort: positional first by index, then flags by name.
        let mut args = cmd.args.clone();
        args.sort_by(|a, b| match (a.index, b.index) {
            (Some(ai), Some(bi)) => ai.cmp(&bi),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.name.cmp(&b.name),
        });
        let mut col_width = 0;
        for a in &args {
            col_width = col_width.max(a.name.len() + 2 + a.ty.len());
        }
        for a in &args {
            let left = if a.index.is_some() {
                format!("<{}> {}", a.name, a.ty)
            } else {
                format!("-{} {}", a.name, a.ty)
            };
            let req = if a.has_default {
                format!("(default {})", go_v(&a.default))
            } else if a.optional {
                "(optional)".to_string()
            } else {
                "(required)".to_string()
            };
            let _ = writeln!(out, "  {:<w$}  {:<12}  {}", left, req, a.description, w = col_width + 2);
        }
        let _ = writeln!(out);
    }

    // Env vars set by the command.
    if !cmd.env.is_empty() {
        let _ = writeln!(out, "ENVIRONMENT (set by this command)");
        // BTreeMap iterates in sorted key order.
        for (k, v) in &cmd.env {
            let _ = writeln!(out, "  {:<20}  {}", k, v);
        }
        let _ = writeln!(out);
    }

    // Modifiers.
    let mut mods: Vec<String> = Vec::new();
    let m = &cmd.modifiers;
    if m.private {
        mods.push("private".into());
    }
    if m.detached {
        mods.push("detached".into());
    }
    if m.proxy_args {
        mods.push("proxy_args".into());
    }
    if !m.require_os.is_empty() {
        mods.push(format!("require_os {}", m.require_os.join("/")));
    }
    if !m.require_arch.is_empty() {
        mods.push(format!("require_arch {}", m.require_arch.join("/")));
    }
    if !m.on_signal.is_empty() {
        mods.push(format!("on_signal {}", m.on_signal));
    }
    if !m.dir.is_empty() {
        mods.push(format!("dir {}", m.dir));
    }
    if !mods.is_empty() {
        let _ = writeln!(out, "MODIFIERS");
        let _ = writeln!(out, "  {}", mods.join(", "));
        let _ = writeln!(out);
    }

    // Examples.
    let ex = examples(cmd, prog_name);
    if !ex.is_empty() {
        let _ = writeln!(out, "EXAMPLES");
        for e in ex {
            let _ = writeln!(out, "  {e}");
        }
        let _ = writeln!(out);
    }

    if !config_path.is_empty() {
        let _ = writeln!(out, "DEFINED IN");
        let _ = writeln!(out, "  {config_path}");
    }
}

fn examples(cmd: &Command, prog_name: &str) -> Vec<String> {
    let mut out = vec![format!("{} {}", prog_name, cmd.name)];
    if !cmd.args.is_empty() {
        // Show one fully-explicit invocation.
        let mut ex = format!("{} {}", prog_name, cmd.name);
        for a in &cmd.args {
            if a.index.is_some() {
                ex.push(' ');
                ex.push_str(&sample_value(a));
            } else {
                ex.push_str(&format!(" -{}={}", a.name, sample_value(a)));
            }
        }
        out.push(ex);
    }
    out
}

fn sample_value(a: &ArgSpec) -> String {
    if a.has_default {
        return go_v(&a.default);
    }
    match a.ty.as_str() {
        "int" => "0".into(),
        "float" => "0.0".into(),
        "bool" => "true".into(),
        _ => "VALUE".into(),
    }
}

fn visible(p: &Program) -> Vec<String> {
    // BTreeMap keys are already sorted.
    p.commands.iter().filter(|(_, c)| !c.modifiers.private).map(|(n, _)| n.clone()).collect()
}

// ────────────────────────────────────────────────────────────────────────
// Fuzzy matching

/// Returns up to 3 candidate command names whose Levenshtein distance to
/// `query` is ≤ 3, ordered by closeness.
pub fn suggest(query: &str, candidates: &[String]) -> Vec<String> {
    let mut ranked: Vec<(&String, usize)> = candidates
        .iter()
        .map(|c| (c, levenshtein(query, c)))
        .filter(|(_, d)| *d <= 3)
        .collect();
    ranked.sort_by_key(|(_, d)| *d); // stable
    ranked.into_iter().take(3).map(|(n, _)| n.clone()).collect()
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
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (cur[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[lb]
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_domain::Modifiers;

    fn prog() -> Program {
        let mut p = Program { name: "app".into(), ..Default::default() };
        let cmd = Command {
            name: "deploy".into(),
            description: "Ship it".into(),
            args: vec![
                ArgSpec { name: "env".into(), ty: "string".into(), description: "target".into(), ..Default::default() },
                ArgSpec {
                    name: "n".into(),
                    ty: "int".into(),
                    description: "count".into(),
                    has_default: true,
                    default: Value::from(3),
                    index: Some(0),
                    ..Default::default()
                },
            ],
            env: [("B".to_string(), "2".to_string()), ("A".to_string(), "1".to_string())].into(),
            modifiers: Modifiers { detached: true, require_os: vec!["linux".into(), "darwin".into()], ..Default::default() },
            ..Default::default()
        };
        p.commands.insert("deploy".into(), cmd);
        p.commands.insert("hidden".into(), Command { name: "hidden".into(), modifiers: Modifiers { private: true, ..Default::default() }, ..Default::default() });
        p
    }

    #[test]
    fn renders_full_block() {
        let p = prog();
        let mut out = Vec::new();
        render(&mut out, &p, &p.commands["deploy"], "commands.perch");
        let want = "deploy — Ship it\n────────────────\n\nUSAGE\n  app deploy [n] -env=…\n\nARGUMENTS\n  <n> int        (default 3)   count\n  -env string    (required)    target\n\nENVIRONMENT (set by this command)\n  A                     1\n  B                     2\n\nMODIFIERS\n  detached, require_os linux/darwin\n\nEXAMPLES\n  app deploy\n  app deploy -env=VALUE 3\n\nDEFINED IN\n  commands.perch\n";
        // header rule: len("deploy — Ship it") in bytes is 18 (em dash = 3 bytes)
        let want = want.replace("────────────────", &"─".repeat(18));
        assert_eq!(String::from_utf8(out).unwrap(), want);
    }

    #[test]
    fn unknown_command_suggests() {
        let p = prog();
        let imp = Impl { load: Box::new(move |_| Ok(p.clone())) };
        let mut out = Vec::new();
        let err = imp.execute("x", "deplyo", &mut out).unwrap_err();
        assert_eq!(err.to_string(), "command not found");
        assert_eq!(String::from_utf8(out).unwrap(), "Unknown command: \"deplyo\"\nDid you mean: deploy?\n");
    }

    #[test]
    fn levenshtein_and_suggest() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        let c: Vec<String> = ["build", "built", "bulid", "zzzzzz", "buil"].iter().map(|s| s.to_string()).collect();
        assert_eq!(suggest("build", &c), vec!["build", "built", "buil"]);
    }

    #[test]
    fn go_float_format() {
        assert_eq!(go_float(3.5), "3.5");
        assert_eq!(go_float(3.0), "3");
        assert_eq!(go_float(1e21), "1e+21");
        assert_eq!(go_float(1e-5), "1e-05");
    }
}
