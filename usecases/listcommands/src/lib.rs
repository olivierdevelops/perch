//! Prints a summary of the loaded program.
use perch_domain::Program;
use serde_json::Value;
use std::io::Write;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type LoadFn = Box<dyn Fn(&str) -> Result<Program, Error>>;

pub struct Impl {
    pub load: LoadFn,
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
    let e = format!("{:e}", f);
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    if !(-4..21).contains(&exp) {
        format!("{}e{}{:02}", mant, if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        format!("{}", f)
    }
}

impl Impl {
    pub fn execute(&self, config_path: &str, out: &mut dyn Write) -> Result<(), Error> {
        let p = (self.load)(config_path)?;
        writeln!(out, "Name:        {}", p.name)?;
        writeln!(out, "Version:     {}", p.version)?;
        writeln!(out, "Description: {}", p.description)?;
        writeln!(out, "{}", "─".repeat(70))?;
        writeln!(out)?;

        if !p.globals.bindings.is_empty() {
            writeln!(out, "Globals:")?;
            for g in &p.globals.bindings {
                writeln!(out, "  {:<20} ({}) = {}", g.name, g.ty, go_v(&g.value))?;
            }
            writeln!(out)?;
        }

        // BTreeMap iteration is already sorted by name.
        let keys: Vec<&String> = p
            .commands
            .iter()
            .filter(|(_, c)| !(c.modifiers.private || c.modifiers.test))
            .map(|(k, _)| k)
            .collect();

        write!(out, "Commands ({}):\n\n", keys.len())?;
        for k in keys {
            let c = &p.commands[k];
            writeln!(out, "  ▸ {k}")?;
            if !c.description.is_empty() {
                for line in c.description.split('\n') {
                    let t = line.trim();
                    if !t.is_empty() {
                        writeln!(out, "      {t}")?;
                    }
                }
            }
            for a in &c.args {
                let def = if a.has_default { format!(" (default {})", go_v(&a.default)) } else { String::new() };
                writeln!(out, "        -{} {}{} — {}", a.name, a.ty, def, a.description)?;
            }
            writeln!(out)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_domain::{ArgSpec, Command, GlobalBinding, Modifiers};

    #[test]
    fn lists_visible_commands() {
        let mut p = Program { name: "n".into(), version: "1".into(), description: "d".into(), ..Default::default() };
        p.globals.bindings.push(GlobalBinding { name: "verbose".into(), ty: "bool".into(), value: Value::Bool(false) });
        p.commands.insert(
            "b".into(),
            Command {
                name: "b".into(),
                description: "line one\n\n  line two ".into(),
                args: vec![ArgSpec { name: "x".into(), ty: "int".into(), description: "the x".into(), has_default: true, default: Value::from(2), ..Default::default() }],
                ..Default::default()
            },
        );
        p.commands.insert("a".into(), Command { name: "a".into(), ..Default::default() });
        p.commands.insert("t".into(), Command { modifiers: Modifiers { test: true, ..Default::default() }, ..Default::default() });
        let imp = Impl { load: Box::new(move |_| Ok(p.clone())) };
        let mut out = Vec::new();
        imp.execute("f", &mut out).unwrap();
        let bar = "─".repeat(70);
        let want = format!("Name:        n\nVersion:     1\nDescription: d\n{bar}\n\nGlobals:\n  verbose              (bool) = false\n\nCommands (2):\n\n  ▸ a\n\n  ▸ b\n      line one\n      line two\n        -x int (default 2) — the x\n\n");
        assert_eq!(String::from_utf8(out).unwrap(), want);
    }
}
