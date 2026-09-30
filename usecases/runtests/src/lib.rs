//! Discovers commands marked with the `test` modifier, runs each in a
//! sandboxed environment, and reports pass/fail.
//!
//! A test is a regular perch command — same ops, same templates, same
//! execution contexts — that has the `test` modifier set. It passes unless any
//! op returns an error (including the `fail "msg"` op and every `assert_*` op).
//!
//! Sandboxing defaults (each may be opted out per-test via modifiers):
//!
//!   - cwd is set to a fresh ${TMPDIR}/perch-test-${name}-XXXXX/
//!   - --no-network is on
//!   - --no-shell is on (most tests should not shell out)
//!   - --no-subprocess is on
//!   - writes are restricted to the temp cwd (via a sandbox CapMask)
//!   - the test gets its own --max-runtime of 30s (or test_timeout N)
//!
//! Each test runs with a fresh interpreter so state doesn't leak between
//! tests. The runner aggregates results and exits non-zero if any test failed.
use perch_domain::{Command, Program};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Parses a .perch file into a Program. Same shape as the other use cases use
/// (deliberately matches runcommand's `LoadFn` so the orchestrator can pass the
/// loader directly).
pub type LoadFn = Box<dyn Fn(&str) -> Result<Program, Error>>;

/// Runs a single test command against the supplied program with the supplied
/// bindings overrides (cwd, capability flags, timeout). Returns the interpreter
/// error (Ok = pass). The orchestrator wires this against the op handlers + the
/// runtime sandboxing the test runner asks for. The last argument receives the
/// test's captured output.
pub type RunTestFn = Box<dyn Fn(&Program, &str, &TestSandbox, &mut Vec<u8>) -> Result<(), Error>>;

/// The per-test environment a test should run in. Populated by the runner from
/// the test's modifiers + the runner's defaults; consumed by the
/// orchestrator-supplied [`RunTestFn`] when it builds an interpreter for the
/// test.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestSandbox {
    /// Absolute path; usually a temp dir.
    pub cwd: String,
    /// Default true; opt out via test_allow_shell.
    pub no_shell: bool,
    /// Default true; opt out via test_allow_network.
    pub no_network: bool,
    /// Default true; opt out via test_allow_subprocess.
    pub no_subprocess: bool,
    /// Default false; on if the test runs in temp cwd (writes still go through
    /// the inner sandbox).
    pub no_write: bool,
    /// Wall-clock cap; zero = use runner default.
    pub timeout: Duration,
}

pub struct Impl {
    pub load: LoadFn,
    pub run_test: RunTestFn,
    /// Caps each test when the test itself doesn't declare `test_timeout N`.
    /// The CLI flag `--test-timeout=N` populates this before calling execute.
    pub default_timeout: Duration,
    /// When true, leaves the per-test temp cwd intact so the user can inspect
    /// it after a failure. Set by `--keep-tempdir`.
    pub keep_temp_dir: bool,
}

struct TestResult {
    name: String,
    pass: bool,
    err: Option<Error>,
    dur: Duration,
    output: String,
    temp_dir: String,
}

impl Impl {
    /// Discovers every `test`-marked command, runs them in order, and writes a
    /// summary to `stderr` (Go: os.Stderr). Returns an error if any test failed.
    pub fn execute(&self, config_path: &str, filter: &str, verbose: bool, stderr: &mut dyn Write) -> Result<(), Error> {
        let p = (self.load)(config_path).map_err(|e| -> Error { format!("loading {config_path}: {e}").into() })?;

        let names = discover(&p, filter);
        if names.is_empty() {
            if !filter.is_empty() {
                writeln!(stderr, "no tests matching {}", quote(filter))?;
            } else {
                writeln!(stderr, "no tests declared. Add `test` to any command's modifiers to mark it as a test.")?;
            }
            return Ok(());
        }

        writeln!(stderr, "── perch test ─────────────────────────────────")?;
        let default_timeout = if self.default_timeout.is_zero() { Duration::from_secs(30) } else { self.default_timeout };

        let mut results: Vec<TestResult> = Vec::with_capacity(names.len());
        for name in &names {
            let cmd = &p.commands[name];
            let (sb, tmp) = self.build_sandbox(cmd, default_timeout);
            let mut buf: Vec<u8> = Vec::new();
            let start = Instant::now();
            let res = (self.run_test)(&p, name, &sb, &mut buf);
            let dur = start.elapsed();
            let r = TestResult {
                name: name.clone(),
                pass: res.is_ok(),
                err: res.err(),
                dur,
                output: String::from_utf8_lossy(&buf).into_owned(),
                temp_dir: tmp.clone(),
            };
            print_one(stderr, &r, verbose)?;
            results.push(r);
            // Clean up the temp dir unless the user wants to inspect it.
            if !tmp.is_empty() && !self.keep_temp_dir {
                let _ = std::fs::remove_dir_all(&tmp);
            }
        }

        let failed = results.iter().filter(|r| !r.pass).count();
        let passed = results.len() - failed;
        let total_dur: Duration = results.iter().map(|r| r.dur).sum();

        writeln!(stderr)?;
        if failed == 0 {
            writeln!(stderr, "{} passed, 0 failed in {}.", passed, format_dur(total_dur))?;
            return Ok(());
        }
        writeln!(stderr, "{} passed, {} failed in {}.", passed, failed, format_dur(total_dur))?;
        // Echo failed test details at the end so they're easy to scroll back to.
        writeln!(stderr)?;
        writeln!(stderr, "Failures:")?;
        for r in results.iter().filter(|r| !r.pass) {
            writeln!(stderr, "  ✗ {}", r.name)?;
            writeln!(stderr, "    {}", r.err.as_ref().map(|e| e.to_string()).unwrap_or_default())?;
            if !r.temp_dir.is_empty() && self.keep_temp_dir {
                writeln!(stderr, "    (sandbox kept at {})", r.temp_dir)?;
            }
        }
        Err(format!("{failed} test(s) failed").into())
    }

    /// Materialises the per-test environment from the command's modifiers +
    /// runner defaults. Returns the sandbox plus the temp dir path (empty if
    /// test_keep_cwd is set), so the runner can clean it up.
    fn build_sandbox(&self, cmd: &Command, default_timeout: Duration) -> (TestSandbox, String) {
        let m = &cmd.modifiers;
        let mut sb = TestSandbox {
            no_shell: !m.test_allow_shell,
            no_network: !m.test_allow_network,
            no_subprocess: !m.test_allow_subprocess,
            no_write: false, // writes are allowed but the inner CapMask scopes them to the temp cwd
            timeout: default_timeout,
            ..Default::default()
        };
        if m.test_timeout_secs > 0 {
            sb.timeout = Duration::from_secs(m.test_timeout_secs as u64);
        }
        let mut tmp = String::new();
        if !m.test_keep_cwd {
            if let Ok(base) = make_temp_dir(&format!("perch-test-{}-", sanitize_name(&cmd.name))) {
                tmp = base.clone();
                sb.cwd = base;
            }
        }
        if sb.cwd.is_empty() {
            // Fall back to cwd if MkdirTemp failed or test_keep_cwd was set.
            sb.cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        }
        (sb, tmp)
    }
}

/// Go's `os.MkdirTemp(dir, prefix+"*")`: unique dir under the system temp dir.
fn make_temp_dir(prefix: &str) -> std::io::Result<String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0);
    for _ in 0..1000 {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let suffix = seed.wrapping_mul(6364136223846793005).wrapping_add(n.wrapping_mul(1442695040888963407)) ^ (std::process::id() as u64);
        let path = std::env::temp_dir().join(format!("{prefix}{}", suffix % 10_000_000_000));
        let mut b = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            b.mode(0o700);
        }
        match b.create(&path) {
            Ok(()) => return Ok(path.to_string_lossy().into_owned()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "no unique temp dir"))
}

/// Go's `%q` for the filter string.
fn quote(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\x07' => o.push_str("\\a"),
            '\x08' => o.push_str("\\b"),
            '\x0c' => o.push_str("\\f"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            '\x0b' => o.push_str("\\v"),
            c if is_print(c) => o.push(c),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => o.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x10000 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    o.push('"');
    o
}

/// Go's `strconv.IsPrint`, approximated for non-ASCII.
fn is_print(c: char) -> bool {
    let u = c as u32;
    if u < 0x80 {
        return (0x20..0x7f).contains(&u);
    }
    if c.is_control() || c.is_whitespace() {
        return false;
    }
    !matches!(u,
        0x00ad | 0x0600..=0x0605 | 0x061c | 0x06dd | 0x070f | 0x180e
        | 0x200b..=0x200f | 0x2028..=0x202e | 0x2060..=0x206f
        | 0xe000..=0xf8ff | 0xfeff | 0xfff9..=0xfffb | 0xfffe | 0xffff
        | 0xf0000..=0x10ffff)
}

/// Returns the sorted list of test-marked command names that match the
/// (optional) filter substring.
fn discover(p: &Program, filter: &str) -> Vec<String> {
    // BTreeMap iterates in sorted (byte) order, matching sort.Strings.
    p.commands
        .iter()
        .filter(|(name, cmd)| cmd.modifiers.test && (filter.is_empty() || name.contains(filter)))
        .map(|(name, _)| name.clone())
        .collect()
}

/// Makes a command name safe for use in a temp-dir name. Mostly to handle
/// namespaced names like `aws.upload` which would otherwise become
/// subdirectories.
fn sanitize_name(s: &str) -> String {
    let bytes: Vec<u8> = s
        .bytes()
        .map(|c| if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' { c } else { b'_' })
        .collect();
    // Each non-ASCII byte becomes one `_`, exactly as the Go byte loop does.
    String::from_utf8(bytes).expect("ascii only")
}

/// Writes the one-line outcome for a single test.
fn print_one(w: &mut dyn Write, r: &TestResult, verbose: bool) -> std::io::Result<()> {
    if r.pass {
        writeln!(w, "✓ {:<40} ({})", r.name, format_dur(r.dur))?;
        if verbose && !r.output.is_empty() {
            writeln!(w, "{}", indent(&r.output, "    "))?;
        }
        return Ok(());
    }
    writeln!(w, "✗ {:<40} ({})", r.name, format_dur(r.dur))?;
    writeln!(w, "    {}", r.err.as_ref().map(|e| e.to_string()).unwrap_or_default())?;
    if !r.output.is_empty() {
        writeln!(w, "{}", indent(&r.output, "    "))?;
    }
    Ok(())
}

fn indent(s: &str, pad: &str) -> String {
    s.trim_end_matches('\n').split('\n').map(|l| format!("{pad}{l}")).collect::<Vec<_>>().join("\n")
}

/// Mirrors infra/report's helper for visual consistency.
fn format_dur(d: Duration) -> String {
    if d < Duration::from_millis(1) {
        format!("{}µs", d.as_micros())
    } else if d < Duration::from_secs(1) {
        format!("{}ms", d.as_millis())
    } else if d < Duration::from_secs(60) {
        format!("{:.2}s", d.as_secs_f64())
    } else {
        format!("{}m{:02}s", d.as_secs() / 60, d.as_secs() % 60)
    }
}

/// Exposed so the orchestrator (which knows the source .perch path) can resolve
/// relative test resources. Currently unused by [`Impl`] directly; kept for
/// downstream consumers.
pub fn script_dir(p: &Program) -> String {
    if p.script_path.is_empty() {
        return String::new();
    }
    match Path::new(&p.script_path).parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_string_lossy().into_owned(),
        _ => ".".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perch_domain::Modifiers;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn prog() -> Program {
        let mut p = Program::default();
        let t = |allow_shell: bool, keep: bool, secs: i64| Command {
            modifiers: Modifiers { test: true, test_allow_shell: allow_shell, test_keep_cwd: keep, test_timeout_secs: secs, ..Default::default() },
            ..Default::default()
        };
        p.commands.insert("b.ok".into(), Command { name: "b.ok".into(), ..t(false, false, 0) });
        p.commands.insert("a_fail".into(), Command { name: "a_fail".into(), ..t(true, true, 5) });
        p.commands.insert("plain".into(), Command { name: "plain".into(), ..Default::default() });
        p
    }

    fn imp(seen: Rc<RefCell<Vec<(String, TestSandbox)>>>) -> Impl {
        Impl {
            load: Box::new(|_| Ok(prog())),
            run_test: Box::new(move |_, name, sb, out| {
                seen.borrow_mut().push((name.to_string(), sb.clone()));
                out.extend_from_slice(b"out\n");
                if name == "a_fail" {
                    Err("assert_failed: nope".into())
                } else {
                    Ok(())
                }
            }),
            default_timeout: Duration::ZERO,
            keep_temp_dir: false,
        }
    }

    #[test]
    fn runs_sorted_reports_failures() {
        let seen = Rc::new(RefCell::new(vec![]));
        let mut err = Vec::new();
        let r = imp(seen.clone()).execute("f", "", true, &mut err);
        assert_eq!(r.unwrap_err().to_string(), "1 test(s) failed");
        let s = String::from_utf8(err).unwrap();
        assert!(s.starts_with("── perch test ─────────────────────────────────\n✗ a_fail"));
        assert!(s.contains("    assert_failed: nope\n    out\n✓ b.ok"));
        assert!(s.contains("\n1 passed, 1 failed in "));
        assert!(s.ends_with("Failures:\n  ✗ a_fail\n    assert_failed: nope\n"));
        let seen = seen.borrow();
        assert_eq!(seen[0].0, "a_fail");
        assert_eq!(seen[0].1.timeout, Duration::from_secs(5));
        assert!(!seen[0].1.no_shell);
        assert!(seen[0].1.no_network);
        assert_eq!(seen[1].1.timeout, Duration::from_secs(30));
        assert!(seen[1].1.cwd.contains("perch-test-b_ok-"));
        assert!(!Path::new(&seen[1].1.cwd).exists(), "temp dir cleaned up");
    }

    #[test]
    fn filter_and_empty() {
        let seen = Rc::new(RefCell::new(vec![]));
        let mut err = Vec::new();
        imp(seen.clone()).execute("f", "zzz", false, &mut err).unwrap();
        assert_eq!(String::from_utf8(err).unwrap(), "no tests matching \"zzz\"\n");
        let mut err = Vec::new();
        imp(seen.clone()).execute("f", "ok", false, &mut err).unwrap();
        assert!(String::from_utf8(err).unwrap().contains("1 passed, 0 failed in "));
        assert_eq!(seen.borrow().len(), 1);
    }

    #[test]
    fn helpers() {
        assert_eq!(sanitize_name("aws.upload é"), "aws_upload___");
        assert_eq!(format_dur(Duration::from_micros(5)), "5µs");
        assert_eq!(format_dur(Duration::from_millis(250)), "250ms");
        assert_eq!(format_dur(Duration::from_millis(1500)), "1.50s");
        assert_eq!(format_dur(Duration::from_secs(125)), "2m05s");
        assert_eq!(indent("a\nb\n\n", "  "), "  a\n  b");
    }
}
