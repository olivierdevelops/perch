//! Executes a named command from a loaded program.
use perch_domain::Program;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type LoadFn = Box<dyn Fn(&str) -> Result<Program, Error>>;
pub type RunFn = Box<dyn Fn(&Program, &str, &[String]) -> Result<(), Error>>;
pub type SuggestFn = Box<dyn Fn(&str, &[String]) -> Vec<String>>;

pub struct Impl {
    pub load: LoadFn,
    pub run: RunFn,
    /// Optional: fuzzy-match suggestion provider.
    pub suggest: Option<SuggestFn>,
}

impl Impl {
    /// Reports whether the named command is declared in the file at
    /// `config_path`. Errors loading the file count as "no" — the caller
    /// (shebang-default dispatch in cli) handles missing files via a separate
    /// path.
    pub fn has_command(&self, config_path: &str, name: &str) -> bool {
        match (self.load)(config_path) {
            Ok(p) => p.commands.contains_key(name),
            Err(_) => false,
        }
    }

    pub fn execute(&self, config_path: &str, name: &str, args: &[String]) -> Result<(), Error> {
        let p = (self.load)(config_path)?;
        // If the command doesn't exist AND there's no catch, give a friendly
        // "Did you mean…?" before erroring out.
        if !p.commands.contains_key(name) && p.catch.is_none() {
            if let Some(suggest) = &self.suggest {
                let candidates: Vec<String> = p
                    .commands
                    .iter()
                    .filter(|(_, c)| !c.modifiers.private && !c.modifiers.test)
                    .map(|(n, _)| n.clone())
                    .collect();
                let matches = suggest(name, &candidates);
                if !matches.is_empty() {
                    return Err(format!("unknown command {}. Did you mean: {}?", quote(name), matches.join(", ")).into());
                }
            }
            return Err(format!("unknown command {}. Try `perch --help` to list available commands", quote(name)).into());
        }
        (self.run)(&p, name, args)
    }
}

/// Go's `%q` for command names.
fn quote(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use perch_domain::{Catch, Command};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn prog(catch: bool) -> Program {
        let mut p = Program::default();
        p.commands.insert("build".into(), Command::default());
        if catch {
            p.catch = Some(Catch::default());
        }
        p
    }

    fn imp(p: Program, suggest: bool, ran: Rc<RefCell<Vec<String>>>) -> Impl {
        Impl {
            load: Box::new(move |_| Ok(p.clone())),
            run: Box::new(move |_, n, a| {
                ran.borrow_mut().push(format!("{n}:{}", a.join(",")));
                Ok(())
            }),
            suggest: if suggest {
                Some(Box::new(|q, c| c.iter().filter(|x| x.starts_with(&q[..2])).cloned().collect()))
            } else {
                None
            },
        }
    }

    #[test]
    fn runs_known_and_catch() {
        let ran = Rc::new(RefCell::new(vec![]));
        let i = imp(prog(false), false, ran.clone());
        assert!(i.has_command("f", "build"));
        i.execute("f", "build", &["a".into(), "b".into()]).unwrap();
        let i = imp(prog(true), false, ran.clone());
        i.execute("f", "anything", &[]).unwrap();
        assert_eq!(*ran.borrow(), vec!["build:a,b", "anything:"]);
    }

    #[test]
    fn unknown_messages() {
        let ran = Rc::new(RefCell::new(vec![]));
        let e = imp(prog(false), true, ran.clone()).execute("f", "bulid", &[]).unwrap_err();
        assert_eq!(e.to_string(), "unknown command \"bulid\". Did you mean: build?");
        let e = imp(prog(false), false, ran).execute("f", "zz", &[]).unwrap_err();
        assert_eq!(e.to_string(), "unknown command \"zz\". Try `perch --help` to list available commands");
    }
}
