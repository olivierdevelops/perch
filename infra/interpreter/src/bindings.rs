use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// The runtime state for one command invocation. Globals, command-args, and
/// `let` captures all live in `vars`.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    pub cwd: String,
    pub env: HashMap<String, String>,
    pub vars: HashMap<String, Value>,
    /// When `Some`, restricts which host env vars resolve via `${NAME}`
    /// fallthrough. `None` = legacy behavior (every host env var visible). An
    /// empty `Some` = no host env vars visible at all. Populated via
    /// `perch --env A,B,C`. Auto-bound names (home, cache_dir, …) are NOT env
    /// vars and are unaffected.
    pub env_allowlist: Option<HashMap<String, bool>>,
    /// The active capability mask. A `sandbox no_shell …` block pushes a
    /// narrower mask onto a stack; on exit the prior mask is restored. Op
    /// handlers (shell, http_*, write_*) consult it before doing work. `None`
    /// means "no in-language gate" (the process-level CLI flags still apply,
    /// enforced inside the handlers).
    ///
    /// The intersection rule is enforced on push: an inner block may disable
    /// capabilities, never re-enable them.
    pub cap_mask: Option<Arc<CapMask>>,
    /// True while a `hooks` handler command is running. The op dispatcher
    /// checks it to prevent re-entrancy — a `before write` hook that itself
    /// writes must not recursively fire the write hook. Set on the child
    /// bindings the hook handler runs under; the parent keeps it false.
    pub in_hook: bool,
}

impl Bindings {
    /// A `Bindings` with empty maps.
    pub fn new(cwd: &str) -> Bindings {
        Bindings { cwd: cwd.to_string(), ..Default::default() }
    }

    /// A shallow child binding for running a hook handler: it shares cwd / env /
    /// capability state and the same variable scope (so the handler can read
    /// current vars and the injected hook.* context) but is flagged `in_hook`
    /// so the dispatcher won't recursively fire hooks.
    pub fn child_for_hook(&self) -> Bindings {
        Bindings {
            cwd: self.cwd.clone(),
            env: self.env.clone(),
            vars: self.vars.clone(),
            env_allowlist: self.env_allowlist.clone(),
            cap_mask: self.cap_mask.clone(),
            in_hook: true,
        }
    }

    /// The string form of a binding's value (suitable for substitution into op
    /// args). Resolution order: command bindings (args / globals / lets),
    /// per-command env, then the host process env (so ${HOME}, ${USER},
    /// ${PATH} etc. work out of the box).
    ///
    /// When `env_allowlist` is set, host-env fallthrough is restricted to the
    /// listed names. This implements `--env`.
    pub fn lookup(&self, name: &str) -> Option<String> {
        if let Some(v) = self.vars.get(name) {
            return Some(to_string_value(v));
        }
        if let Some(v) = self.env.get(name) {
            return Some(v.clone());
        }
        if let Some(allow) = &self.env_allowlist {
            if !allow.get(name).copied().unwrap_or(false) {
                // Allowlist is active and this name isn't on it. Do not fall
                // through to the host env, even if the host has the value.
                return None;
            }
        }
        std::env::var_os(name).map(|v| v.to_string_lossy().into_owned())
    }

    /// Whether the env allowlist is active. Lets callers produce a more
    /// helpful error ("env var X is not in --env allowlist") instead of the
    /// generic "unknown placeholder."
    pub fn env_restricted(&self) -> bool {
        self.env_allowlist.is_some()
    }

    /// Stores a binding.
    pub fn set(&mut self, name: &str, v: impl Into<Value>) {
        self.vars.insert(name.to_string(), v.into());
    }
}

/// One layer of in-language capability restriction. A `None` mask means
/// "permit everything (the CLI flags are the only gate)". A `Some` mask
/// carries explicit deny flags + narrowed allowlists.
///
/// Lookup goes through the `any_*` functions, which walk the chain via
/// `parent`; they take `Option<&CapMask>` so a nil mask is a valid receiver as
/// in Go: `CapMask::any_no_shell(b.cap_mask.as_deref())`.
#[derive(Debug, Clone, Default)]
pub struct CapMask {
    pub no_shell: bool,
    pub no_subprocess: bool,
    pub no_network: bool,
    pub no_write: bool,
    /// When `Some`, restricts shell to argv[0] in this set. `None` = no
    /// in-language narrowing (CLI --allow-bin still applies).
    pub allowed_bins: Option<HashMap<String, bool>>,
    /// Narrows the network host allowlist further.
    pub allowed_hosts: Vec<String>,
    /// Narrows the env-var allowlist further. `None` = inherit; empty `Some`
    /// = block all host envs from this layer down.
    pub env_allow: Option<HashMap<String, bool>>,
    /// When non-empty, restricts write ops to paths under these roots. Each is
    /// the absolute path of a directory the inner block may write to; anything
    /// outside errors.
    pub read_only_roots: Vec<String>,
    /// The next mask outward. Lookups walk the chain.
    pub parent: Option<Arc<CapMask>>,
}

impl CapMask {
    /// A new mask whose state is the intersection of `next` and the current
    /// chain. Because capabilities can only be narrowed, the result carries
    /// every restriction from both layers — never widens.
    pub fn push(parent: Option<&Arc<CapMask>>, mut next: CapMask) -> Arc<CapMask> {
        next.parent = parent.cloned();
        Arc::new(next)
    }

    fn chain(m: Option<&CapMask>) -> impl Iterator<Item = &CapMask> {
        std::iter::successors(m, |c| c.parent.as_deref())
    }

    /// Whether ANY mask in the chain forbids shell.
    pub fn any_no_shell(m: Option<&CapMask>) -> bool {
        Self::chain(m).any(|c| c.no_shell)
    }

    /// The same for subprocess ops.
    pub fn any_no_subprocess(m: Option<&CapMask>) -> bool {
        Self::chain(m).any(|c| c.no_subprocess)
    }

    /// The same for network ops.
    pub fn any_no_network(m: Option<&CapMask>) -> bool {
        Self::chain(m).any(|c| c.no_network)
    }

    /// The same for FS-write ops.
    pub fn any_no_write(m: Option<&CapMask>) -> bool {
        Self::chain(m).any(|c| c.no_write)
    }

    /// Whether `bin` may be invoked under the current mask chain. A `Some`
    /// `allowed_bins` anywhere in the chain restricts to that set (intersected
    /// across layers).
    pub fn allowed_bin_permitted(m: Option<&CapMask>, bin: &str) -> bool {
        Self::chain(m).all(|c| match &c.allowed_bins {
            Some(set) => set.get(bin).copied().unwrap_or(false),
            None => true,
        })
    }
}

/// Converts a value to its string representation for use in interpolation and
/// shell environments.
pub fn to_string_value(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else {
                let x = n.as_f64().unwrap_or(0.0);
                fmt_float(x)
            }
        }
        Value::Array(_) | Value::Object(_) => go_v(v),
    }
}

/// Go's float rendering here: whole values print as integers, otherwise the
/// shortest 'f' form.
fn fmt_float(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 9.2e18 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}

/// Go's `%v` for a decoded JSON value: `[a b]`, `map[k:v]` (sorted keys),
/// `<nil>` inside containers.
fn go_v(v: &Value) -> String {
    match v {
        Value::Null => "<nil>".to_string(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(_) => to_string_value(v),
        Value::Array(a) => {
            let parts: Vec<String> = a.iter().map(go_v).collect();
            format!("[{}]", parts.join(" "))
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys.iter().map(|k| format!("{}:{}", k, go_v(&m[*k]))).collect();
            format!("map[{}]", parts.join(" "))
        }
    }
}
