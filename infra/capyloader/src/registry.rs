use std::collections::{BTreeSet, HashMap};

use perch_domain::Program;

use crate::enforce::op_vocabulary;
use crate::loader::go_quote;

/// Enforces the unique-name rule that makes bare-name dispatch unambiguous:
/// every dispatchable name (command, template, and declared bin alias / bare
/// bin name) must be distinct, and a command or template may not shadow a
/// built-in op or grammar keyword. With names globally unique, a bare
/// `deploy` / `ensure_dir` / `tool` resolves to exactly one action with no
/// precedence rules and no disambiguation keywords needed.
///
/// Path-form bins (`./bins/x`) and interpolated bins (`${tool}`) are NOT
/// dispatchable bare names, so they don't participate. Catch handlers aren't
/// bare-invokable either and are excluded.
pub(crate) fn check_name_registry(prog: &Program) -> Result<(), String> {
    let reserved: BTreeSet<&str> = op_vocabulary().iter().map(|s| s.as_str()).collect();

    let mut seen: HashMap<String, &'static str> = HashMap::new(); // name -> category that claimed it
    fn claim(
        reserved: &BTreeSet<&str>,
        seen: &mut HashMap<String, &'static str>,
        name: &str,
        category: &'static str,
    ) -> Result<(), String> {
        if name.is_empty() {
            return Ok(());
        }
        if reserved.contains(name) {
            return Err(format!(
                "{} {} collides with a built-in op/keyword named {} — rename the {}",
                category,
                go_quote(name),
                go_quote(name),
                category
            ));
        }
        if let Some(prev) = seen.get(name) {
            return Err(format!(
                "name {} is declared as both a {} and a {} — names must be unique",
                go_quote(name),
                prev,
                category
            ));
        }
        seen.insert(name.to_string(), category);
        Ok(())
    }

    // BTreeMap iteration is already sorted.
    for n in prog.commands.keys() {
        claim(&reserved, &mut seen, n, "command")?;
    }
    for n in prog.templates.keys() {
        claim(&reserved, &mut seen, n, "template")?;
    }
    // Bin aliases are dispatchable names → fully claimed. A bare-name bin only
    // needs to not collide with a command/template (which would make `name …`
    // ambiguous); two bins sharing a bare name is merely redundant, not fatal.
    for b in &prog.requirements.bins {
        if !b.alias.is_empty() {
            claim(&reserved, &mut seen, &b.alias, "bin alias")?;
        }
        if !b.name.is_empty() && !b.name.contains(['/', '\\']) && !b.name.contains("${") {
            if let Some(prev) = seen.get(&b.name) {
                if *prev != "bin" {
                    return Err(format!(
                        "name {} is declared as both a {} and a bin — names must be unique",
                        go_quote(&b.name),
                        prev
                    ));
                }
            }
            seen.entry(b.name.clone()).or_insert("bin");
        }
    }
    Ok(())
}
