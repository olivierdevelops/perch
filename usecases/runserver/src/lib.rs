//! Serves a loaded perch program over HTTP.
use perch_domain::Program;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type LoadFn = Box<dyn Fn(&str) -> Result<Program, Error>>;

/// Mirrors `httpserver::Server::serve` plus `config_path` — the UI displays the
/// source path and uses it for context in the check/scan/simulate panels.
pub type ServeFn = Box<dyn Fn(&Program, &str, i64, &str) -> Result<(), Error>>;

pub struct Impl {
    pub load: LoadFn,
    pub serve: ServeFn,
}

const USAGE: &str = "Usage of server:\n  -host string\n    \tHost (default \"127.0.0.1\")\n  -port int\n    \tPort (default 10032)";

/// Parses `-port N` / `--port=N` / `-host H` like Go's `flag` package
/// (defaults: port 10032, host 127.0.0.1). Go's ExitOnError prints the message
/// plus usage and exits 2; here both are the returned error's text.
fn parse_flags(args: &[String]) -> Result<(String, i64), Error> {
    let mut port: i64 = 10032;
    let mut host = "127.0.0.1".to_string();
    let fail = |msg: String| -> Error { format!("{msg}\n{USAGE}").into() };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        // Flag parsing stops at the first non-flag argument or "--".
        if !a.starts_with('-') || a == "-" || a == "--" {
            break;
        }
        let name = a.trim_start_matches('-');
        if name.is_empty() || name.starts_with('-') || name.starts_with('=') || a.starts_with("---") {
            return Err(fail(format!("bad flag syntax: {a}")));
        }
        let (name, inline) = match name.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (name, None),
        };
        if name != "port" && name != "host" {
            return Err(fail(format!("flag provided but not defined: -{name}")));
        }
        i += 1;
        let value = match inline {
            Some(v) => v,
            None => match args.get(i) {
                Some(v) => {
                    i += 1;
                    v.clone()
                }
                None => return Err(fail(format!("flag needs an argument: -{name}"))),
            },
        };
        if name == "port" {
            port = value
                .parse::<i64>()
                .map_err(|e| {
                    let why = if matches!(e.kind(), std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow) {
                        "value out of range"
                    } else {
                        "parse error"
                    };
                    fail(format!("invalid value \"{value}\" for flag -port: {why}"))
                })?;
        } else {
            host = value;
        }
    }
    Ok((host, port))
}

impl Impl {
    pub fn execute(&self, config_path: &str, args: &[String]) -> Result<(), Error> {
        let (host, port) = parse_flags(args)?;
        let p = (self.load)(config_path)?;
        (self.serve)(&p, &host, port, config_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn defaults_and_flags() {
        assert_eq!(parse_flags(&[]).unwrap(), ("127.0.0.1".into(), 10032));
        assert_eq!(parse_flags(&s(&["-port", "8080", "--host=0.0.0.0"])).unwrap(), ("0.0.0.0".into(), 8080));
        assert!(parse_flags(&s(&["-nope"])).unwrap_err().to_string().contains("flag provided but not defined: -nope"));
        assert!(parse_flags(&s(&["-port", "x"])).is_err());
    }

    #[test]
    fn serves_with_loaded_program() {
        let got = Rc::new(RefCell::new(String::new()));
        let g = got.clone();
        let i = Impl {
            load: Box::new(|_| Ok(Program::default())),
            serve: Box::new(move |_, h, p, c| {
                *g.borrow_mut() = format!("{h}:{p}:{c}");
                Ok(())
            }),
        };
        i.execute("c.perch", &s(&["-port=9"])).unwrap();
        assert_eq!(*got.borrow(), "127.0.0.1:9:c.perch");
    }
}
