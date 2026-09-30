//! The perch REPL: each line is wrapped as a throwaway
//! `command __repl__ do <LINE> end`, fed through capy, and dispatched by the
//! interpreter. Bindings persist across lines so `let x = …` then
//! `print "${x}"` works one line apart.
use perch_domain::{Command, Program};
use perch_interpreter::{Bindings, Handler, Interpreter};
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, Write};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Compiles a perch source string into a Program.
pub type LoadStringFn = Box<dyn Fn(&str) -> Result<Program, Error> + Send + Sync>;

/// Loads the host program (the user's commands.perch) so REPL lines can
/// `run other_cmd`.
pub type LoadFn = Box<dyn Fn(&str) -> Result<Program, Error> + Send + Sync>;

/// The op-handler registry.
pub type Handlers = HashMap<String, Handler>;

pub struct Impl {
    pub load_string: LoadStringFn,
    pub load: LoadFn,
    pub handlers: Handlers,
}

const REPL_COMMAND: &str = "__repl__";

impl Impl {
    pub fn execute(&self, config_path: &str) -> Result<(), Error> {
        let stdin = std::io::stdin();
        self.execute_with(config_path, &mut stdin.lock(), &mut std::io::stdout())
    }

    /// Runs the REPL reading lines from `input` and printing prompts to `out`.
    pub fn execute_with(
        &self,
        config_path: &str,
        input: &mut dyn BufRead,
        out: &mut dyn Write,
    ) -> Result<(), Error> {
        // Allow the REPL to run even without a host file.
        let host = (self.load)(config_path)
            .unwrap_or_else(|_| Program { commands: BTreeMap::new(), ..Default::default() });

        let intr = Interpreter::new(self.handlers.clone(), host.clone());
        let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let mut binds = Bindings::new(&cwd);
        for g in &host.globals.bindings {
            binds.set(&g.name, g.value.clone());
        }

        let _ = writeln!(out, "perch shell — Ctrl-D to exit. Each line runs as one op.");
        let _ = writeln!(out, "Available host commands: {}", command_list(&host));

        loop {
            let _ = write!(out, "» ");
            let _ = out.flush();
            let mut line = String::new();
            let n = input.read_line(&mut line)?;
            // Go's ReadString returns io.EOF (dropping any partial line).
            if n == 0 || !line.ends_with('\n') {
                let _ = writeln!(out);
                return Ok(());
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line == "exit" || line == "quit" {
                return Ok(());
            }
            if let Err(e) = self.run_line(&intr, &host, &mut binds, line) {
                eprintln!("error: {e}");
            }
        }
    }

    fn run_line(
        &self,
        intr: &Interpreter,
        host: &Program,
        b: &mut Bindings,
        line: &str,
    ) -> Result<(), Error> {
        // A bare host-command name just dispatches it.
        let first = line.split(' ').next().unwrap_or("");
        if let Some(cmd) = host.commands.get(first) {
            if !cmd.modifiers.private {
                return intr.run_ops(&cmd.ops, b);
            }
        }

        // Otherwise wrap the line as a throwaway command body.
        let src = format!("command {REPL_COMMAND}\n    do\n        {line}\n    end\nend\n");
        let tmp = (self.load_string)(&src)?;
        let cmd: &Command = tmp
            .commands
            .get(REPL_COMMAND)
            .ok_or_else(|| -> Error { "internal: failed to wrap REPL line".into() })?;
        intr.run_ops(&cmd.ops, b)
    }
}

fn command_list(p: &Program) -> String {
    if p.commands.is_empty() {
        return "(none)".to_string();
    }
    p.commands
        .iter()
        .filter(|(_, c)| !c.modifiers.private)
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imp(load_ok: bool) -> Impl {
        Impl {
            load_string: Box::new(|_| Err("no capy".into())),
            load: Box::new(move |_| {
                if load_ok {
                    let mut p = Program::default();
                    for (n, private) in [("b", false), ("a", false), ("hid", true)] {
                        let mut c = Command { name: n.into(), ..Default::default() };
                        c.modifiers.private = private;
                        p.commands.insert(n.into(), c);
                    }
                    Ok(p)
                } else {
                    Err("missing".into())
                }
            }),
            handlers: HashMap::new(),
        }
    }

    #[test]
    fn banner_and_eof() {
        let mut out = Vec::new();
        imp(true).execute_with("x", &mut "".as_bytes(), &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "perch shell — Ctrl-D to exit. Each line runs as one op.\nAvailable host commands: a, b\n» \n"
        );
    }

    #[test]
    fn no_host_file_and_exit() {
        let mut out = Vec::new();
        imp(false).execute_with("x", &mut "\n  exit\n".as_bytes(), &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("Available host commands: (none)\n» » "));
    }

    #[test]
    fn wrap_failure_reports_and_continues() {
        let mut out = Vec::new();
        imp(false).execute_with("x", &mut "print hi\nquit\n".as_bytes(), &mut out).unwrap();
    }
}
