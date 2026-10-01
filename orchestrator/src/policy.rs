//! [`Policy`]: the plain-data description of what a [`crate::Runtime`] may do.
//!
//! A `Policy` is just data (clone it, compare it, build it in one expression).
//! The same type feeds both the library `Runtime` and the CLI wiring, so the
//! two cannot drift: [`Policy::handlers`] and [`Policy::configure`] are the
//! single place where a policy is turned into interpreter state.
use perch_interpreter::{BeforeOp, HTTPPolicy, Handler, Interpreter};
use perch_ops::Restrictions;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::time::Duration;

/// Where a runtime's stdout or stderr goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Output {
    /// Collect into a buffer returned in [`crate::RunResult`] (the default).
    #[default]
    Capture,
    /// Write to the host process's own stream. The result's field stays empty.
    Inherit,
    /// Throw the output away.
    Discard,
}

/// Where a runtime's stdin comes from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Input {
    /// Immediately at end-of-file (the default).
    #[default]
    Empty,
    /// The host process's stdin.
    Inherit,
    /// These bytes, then end-of-file.
    Bytes(Vec<u8>),
}

/// Limits for HTTP ops. The default is the secure CLI default: at most 5
/// redirects, no private/loopback addresses, no https to http downgrade, any
/// public host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpPolicy {
    /// Redirect chain cap. `0` refuses every redirect.
    pub max_redirects: i64,
    /// Disables the SSRF guard (private, loopback, link-local targets).
    pub allow_private_ips: bool,
    /// Permits https to http redirects.
    pub allow_scheme_downgrade: bool,
    /// When non-empty, every request and redirect target must match one entry
    /// (`host`, `*.host`, `host:port`, IP literal).
    pub allowed_hosts: Vec<String>,
}

impl Default for HttpPolicy {
    fn default() -> Self {
        HttpPolicy { max_redirects: 5, allow_private_ips: false, allow_scheme_downgrade: false, allowed_hosts: vec![] }
    }
}

impl From<HttpPolicy> for HTTPPolicy {
    fn from(p: HttpPolicy) -> Self {
        HTTPPolicy {
            max_redirects: p.max_redirects,
            allow_private_ips: p.allow_private_ips,
            allow_scheme_downgrade: p.allow_scheme_downgrade,
            allowed_hosts: p.allowed_hosts,
        }
    }
}

impl From<HTTPPolicy> for HttpPolicy {
    fn from(p: HTTPPolicy) -> Self {
        HttpPolicy {
            max_redirects: p.max_redirects,
            allow_private_ips: p.allow_private_ips,
            allow_scheme_downgrade: p.allow_scheme_downgrade,
            allowed_hosts: p.allowed_hosts,
        }
    }
}

/// What a runtime is allowed to do. `Policy::default()` is the CLI's default
/// posture: no capability restrictions, the whole environment visible, any
/// shell binary, secure HTTP defaults, no deadline; output captured.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Policy {
    /// Deny every `shell` op (`cap_shell_denied`).
    pub no_shell: bool,
    /// Deny every subprocess-spawning op (`cap_subprocess_denied`).
    pub no_subprocess: bool,
    /// Deny every network op (`cap_network_denied`).
    pub no_network: bool,
    /// Deny every filesystem-mutating op (`cap_write_denied`).
    pub no_write: bool,
    /// `Some(names)`: only these host environment variables are visible
    /// (`Some(empty)` hides all). `None`: the whole environment.
    pub env_allow: Option<BTreeSet<String>>,
    /// `Some(names)`: `shell` may only run these binaries. `None`: any.
    pub allowed_bins: Option<BTreeSet<String>>,
    /// Refuse shell strings containing metacharacters (pipes, `;`, `&&`, ...).
    pub no_shell_metachars: bool,
    /// HTTP limits. `None` uses the interpreter's secure defaults.
    pub http: Option<HttpPolicy>,
    /// Wall-clock budget for one [`crate::Runtime::run`]. `None`: unlimited.
    pub max_runtime: Option<Duration>,
    /// Let advisory (non-enforced) confinement scopes run instead of failing
    /// closed.
    pub allow_advisory_scopes: bool,
    /// Directory relative paths and subprocesses start in. `None`: the host
    /// process's cwd. Applied per run; the process cwd is never changed.
    pub working_dir: Option<PathBuf>,
    /// stdout destination. Default [`Output::Capture`].
    pub stdout: Output,
    /// stderr destination. Default [`Output::Capture`].
    pub stderr: Output,
    /// stdin source. Default [`Input::Empty`].
    pub stdin: Input,
}

fn names<I, S>(it: I) -> BTreeSet<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    it.into_iter().map(Into::into).collect()
}

impl Policy {
    /// Same as `Policy::default()`.
    pub fn new() -> Policy {
        Policy::default()
    }
    /// Sets [`Policy::no_shell`].
    pub fn no_shell(mut self, on: bool) -> Self {
        self.no_shell = on;
        self
    }
    /// Sets [`Policy::no_subprocess`].
    pub fn no_subprocess(mut self, on: bool) -> Self {
        self.no_subprocess = on;
        self
    }
    /// Sets [`Policy::no_network`].
    pub fn no_network(mut self, on: bool) -> Self {
        self.no_network = on;
        self
    }
    /// Sets [`Policy::no_write`].
    pub fn no_write(mut self, on: bool) -> Self {
        self.no_write = on;
        self
    }
    /// Denies all four capabilities at once.
    pub fn deny_all(self) -> Self {
        self.no_shell(true).no_subprocess(true).no_network(true).no_write(true)
    }
    /// Restricts the visible host environment to `vars` (empty hides it all).
    pub fn env_allow<I, S>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.env_allow = Some(names(vars));
        self
    }
    /// Restricts `shell` to the given binaries.
    pub fn allowed_bins<I, S>(mut self, bins: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_bins = Some(names(bins));
        self
    }
    /// Sets [`Policy::no_shell_metachars`].
    pub fn no_shell_metachars(mut self, on: bool) -> Self {
        self.no_shell_metachars = on;
        self
    }
    /// Sets the HTTP policy.
    pub fn http(mut self, p: HttpPolicy) -> Self {
        self.http = Some(p);
        self
    }
    /// Sets the per-run wall-clock budget.
    pub fn max_runtime(mut self, d: Duration) -> Self {
        self.max_runtime = Some(d);
        self
    }
    /// Sets [`Policy::allow_advisory_scopes`].
    pub fn allow_advisory_scopes(mut self, on: bool) -> Self {
        self.allow_advisory_scopes = on;
        self
    }
    /// Sets the starting directory.
    pub fn working_dir(mut self, d: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(d.into());
        self
    }
    /// Sets the stdout destination.
    pub fn stdout(mut self, o: Output) -> Self {
        self.stdout = o;
        self
    }
    /// Sets the stderr destination.
    pub fn stderr(mut self, o: Output) -> Self {
        self.stderr = o;
        self
    }
    /// Sets the stdin source.
    pub fn stdin(mut self, i: Input) -> Self {
        self.stdin = i;
        self
    }

    /// The capability switches as the ops crate's [`Restrictions`].
    pub fn restrictions(&self) -> Restrictions {
        Restrictions {
            no_shell: self.no_shell,
            no_subprocess: self.no_subprocess,
            no_network: self.no_network,
            no_write: self.no_write,
        }
    }

    /// The op handler registry with this policy's restrictions applied.
    pub fn handlers(&self) -> HashMap<String, Handler> {
        let mut handlers = perch_ops::all_handlers();
        perch_ops::apply_restrictions(&mut handlers, &self.restrictions());
        perch_ops::apply_mask_gating(&mut handlers);
        handlers
    }

    /// Copies this policy onto a fresh interpreter: preflight and category
    /// hooks, env/bin allowlists, HTTP policy, advisory scopes and the
    /// capability flags. IO sinks and the deadline are the caller's concern.
    pub fn configure(&self, i: &mut Interpreter, hook: &Option<BeforeOp>) {
        let to_map = |s: &Option<BTreeSet<String>>| s.as_ref().map(|s| s.iter().map(|k| (k.clone(), true)).collect());
        i.preflight_hook = Some(std::sync::Arc::new(perch_ops::preflight));
        i.hook_category = Some(std::sync::Arc::new(|k: &str| perch_ops::hook_category_of(k)));
        i.set_before_op(hook.clone());
        i.env_allowlist = to_map(&self.env_allow);
        i.allowed_shell_bins = to_map(&self.allowed_bins);
        i.no_shell_metachars = self.no_shell_metachars;
        i.http_policy = self.http.clone().map(Into::into);
        i.allow_advisory_scopes = self.allow_advisory_scopes;
        i.restrict_no_write = self.no_write;
        i.restrict_no_network = self.no_network;
        i.working_dir = self.working_dir.as_ref().map(|d| d.to_string_lossy().into_owned());
    }
}
