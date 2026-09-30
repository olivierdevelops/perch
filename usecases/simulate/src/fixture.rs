use crate::gofmt;
use crate::SimEnv;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Reads + parses a JSON fixture file describing capabilities, oracles, and
/// named scenarios.
pub fn load_fixture(path: &str) -> Result<Fixture, crate::Error> {
    let b = std::fs::read(path).map_err(|e| -> crate::Error { gofmt::path_err("open", path, &e).into() })?;
    serde_json::from_slice::<Fixture>(&b).map_err(|e| -> crate::Error { format!("parsing fixture JSON: {e}").into() })
}

/// The JSON-loadable companion to [`SimEnv`]: the same capability declarations
/// PLUS oracles (simulated outputs for ops the static walk can't resolve) PLUS
/// named scenarios (override sets that branch the simulation).
///
/// File shape:
///
/// ```json
/// {
///   "os": "linux", "arch": "amd64",
///   "env": {"HOME": "/h", "PATH": "/usr/bin"}, "env_only": true,
///   "fs_read":  ["/srv"], "fs_write": ["/tmp"],
///   "bins":     ["docker", "kubectl"],
///   "network":  ["api.github.com"],
///   "no_shell": false, "no_network": false,
///
///   "oracles": {
///     "file_exists":  {"./manifest.yaml": true, "/etc/passwd": false},
///     "shell_output": {"git rev-parse HEAD": "1f1db7b"},
///     "http":         {"https://api.github.com/health": {"status": 200, "body": "OK"}},
///     "has_bin":      {"docker": true, "kubectl": true}
///   },
///
///   "scenarios": [
///     {"name": "happy",      "overrides": {}},
///     {"name": "github-down","overrides": {
///       "http": {"https://api.github.com/health": {"status": 500}}
///     }}
///   ]
/// }
/// ```
///
/// If `scenarios` is absent or empty, the fixture is treated as ONE implicit
/// scenario named "default" using the top-level oracles.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fixture {
    // Capability declarations (same shape as SimEnv, JSON-tagged).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub os: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub arch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub env_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fs_read: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fs_write: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bins: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<Vec<String>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_shell: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_subprocess: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_network: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_write: bool,

    /// Concrete simulated outputs for ops the static walk can't statically
    /// resolve. Each is keyed by the op's primary argument after `${}`
    /// substitution against the current state.
    #[serde(skip_serializing_if = "OracleSet::is_empty")]
    pub oracles: OracleSet,

    /// Lets one fixture file describe multiple what-ifs. Each scenario inherits
    /// the top-level oracles and overrides any keys it specifies. If empty, one
    /// implicit "default" scenario runs with the top-level oracles.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scenarios: Vec<Scenario_>,
}

/// Concrete simulated outputs for ops whose result would otherwise be "unknown."
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OracleSet {
    /// Path → present? Used by `if exists "PATH"` and by `write_file "PATH"` to
    /// decide whether the op's stateful effect was already in place. Paths
    /// recorded by the simulator as written by an earlier op shadow this.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub file_exists: HashMap<String, bool>,

    /// The (post-interpolation) shell command → its simulated stdout. Used by
    /// `let X = shell_output "Y"` so downstream ${X} resolves to the simulated
    /// value.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub shell_output: HashMap<String, String>,

    /// URL → simulated response. Used by `http_get`, `http_post`, etc. Lets you
    /// set status codes and bodies per URL.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub http: HashMap<String, HTTPResponse>,

    /// Overrides the `has_bin "X"` predicate's result. Useful when you want to
    /// simulate "what if kubectl is missing?" without removing kubectl from the
    /// Bins capability allowlist.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub has_bin: HashMap<String, bool>,
}

impl OracleSet {
    fn is_empty(&self) -> bool {
        self.file_exists.is_empty() && self.shell_output.is_empty() && self.http.is_empty() && self.has_bin.is_empty()
    }
}

/// The simulated outcome of an http_* op.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HTTPResponse {
    /// Default 200 if zero.
    #[serde(skip_serializing_if = "is_zero")]
    pub status: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub body: String,
    /// When non-empty, simulates the server returning a 3xx pointing here.
    /// Useful for "what if api.github.com redirects to evil.com?" scenarios.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub redirect: String,
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

/// One named override set. (Underscore suffix because `Scenario` already exists
/// in the simulator for per-op alternatives. Distinct concept, distinct name.)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scenario_ {
    pub name: String,
    #[serde(skip_serializing_if = "OracleSet::is_empty")]
    pub overrides: OracleSet,
    /// Lets a scenario tweak env vars too — e.g. "what if GITHUB_TOKEN isn't
    /// set in this scenario?"
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

impl Fixture {
    /// Lifts the capability declarations from a Fixture into the [`SimEnv`]
    /// shape the simulator already uses.
    pub fn to_sim_env(&self) -> SimEnv {
        let bins: BTreeMap<String, bool> = self.bins.iter().map(|b| (b.clone(), true)).collect();
        SimEnv {
            os: self.os.clone(),
            arch: self.arch.clone(),
            env: self.env.clone(),
            env_restrict: self.env_only,
            fs_read: self.fs_read.clone(),
            fs_write: self.fs_write.clone(),
            bins: if bins.is_empty() { None } else { Some(bins) },
            network: self.network.clone(),
            no_shell: self.no_shell,
            no_subprocess: self.no_subprocess,
            no_network: self.no_network,
            no_write: self.no_write,
        }
    }
}

/// Produces an [`OracleSet`] that is the base oracles overlaid with overrides —
/// used to build a scenario's effective oracles from the fixture's defaults +
/// the scenario's tweaks.
pub fn merge_oracles(base: &OracleSet, over: &OracleSet) -> OracleSet {
    fn merge<V: Clone>(a: &HashMap<String, V>, b: &HashMap<String, V>) -> HashMap<String, V> {
        let mut out = a.clone();
        out.extend(b.iter().map(|(k, v)| (k.clone(), v.clone())));
        out
    }
    OracleSet {
        file_exists: merge(&base.file_exists, &over.file_exists),
        shell_output: merge(&base.shell_output, &over.shell_output),
        http: merge(&base.http, &over.http),
        has_bin: merge(&base.has_bin, &over.has_bin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docs_example() {
        let f: Fixture = serde_json::from_str(include_str!("../../../docs/simulate-example.json")).unwrap();
        assert!(!f.scenarios.is_empty() || !f.oracles.http.is_empty() || !f.oracles.file_exists.is_empty());
    }

    #[test]
    fn to_sim_env_and_merge() {
        let f: Fixture = serde_json::from_str(r#"{"os":"linux","bins":["a","a"],"env_only":true,"env":{"K":"v"},"network":[]}"#).unwrap();
        let e = f.to_sim_env();
        assert_eq!(e.os, "linux");
        assert_eq!(e.bins.as_ref().unwrap().len(), 1);
        assert_eq!(e.network, Some(vec![]));
        assert!(e.fs_read.is_none() && e.env_restrict);
        let mut a = OracleSet::default();
        a.has_bin.insert("x".into(), true);
        a.has_bin.insert("y".into(), true);
        let mut b = OracleSet::default();
        b.has_bin.insert("y".into(), false);
        let m = merge_oracles(&a, &b);
        assert!(m.has_bin["x"]);
        assert!(!m.has_bin["y"]);
    }

    #[test]
    fn load_errors_match_go_shape() {
        let e = load_fixture("/nonexistent/f.json").unwrap_err().to_string();
        assert_eq!(e, "open /nonexistent/f.json: no such file or directory");
    }
}
