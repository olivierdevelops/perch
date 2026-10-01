//! Parses argv into an intent and dispatches to use-cases. A hand port of the
//! Go dispatcher so the flag surface and help text stay byte-identical.
use std::collections::HashMap;
use std::io::Write;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

const COMPLETION_BASH: &str = include_str!("../completions/perch.bash");
const COMPLETION_ZSH: &str = include_str!("../completions/perch.zsh");
const COMPLETION_FISH: &str = include_str!("../completions/perch.fish");

pub trait RunCommandUseCase {
    fn execute(&self, config_path: &str, command_name: &str, args: &[String]) -> Result<(), Error>;
    /// Whether the named command is declared in the file at `config_path`.
    fn has_command(&self, config_path: &str, command_name: &str) -> bool;
}
pub trait ListCommandsUseCase {
    fn execute(&self, config_path: &str) -> Result<(), Error>;
}
pub trait InitConfigUseCase {
    fn execute(&self, path: &str) -> Result<(), Error>;
}
pub trait RunBuildUseCase {
    fn execute(&self, config_path: &str, args: &[String]) -> Result<(), Error>;
}
pub trait RunServerUseCase {
    fn execute(&self, config_path: &str, args: &[String]) -> Result<(), Error>;
}
pub trait RunShellUseCase {
    fn execute(&self, config_path: &str) -> Result<(), Error>;
}
pub trait ValidateUseCase {
    fn execute(&self, config_path: &str) -> Result<(), Error>;
}
pub trait CommandHelpUseCase {
    fn execute(&self, config_path: &str, command_name: &str) -> Result<(), Error>;
}
pub trait InstallLSPUseCase {
    fn execute(&self) -> Result<(), Error>;
}
pub trait InstallVSCodeUseCase {
    fn execute(&self) -> Result<(), Error>;
}
pub trait ImportShUseCase {
    fn execute(&self, src_path: &str, out_path: &str) -> Result<(), Error>;
}
pub trait ScanUseCase {
    /// `format` is "text" or "json" (validated by the CLI).
    fn execute(&self, config_path: &str, format: &str) -> Result<(), Error>;
}
pub trait HelpUseCase {
    fn execute(&self, topic: &str, as_json: bool) -> Result<(), Error>;
}
/// Runs every command marked with the `test` modifier.
pub trait TestUseCase {
    fn execute(&self, config_path: &str, filter: &str, verbose: bool) -> Result<(), Error>;
}
/// Analyzes the program against a hypothetical runtime environment.
pub trait SimulateUseCase {
    fn execute(
        &self,
        config_path: &str,
        command_name: &str,
        env: &SimulateEnv,
        fixture_path: &str,
        w: &mut dyn Write,
    ) -> Result<(), Error>;
}
pub trait ExportOpsCatalogUseCase {
    fn execute(&self, path: &str) -> Result<(), Error>;
}

/// The CLI-side mirror of `usecases/simulate::SimEnv`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimulateEnv {
    pub os: String,
    pub arch: String,
    // `None` = flag not given; `Some(empty)` = given but empty (Go's nil vs
    // empty map/slice), which simulate treats differently (e.g. network blocked).
    pub env: Option<HashMap<String, String>>,
    pub env_restrict: bool,
    pub fs_read: Option<Vec<String>>,
    pub fs_write: Option<Vec<String>>,
    pub bins: Option<HashMap<String, bool>>,
    pub network: Option<Vec<String>>,
    pub no_shell: bool,
    pub no_subprocess: bool,
    pub no_network: bool,
    pub no_write: bool,
}

pub struct UseCases {
    pub run: Box<dyn RunCommandUseCase>,
    pub list: Box<dyn ListCommandsUseCase>,
    pub init: Box<dyn InitConfigUseCase>,
    pub build: Box<dyn RunBuildUseCase>,
    pub server: Box<dyn RunServerUseCase>,
    pub shell: Box<dyn RunShellUseCase>,
    pub validate: Box<dyn ValidateUseCase>,
    pub command_help: Box<dyn CommandHelpUseCase>,
    pub install_lsp: Box<dyn InstallLSPUseCase>,
    pub install_vscode: Box<dyn InstallVSCodeUseCase>,
    pub import_sh: Box<dyn ImportShUseCase>,
    pub scan: Box<dyn ScanUseCase>,
    pub help: Box<dyn HelpUseCase>,
    pub test: Box<dyn TestUseCase>,
    pub simulate: Box<dyn SimulateUseCase>,
    pub export_ops_catalog: Box<dyn ExportOpsCatalogUseCase>,
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub default_commands_file: String,
    pub version: String,
}

pub struct Cli {
    pub use_cases: UseCases,
    pub config: Config,
}

/// Sentinel used when `perch ./script.perch` is invoked with no command after
/// it; the dispatcher resolves it to `main` (if declared) or a listing.
const SCRIPT_DEFAULT_COMMAND: &str = "\0__script_default__";

impl Cli {
    /// Runs against the process argv (Go: `os.Args`), returning the exit code.
    pub fn run(&self) -> i32 {
        let argv: Vec<String> = std::env::args().collect();
        self.run_args(&argv)
    }

    /// Runs against an explicit argv (including the program name at index 0).
    pub fn run_args(&self, argv: &[String]) -> i32 {
        if argv.len() <= 1 {
            self.print_help();
            return 0;
        }
        let args = &argv[1..];

        if let Some(path) = parse_export_flag(args) {
            return err_exit(self.use_cases.export_ops_catalog.execute(&path));
        }

        let command_name: String;
        let remaining: Vec<String>;

        if args[0] == "-f" {
            if args.len() < 3 {
                println!("Usage: perch -f <file> <command> [args...]");
                return 1;
            }
            command_name = args[2].clone();
            let mut r = vec!["-f".to_string(), args[1].clone()];
            r.extend_from_slice(&args[3..]);
            remaining = r;
        } else if looks_like_script_path(&args[0]) {
            // Shebang-style invocation: `perch /abs/path/to/script.perch ARGS…`.
            let file_path = args[0].clone();
            if args.len() >= 2 {
                command_name = args[1].clone();
                let mut r = vec!["-f".to_string(), file_path];
                r.extend_from_slice(&args[2..]);
                remaining = r;
            } else {
                command_name = SCRIPT_DEFAULT_COMMAND.to_string();
                remaining = vec!["-f".to_string(), file_path];
            }
        } else {
            command_name = args[0].clone();
            remaining = args[1..].to_vec();
        }

        let def = &self.config.default_commands_file;
        let uc = &self.use_cases;
        match command_name.as_str() {
            "--help" | "-h" => {
                let (path, _) = parse_file_flag(&remaining, def);
                return err_exit(uc.list.execute(&path));
            }
            "--version" => {
                println!("{}", self.config.version);
                return 0;
            }
            "--init" => {
                let (path, _) = parse_file_flag(&remaining, def);
                return err_exit(uc.init.execute(&path));
            }
            "--build" => {
                let (path, rest) = parse_file_flag(&remaining, def);
                return err_exit(uc.build.execute(&path, &rest));
            }
            "--server" => {
                let (path, rest) = parse_file_flag(&remaining, def);
                return err_exit(uc.server.execute(&path, &rest));
            }
            "--shell" => {
                let (path, _) = parse_file_flag(&remaining, def);
                return err_exit(uc.shell.execute(&path));
            }
            "--completions" => return print_completions(&remaining),
            "--check" | "--validate" => {
                let (path, _) = parse_file_flag(&remaining, def);
                return err_exit(uc.validate.execute(&path));
            }
            "test" | "--test" => {
                let (path, rest) = parse_file_flag(&remaining, def);
                let (filter, verbose) = parse_test_flags(&rest);
                return err_exit(uc.test.execute(&path, &filter, verbose));
            }
            "simulate" | "--simulate" => {
                let (path, rest) = parse_file_flag(&remaining, def);
                let (cmd_name, env, fixture) = parse_simulate_flags(&rest);
                return err_exit(uc.simulate.execute(&path, &cmd_name, &env, &fixture, &mut std::io::stdout()));
            }
            "--scan" => {
                let (path, rest) = parse_file_flag(&remaining, def);
                let format = match parse_scan_format(&rest) {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("{e}");
                        return 2;
                    }
                };
                return err_exit(uc.scan.execute(&path, format));
            }
            "help" => {
                let mut topic = String::new();
                let mut as_json = false;
                for a in &remaining {
                    if a == "--json" {
                        as_json = true;
                        continue;
                    }
                    if topic.is_empty() {
                        topic = a.clone();
                    }
                }
                return err_exit(uc.help.execute(&topic, as_json));
            }
            "--install-lsp" => return err_exit(uc.install_lsp.execute()),
            "--install-vscode" => return err_exit(uc.install_vscode.execute()),
            "--import" => {
                if remaining.is_empty() {
                    println!("Usage: perch --import <script.sh> [-o <out.perch>]");
                    return 1;
                }
                let src = &remaining[0];
                let mut out = "";
                let mut i = 1;
                while i + 1 < remaining.len() {
                    if remaining[i] == "-o" {
                        out = &remaining[i + 1];
                        break;
                    }
                    i += 1;
                }
                return err_exit(uc.import_sh.execute(src, out));
            }
            _ => {}
        }

        let (path, rest) = parse_file_flag(&remaining, def);

        // `--help`/`-h` after the command routes to per-command help.
        if has_help(&rest) {
            return err_exit(uc.command_help.execute(&path, &command_name));
        }

        // Shebang invocation with no command: try `main`, else list.
        if command_name == SCRIPT_DEFAULT_COMMAND {
            if uc.run.has_command(&path, "main") {
                return err_exit(uc.run.execute(&path, "main", &rest));
            }
            return err_exit(uc.list.execute(&path));
        }

        err_exit(uc.run.execute(&path, &command_name, &rest))
    }

    fn print_help(&self) {
        print!("{}", help_text());
    }
}

/// The top-level `perch` usage text.
pub fn help_text() -> &'static str {
    "perch — a cross-platform command runner

Usage:
  perch <command> [args...]            Run a command from commands.perch
  perch -f <file> <command> [args...]  Run a command from a custom file

Flags:
  --help        Show summary of the commands file
  --version     Print perch version
  --init        Write a starter commands.perch in the current dir
  --build -o X  Bundle commands.perch into a portable binary at X
  --server      Serve commands.perch as an HTTP UI
  --shell       REPL — type one op per line; bindings persist
  --check       Parse and statically check commands.perch; report problems
  --scan [--json | --format text|json]
                Audit what commands.perch can reach; JSON separates declared from inferred
  --completions SHELL  Print shell completions (bash|zsh|fish)
  --install-lsp        Install the perch-lsp language server (release download, sha256-verified)
  --install-vscode     Install perch-lsp + the perch VS Code extension
  --allow-advisory-scopes
                Where spawned binaries cannot be confined to the declared read/write/host
                scopes (Windows, old kernels), run them unconfined with a stderr banner
                instead of refusing (default: refuse)

Per-command help:
  perch <command> --help   Show args, defaults, examples for one command

Options:
  -f <path>     Specify the config file (default: commands.perch)
"
}

/// Whether argv requests the language catalog export. Bare `--export` writes
/// JSON to stdout (`-`); `--export=PATH` writes to PATH.
pub fn parse_export_flag(args: &[String]) -> Option<String> {
    for a in args {
        if a == "--export" {
            return Some("-".to_string());
        }
        if let Some(p) = a.strip_prefix("--export=") {
            return Some(if p.is_empty() { "-".to_string() } else { p.to_string() });
        }
    }
    None
}

fn err_exit(r: Result<(), Error>) -> i32 {
    match r {
        Err(e) => {
            println!("{e}");
            1
        }
        Ok(()) => 0,
    }
}

fn print_completions(args: &[String]) -> i32 {
    let Some(shell) = args.first() else {
        eprintln!("usage: perch --completions <bash|zsh|fish>");
        return 1;
    };
    match shell.as_str() {
        "bash" => print!("{COMPLETION_BASH}"),
        "zsh" => print!("{COMPLETION_ZSH}"),
        "fish" => print!("{COMPLETION_FISH}"),
        other => {
            eprintln!("unknown shell: {} (use bash|zsh|fish)", go_quote(other));
            return 1;
        }
    }
    0
}

/// Minimal `strconv.Quote` for the common case (shell names).
fn go_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Extracts `-f PATH` from args, returning the path (or `def`) and the rest.
/// A trailing `-f` with no path exits the process, like the Go version.
pub fn parse_file_flag(args: &[String], def: &str) -> (String, Vec<String>) {
    for (i, a) in args.iter().enumerate() {
        if a == "-f" {
            if i + 1 >= args.len() {
                println!("-f flag requires a file path");
                std::process::exit(1);
            }
            let path = args[i + 1].clone();
            let mut out: Vec<String> = args[..i].to_vec();
            if i + 2 < args.len() {
                out.extend_from_slice(&args[i + 2..]);
            }
            return (path, out);
        }
    }
    (def.to_string(), args.to_vec())
}

/// Reads `--json`, `--format json|text` and `--format=json|text` after
/// `--scan`. Default "text"; an unknown value is an error (exit 2).
pub fn parse_scan_format(args: &[String]) -> Result<&'static str, String> {
    let mut fmt: &'static str = "text";
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let val = if a == "--json" {
            Some("json".to_string())
        } else if let Some(v) = a.strip_prefix("--format=") {
            Some(v.to_string())
        } else if a == "--format" {
            i += 1;
            match args.get(i) {
                Some(v) => Some(v.clone()),
                None => return Err("--format requires a value (text or json)".into()),
            }
        } else {
            None
        };
        if let Some(v) = val {
            fmt = match v.as_str() {
                "json" => "json",
                "text" => "text",
                other => return Err(format!("unknown --format value {other:?} for --scan (want text or json)")),
            };
        }
        i += 1;
    }
    Ok(fmt)
}

/// Strips the `test`-specific flags: `--filter PAT`, `--filter=PAT`,
/// `-v` / `--verbose`. Unrecognised tokens are ignored.
pub fn parse_test_flags(args: &[String]) -> (String, bool) {
    let mut filter = String::new();
    let mut verbose = false;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--filter" {
            if i + 1 < args.len() {
                filter = args[i + 1].clone();
                i += 1;
            }
        } else if a.len() > 9 && a.starts_with("--filter=") {
            filter = a[9..].to_string();
        } else if a == "-v" || a == "--verbose" {
            verbose = true;
        }
        i += 1;
    }
    (filter, verbose)
}

fn string_list(s: &str) -> Vec<String> {
    s.split(',').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect()
}

fn kv_map(s: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for pair in s.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        match pair.find('=') {
            Some(eq) if eq > 0 => {
                out.insert(pair[..eq].to_string(), pair[eq + 1..].to_string());
            }
            _ => {
                out.insert(pair.to_string(), String::new());
            }
        }
    }
    out
}

fn bin_set(s: &str) -> HashMap<String, bool> {
    string_list(s).into_iter().map(|b| (b, true)).collect()
}

/// Reads the `perch simulate` argv: the command name plus the `--sim-*` flags
/// (each accepting `--flag VALUE` or `--flag=VALUE`).
pub fn parse_simulate_flags(args: &[String]) -> (String, SimulateEnv, String) {
    let mut cmd = String::new();
    let mut env = SimulateEnv::default();
    let mut fixture = String::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        // Value of `--flag VALUE` (empty when missing), advancing past it.
        let mut take = || -> String {
            if i + 1 >= args.len() {
                return String::new();
            }
            i += 1;
            args[i].clone()
        };
        if let Some(v) = a.strip_prefix("--sim-os=") {
            env.os = v.to_string();
        } else if a == "--sim-os" {
            env.os = take();
        } else if let Some(v) = a.strip_prefix("--sim-arch=") {
            env.arch = v.to_string();
        } else if a == "--sim-arch" {
            env.arch = take();
        } else if let Some(v) = a.strip_prefix("--sim-env=") {
            env.env = Some(kv_map(v));
        } else if a == "--sim-env" {
            env.env = Some(kv_map(&take()));
        } else if a == "--sim-env-only" {
            env.env_restrict = true;
        } else if let Some(v) = a.strip_prefix("--sim-fs-read=") {
            env.fs_read = Some(string_list(v));
        } else if a == "--sim-fs-read" {
            env.fs_read = Some(string_list(&take()));
        } else if let Some(v) = a.strip_prefix("--sim-fs-write=") {
            env.fs_write = Some(string_list(v));
        } else if a == "--sim-fs-write" {
            env.fs_write = Some(string_list(&take()));
        } else if let Some(v) = a.strip_prefix("--sim-have-bin=") {
            env.bins = Some(bin_set(v));
        } else if a == "--sim-have-bin" {
            env.bins = Some(bin_set(&take()));
        } else if let Some(v) = a.strip_prefix("--sim-allow-host=") {
            env.network = Some(string_list(v));
        } else if a == "--sim-allow-host" {
            env.network = Some(string_list(&take()));
        } else if a == "--sim-no-shell" {
            env.no_shell = true;
        } else if a == "--sim-no-subprocess" {
            env.no_subprocess = true;
        } else if a == "--sim-no-network" {
            env.no_network = true;
        } else if a == "--sim-no-write" {
            env.no_write = true;
        } else if let Some(v) = a.strip_prefix("--sim-file=") {
            fixture = v.to_string();
        } else if a == "--sim-file" {
            fixture = take();
        } else if cmd.is_empty() && !a.starts_with('-') {
            cmd = a.to_string();
        }
        i += 1;
    }
    (cmd, env, fixture)
}

/// Whether arg looks like a path to a `.perch` file (shebang-style invocation).
/// Must be an existing regular file that ends in `.perch` or is path-shaped.
pub fn looks_like_script_path(arg: &str) -> bool {
    let path_shaped = arg.ends_with(".perch")
        || arg.starts_with("./")
        || arg.starts_with("../")
        || arg.starts_with('/')
        || arg.starts_with('~')
        || arg.contains('/');
    if !path_shaped {
        return false;
    }
    match std::fs::metadata(arg) {
        Ok(m) => !m.is_dir(),
        Err(_) => false,
    }
}

fn has_help(args: &[String]) -> bool {
    args.iter().any(|a| a == "--help" || a == "-h")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn export_flag() {
        assert_eq!(parse_export_flag(&s(&["--export"])), Some("-".into()));
        assert_eq!(parse_export_flag(&s(&["a", "--export=x.json"])), Some("x.json".into()));
        assert_eq!(parse_export_flag(&s(&["--export="])), Some("-".into()));
        assert_eq!(parse_export_flag(&s(&["build"])), None);
    }

    #[test]
    fn file_flag() {
        let (p, r) = parse_file_flag(&s(&["a", "-f", "x.perch", "b"]), "d");
        assert_eq!((p.as_str(), r), ("x.perch", s(&["a", "b"])));
        let (p, r) = parse_file_flag(&s(&["a"]), "d");
        assert_eq!((p.as_str(), r), ("d", s(&["a"])));
    }

    #[test]
    fn test_flags() {
        assert_eq!(parse_test_flags(&s(&["--filter", "x", "-v"])), ("x".into(), true));
        assert_eq!(parse_test_flags(&s(&["--filter=abc"])), ("abc".into(), false));
        assert_eq!(parse_test_flags(&s(&["--filter="])), ("".into(), false));
        assert_eq!(parse_test_flags(&s(&["--filter"])), ("".into(), false));
    }

    #[test]
    fn simulate_flags() {
        let (cmd, env, fx) = parse_simulate_flags(&s(&[
            "deploy", "--sim-os", "linux", "--sim-arch=arm64", "--sim-env", "A=1,B, C=x=y", "--sim-env-only",
            "--sim-fs-read", "a, b,,c", "--sim-have-bin=git,go", "--sim-no-shell", "--sim-file", "f.json",
        ]));
        assert_eq!(cmd, "deploy");
        assert_eq!(fx, "f.json");
        assert_eq!(env.os, "linux");
        assert_eq!(env.arch, "arm64");
        assert_eq!(env.env.as_ref().unwrap().get("C").unwrap(), "x=y");
        assert_eq!(env.env.as_ref().unwrap().get("B").unwrap(), "");
        assert!(env.env_restrict && env.no_shell && !env.no_write);
        assert_eq!(env.fs_read, Some(s(&["a", "b", "c"])));
        assert!(env.network.is_none());
        let bins = env.bins.unwrap();
        assert!(bins["git"] && bins["go"]);
    }

    #[test]
    fn script_path_detection() {
        assert!(!looks_like_script_path("deploy"));
        assert!(!looks_like_script_path("./definitely-missing.perch"));
        assert!(!looks_like_script_path("/tmp"));
    }

    #[test]
    fn help_text_shape() {
        let h = help_text();
        assert!(h.starts_with("perch — a cross-platform command runner\n\nUsage:\n"));
        assert!(h.ends_with("  -f <path>     Specify the config file (default: commands.perch)\n"));
    }

    type Log = Arc<Mutex<Vec<String>>>;
    struct Rec(Log);
    impl Rec {
        fn add(&self, m: String) {
            self.0.lock().unwrap().push(m);
        }
    }
    macro_rules! rec_impl {
        ($tr:ident, $($body:tt)*) => { impl $tr for Rec { $($body)* } };
    }
    rec_impl!(RunCommandUseCase,
        fn execute(&self, c: &str, n: &str, a: &[String]) -> Result<(), Error> { self.add(format!("run {c} {n} {a:?}")); Ok(()) }
        fn has_command(&self, _: &str, _: &str) -> bool { false }
    );
    rec_impl!(ListCommandsUseCase, fn execute(&self, c: &str) -> Result<(), Error> { self.add(format!("list {c}")); Ok(()) });
    rec_impl!(InitConfigUseCase, fn execute(&self, c: &str) -> Result<(), Error> { self.add(format!("init {c}")); Ok(()) });
    rec_impl!(RunBuildUseCase, fn execute(&self, c: &str, a: &[String]) -> Result<(), Error> { self.add(format!("build {c} {a:?}")); Ok(()) });
    rec_impl!(RunServerUseCase, fn execute(&self, c: &str, a: &[String]) -> Result<(), Error> { self.add(format!("server {c} {a:?}")); Ok(()) });
    rec_impl!(RunShellUseCase, fn execute(&self, c: &str) -> Result<(), Error> { self.add(format!("shell {c}")); Ok(()) });
    rec_impl!(ValidateUseCase, fn execute(&self, c: &str) -> Result<(), Error> { self.add(format!("validate {c}")); Ok(()) });
    rec_impl!(CommandHelpUseCase, fn execute(&self, c: &str, n: &str) -> Result<(), Error> { self.add(format!("cmdhelp {c} {n}")); Ok(()) });
    rec_impl!(InstallLSPUseCase, fn execute(&self) -> Result<(), Error> { self.add("lsp".into()); Ok(()) });
    rec_impl!(InstallVSCodeUseCase, fn execute(&self) -> Result<(), Error> { self.add("vscode".into()); Ok(()) });
    rec_impl!(ImportShUseCase, fn execute(&self, a: &str, b: &str) -> Result<(), Error> { self.add(format!("import {a} {b:?}")); Ok(()) });
    rec_impl!(ScanUseCase, fn execute(&self, c: &str, f: &str) -> Result<(), Error> { self.add(format!("scan {c} {f}")); Ok(()) });
    rec_impl!(HelpUseCase, fn execute(&self, t: &str, j: bool) -> Result<(), Error> { self.add(format!("help {t:?} {j}")); Ok(()) });
    rec_impl!(TestUseCase, fn execute(&self, c: &str, f: &str, v: bool) -> Result<(), Error> { self.add(format!("test {c} {f:?} {v}")); Ok(()) });
    rec_impl!(SimulateUseCase, fn execute(&self, c: &str, n: &str, e: &SimulateEnv, f: &str, _: &mut dyn Write) -> Result<(), Error> { self.add(format!("sim {c} {n:?} {} {f:?}", e.os)); Ok(()) });
    rec_impl!(ExportOpsCatalogUseCase, fn execute(&self, p: &str) -> Result<(), Error> { self.add(format!("export {p}")); Err("boom".into()) });

    fn cli() -> (Cli, Log) {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let r = || Rec(log.clone());
        let c = Cli {
            use_cases: UseCases {
                run: Box::new(r()), list: Box::new(r()), init: Box::new(r()), build: Box::new(r()),
                server: Box::new(r()), shell: Box::new(r()), validate: Box::new(r()), command_help: Box::new(r()),
                install_lsp: Box::new(r()), install_vscode: Box::new(r()), import_sh: Box::new(r()),
                scan: Box::new(r()), help: Box::new(r()), test: Box::new(r()), simulate: Box::new(r()),
                export_ops_catalog: Box::new(r()),
            },
            config: Config { default_commands_file: "commands.perch".into(), version: "1.2.3".into() },
        };
        (c, log)
    }

    fn dispatch(args: &[&str]) -> (i32, Vec<String>) {
        let (c, log) = cli();
        let mut argv = vec!["perch".to_string()];
        argv.extend(args.iter().map(|a| a.to_string()));
        let code = c.run_args(&argv);
        let l = log.lock().unwrap().clone();
        (code, l)
    }

    #[test]
    fn dispatch_table() {
        assert_eq!(dispatch(&["build", "a"]), (0, vec!["run commands.perch build [\"a\"]".to_string()]));
        assert_eq!(dispatch(&["-f", "x.perch", "build", "a"]).1, vec!["run x.perch build [\"a\"]"]);
        assert_eq!(dispatch(&["--help"]).1, vec!["list commands.perch"]);
        assert_eq!(dispatch(&["--check", "-f", "f"]).1, vec!["validate f"]);
        assert_eq!(dispatch(&["--shell"]).1, vec!["shell commands.perch"]);
        assert_eq!(dispatch(&["build", "--help"]).1, vec!["cmdhelp commands.perch build"]);
        assert_eq!(dispatch(&["test", "--filter", "q", "-v"]).1, vec!["test commands.perch \"q\" true"]);
        assert_eq!(dispatch(&["help", "--json", "ops"]).1, vec!["help \"ops\" true"]);
        assert_eq!(dispatch(&["--import", "a.sh", "-o", "b.perch"]).1, vec!["import a.sh \"b.perch\""]);
        assert_eq!(dispatch(&["--import"]).0, 1);
        assert_eq!(dispatch(&["-f", "x"]).0, 1);
        assert_eq!(dispatch(&["--version"]), (0, vec![]));
        assert_eq!(dispatch(&["build", "--export"]), (1, vec!["export -".to_string()]));
        assert_eq!(dispatch(&["--completions"]).0, 1);
        assert_eq!(dispatch(&["--completions", "nu"]).0, 1);
        assert_eq!(dispatch(&["--completions", "bash"]).0, 0);
        assert_eq!(dispatch(&["simulate", "go", "--sim-os", "linux"]).1, vec!["sim commands.perch \"go\" linux \"\""]);
        assert_eq!(dispatch(&[]), (0, vec![]));
    }

    #[test]
    fn scan_format_flags() {
        assert_eq!(dispatch(&["-f", "t.perch", "--scan"]).1, vec!["scan t.perch text"]);
        assert_eq!(dispatch(&["-f", "t.perch", "--scan", "--json"]).1, vec!["scan t.perch json"]);
        assert_eq!(dispatch(&["-f", "t.perch", "--scan", "--format", "json"]).1, vec!["scan t.perch json"]);
        assert_eq!(dispatch(&["--scan", "--format=json"]).1, vec!["scan commands.perch json"]);
        assert_eq!(dispatch(&["--scan", "--format", "text"]).1, vec!["scan commands.perch text"]);
        let (code, log) = dispatch(&["--scan", "--format", "yaml"]);
        assert_eq!((code, log.len()), (2, 0));
        assert_eq!(dispatch(&["--scan", "--format"]).0, 2);
    }

    #[test]
    fn shebang_default_lists_when_no_main() {
        let dir = std::env::temp_dir().join(format!("perch-cli-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.perch");
        std::fs::write(&f, "x").unwrap();
        let fp = f.to_string_lossy().to_string();
        let (code, log) = dispatch(&[&fp]);
        assert_eq!(code, 0);
        assert_eq!(log, vec![format!("list {fp}")]);
        let (_, log) = dispatch(&[&fp, "go", "z"]);
        assert_eq!(log, vec![format!("run {fp} go [\"z\"]")]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
