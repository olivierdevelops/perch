use crate::gofmt;
use crate::{bool_str, OracleSet, SimEnv};
use std::collections::{BTreeMap, HashMap};

/// The *mutable* world the simulator threads through the op walk. Unlike
/// [`SimEnv`] (capability declarations), `SimState` represents what's TRUE NOW
/// — values produced by earlier ops, files the simulator has seen written, etc.
///
/// Each scenario starts with a fresh SimState derived from the fixture's
/// oracles. As ops execute (conceptually), the state evolves:
///
/// ```text
/// write_file "/tmp/x" "data"  → state.files["/tmp/x"] = true
/// let n = shell_output "echo 5" → state.vars["n"] = "5" (from oracle)
/// cd /srv                       → state.cwd = "/srv"
/// rm "/tmp/x"                   → state.files["/tmp/x"] = false
/// ```
///
/// Block ops snapshot the state, simulate the body with the snapshot, then
/// either commit (for sequential blocks like `if` taken-branch) or merge (for
/// branching blocks like `if not taken`, where the not-taken state shouldn't
/// bleed).
#[derive(Debug, Clone, Default)]
pub struct SimState {
    /// The symbolic bindings — args from the command-line shape, globals, and
    /// `let X = ...` captures resolved against oracles. Stringly-typed (matches
    /// the runtime's bindings).
    pub vars: BTreeMap<String, String>,

    /// The active host-env layer (host env + with_env overlays). Separate from
    /// `vars` so capability checks (env_restrict) can apply to env-var
    /// interpolation without affecting `let` bindings.
    pub env: BTreeMap<String, String>,

    /// The simulated current working directory.
    pub cwd: String,

    /// Tracks file existence as the simulator has observed it. Starts populated
    /// from the fixture's file_exists oracle; ops mutate it (write_file / touch
    /// → true, rm → false).
    pub files: HashMap<String, bool>,

    /// Which vars were assigned by `let` from an op whose result the simulator
    /// couldn't statically resolve. Downstream uses of `${name}` for these get
    /// a MIGHT_FAIL reason ("value depends on a runtime call that wasn't
    /// oracled").
    pub unknown: HashMap<String, bool>,

    /// The current scenario's effective oracle set. Read-only during the walk.
    pub oracles: OracleSet,
}

impl SimState {
    /// Builds a fresh state for a scenario. Initial files comes from the
    /// file_exists oracle; vars/env are seeded with the SimEnv's env map; cwd
    /// defaults to `/`.
    pub fn new(env: &SimEnv, oracles: &OracleSet) -> SimState {
        let mut st = SimState {
            cwd: "/".into(),
            oracles: oracles.clone(),
            ..Default::default()
        };
        if let Some(e) = &env.env {
            st.env = e.clone();
        }
        for (k, v) in &oracles.file_exists {
            st.files.insert(k.clone(), *v);
        }
        // Auto-bound vars the simulator can resolve from the SimEnv.
        if !env.os.is_empty() {
            st.vars.insert("os".into(), env.os.clone());
            st.vars.insert("is_windows".into(), bool_str(env.os == "windows").into());
            st.vars.insert("is_macos".into(), bool_str(env.os == "darwin").into());
            st.vars.insert("is_linux".into(), bool_str(env.os == "linux").into());
            st.vars.insert("is_unix".into(), bool_str(env.os != "windows").into());
        }
        if !env.arch.is_empty() {
            st.vars.insert("arch".into(), env.arch.clone());
            st.vars.insert("is_arm64".into(), bool_str(env.arch == "arm64").into());
            st.vars.insert("is_amd64".into(), bool_str(env.arch == "amd64").into());
        }
        st
    }

    /// Returns a deep-enough copy for branching simulation (`if` taken vs
    /// not-taken, parallel branches). Maps copied; oracles are read-only.
    pub fn snapshot(&self) -> SimState {
        self.clone()
    }

    /// Interpolates `${name}` placeholders in `input` against the state's vars
    /// and env. Returns the resolved string and a bool saying whether every
    /// reference resolved (false: at least one `${name}` stayed a placeholder
    /// because the value was unknown).
    pub fn substitute(&self, input: &str) -> (String, bool) {
        if !input.contains("${") {
            return (input.to_string(), true);
        }
        let b = input.as_bytes();
        let mut out = String::with_capacity(input.len());
        let mut all_resolved = true;
        let mut i = 0;
        while i < b.len() {
            if i + 1 < b.len() && b[i] == b'$' && b[i + 1] == b'{' {
                let Some(end) = input[i + 2..].find('}') else {
                    out.push_str(&input[i..]);
                    return (out, false);
                };
                let name = &input[i + 2..i + 2 + end];
                let placeholder = &input[i..i + 2 + end + 1];
                if let Some(v) = self.vars.get(name) {
                    if self.unknown.contains_key(name) {
                        out.push_str(placeholder);
                        all_resolved = false;
                    } else {
                        out.push_str(v);
                    }
                } else if let Some(v) = self.env.get(name) {
                    out.push_str(v);
                } else {
                    out.push_str(placeholder);
                    all_resolved = false;
                }
                i += 2 + end + 1;
                continue;
            }
            // Copy one whole char (multi-byte safe).
            let ch = input[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
        (out, all_resolved)
    }

    /// Consults the simulator's current view of the file system. Returns
    /// `(exists, known)` — known=false means we have no information about this
    /// path.
    pub fn file_exists(&self, path: &str) -> (bool, bool) {
        // Resolve relative path against current cwd.
        let abs = if path.starts_with('/') { path.to_string() } else { gofmt::join(&self.cwd, path) };
        if let Some(v) = self.files.get(&abs) {
            return (*v, true);
        }
        if let Some(v) = self.files.get(path) {
            return (*v, true);
        }
        (false, false)
    }

    /// Records that `path` does (or doesn't) exist after this step. Used by
    /// write_file (true), touch (true), rm (false), etc.
    pub fn mark_file(&mut self, path: &str, exists: bool) {
        let abs = if path.starts_with('/') { path.to_string() } else { gofmt::join(&self.cwd, path) };
        self.files.insert(abs, exists);
    }

    /// Binds `name` → `value`. If unknown, also flag it so downstream
    /// interpolation knows the value is a placeholder.
    pub fn set_var(&mut self, name: &str, value: &str, unknown: bool) {
        self.vars.insert(name.to_string(), value.to_string());
        if unknown {
            self.unknown.insert(name.to_string(), true);
        } else {
            self.unknown.remove(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> SimState {
        let env = SimEnv { os: "linux".into(), env: Some([("HOME".to_string(), "/h".to_string())].into()), ..Default::default() };
        SimState::new(&env, &OracleSet::default())
    }

    #[test]
    fn autobound_and_substitute() {
        let mut s = st();
        assert_eq!(s.vars["is_linux"], "true");
        assert!(!s.vars.contains_key("arch"));
        s.set_var("x", "1", false);
        s.set_var("u", "${u}", true);
        assert_eq!(s.substitute("é${x}-${HOME}/é"), ("é1-/h/é".to_string(), true));
        assert_eq!(s.substitute("${u} ${nope} ${x"), ("${u} ${nope} ${x".to_string(), false));
        s.set_var("u", "z", false);
        assert_eq!(s.substitute("${u}"), ("z".to_string(), true));
    }

    #[test]
    fn files_and_cwd() {
        let mut s = st();
        s.cwd = "/srv".into();
        s.mark_file("a/../b", true);
        assert_eq!(s.file_exists("b"), (true, true));
        assert_eq!(s.file_exists("/srv/b"), (true, true));
        s.mark_file("/srv/b", false);
        assert_eq!(s.file_exists("b"), (false, true));
        assert_eq!(s.file_exists("zz"), (false, false));
        let snap = s.snapshot();
        s.mark_file("q", true);
        assert_eq!(snap.file_exists("q"), (false, false));
    }
}
