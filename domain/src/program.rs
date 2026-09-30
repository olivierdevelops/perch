//! A perch program: named, callable commands compiled from a `.perch` file.
//! Capy parses source into JSON; the loader hydrates it into a [`Program`].
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

fn is_false(b: &bool) -> bool {
    !*b
}
fn is_zero(n: &i64) -> bool {
    *n == 0
}
fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Whole parsed config. One per `.perch` file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Program {
    pub name: String,
    pub description: String,
    pub version: String,
    pub globals: Globals,
    pub commands: BTreeMap<String, Command>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catch: Option<Catch>,
    /// Parse-time stamps (`template NAME ... end`), expanded at load; kept for
    /// `--check` and the LSP.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub templates: BTreeMap<String, Template>,
    /// Absolute path of the source; empty when embedded in a binary.
    #[serde(skip)]
    pub script_path: String,
    /// The file's self-declared manifest; enforcement applies when declared.
    #[serde(skip_serializing_if = "is_default")]
    pub requirements: Requirements,
    /// Files/dirs to embed at `--build` time.
    #[serde(skip_serializing_if = "is_default")]
    pub bundle: Bundle,
    /// File-scope interceptors from a `hooks ... end` block.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<Hook>,
}

/// One `TIMING TARGET HANDLER` line in a `hooks` block — policy and
/// observability at perch's dispatch layer, not an OS sandbox.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hook {
    /// "before" (an error vetoes the op), "after", or "on_error".
    pub timing: String,
    /// Capability category, op KIND, or "any".
    pub target: String,
    /// Command run when the hook fires.
    pub handler: String,
}

/// File tree to embed into the fat binary at `--build` time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bundle {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<BundleAlias>,
}

/// One `include "PATH" as NAME` declaration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BundleAlias {
    pub name: String,
    /// Bundle-relative path under which the file lives after `--build`.
    pub entry: String,
}

/// Parsed `requires ... end` block: what the host machine must provide.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Requirements {
    #[serde(skip_serializing_if = "is_false")]
    pub declared: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bins: Vec<BinReq>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub envs: Vec<EnvReq>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hosts: Vec<HostReq>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub os: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub arch: Vec<String>,
    /// Filesystem scopes the program may read / write. A write root implies
    /// read on the same tree. Matched after interpolation, cleaned, absolute.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub read_roots: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub write_roots: Vec<String>,
}

/// One `bin "NAME" [optional]` line, optionally hash-pinned. There is
/// deliberately no version checking (it would require executing the binary).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BinReq {
    /// Bare command on PATH, or a path to an executable (resolved relative to
    /// the script directory).
    pub name: String,
    /// `bin "PATH" as NAME`: a clean handle resolved to `name` before spawn.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub alias: String,
    #[serde(skip_serializing_if = "is_false")]
    pub optional: bool,
    /// Pin as "sha256:HEXDIGEST"; preflight verifies the resolved binary.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hash: String,
    /// Load the hash from `bundle:PATH` or a script-relative file instead.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub hash_file: String,
}

/// One `env "NAME" [optional]` line.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvReq {
    pub name: String,
    #[serde(skip_serializing_if = "is_false")]
    pub optional: bool,
}

/// One `host "name" [optional]` line (exact match or `*.suffix`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HostReq {
    pub name: String,
    #[serde(skip_serializing_if = "is_false")]
    pub optional: bool,
}

/// Parameterized op-sequence expanded inline at every call site.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Template {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<ArgSpec>,
    pub ops: Vec<Op>,
}

/// Bindings shared by every command invocation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Globals {
    pub bindings: Vec<GlobalBinding>,
}

/// One `NAME = VALUE` line from a globals block.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalBinding {
    pub name: String,
    /// "bool" | "int" | "float" | "string"
    #[serde(rename = "type")]
    pub ty: String,
    pub value: Value,
}

/// One declared, callable unit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Command {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<ArgSpec>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    pub modifiers: Modifiers,
    pub ops: Vec<Op>,
}

/// One typed CLI argument on a command.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArgSpec {
    pub name: String,
    /// "string" | "int" | "float" | "bool"
    #[serde(rename = "type")]
    pub ty: String,
    pub description: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub default: Value,
    #[serde(skip_serializing_if = "is_false")]
    pub has_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<i64>,
    #[serde(skip_serializing_if = "is_false")]
    pub optional: bool,
    /// Consumes every remaining positional argument (newline-joined; a
    /// `${NAME_count}` binding holds the count). Must be last, string, no default.
    #[serde(skip_serializing_if = "is_false")]
    pub rest: bool,
}

/// Flags declared on a command before its `do` block.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Modifiers {
    #[serde(skip_serializing_if = "is_false")]
    pub private: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub detached: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub proxy_args: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub require_os: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub require_arch: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub dir: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub on_signal: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub post_start_delay_secs: i64,

    // Test modifiers: a `test` command is hidden from help, discovered by
    // `perch test`, and run sandboxed unless it opts out below.
    #[serde(skip_serializing_if = "is_false")]
    pub test: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub test_allow_network: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub test_allow_shell: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub test_allow_write: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub test_allow_subprocess: bool,
    /// Run in the file's directory instead of an auto temp cwd.
    #[serde(skip_serializing_if = "is_false")]
    pub test_keep_cwd: bool,
    /// Wall-clock cap; 0 means the global default (30s).
    #[serde(skip_serializing_if = "is_zero")]
    pub test_timeout_secs: i64,
}

/// Optional catch-all handler for unknown command names.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Catch {
    /// Name of the implicit arg holding the unknown name.
    pub bind: String,
    pub description: String,
    pub ops: Vec<Op>,
    /// Binds `${proxy_args}` to the full unknown invocation; off by default
    /// because catch→shell forwarding is a hidden-privilege pattern.
    #[serde(skip_serializing_if = "is_false")]
    pub proxy_args: bool,
}

/// One statement inside a command body (or a block op's body).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Op {
    pub kind: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub line: i64,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub args: Map<String, Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<Op>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub capture_into: String,
    /// Template this op was expanded from, for error messages and traces.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub expanded_from: String,
}


impl Op {
    /// Whether this op kind contains a nested body: control flow (`if`,
    /// `if_call`, `for_each`) or an execution context (`parallel`, `timeout`,
    /// `retry`, `with_env`, `with_cwd`, `sandbox`, `cache`, ...).
    pub fn is_block(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "if" | "if_call"
                | "for_each"
                | "parallel"
                | "timeout"
                | "retry"
                | "with_env"
                | "with_cwd"
                | "sandbox"
                | "cache"
                | "wasm_run"
                | "try"
                | "match"
                | "os"
                | "arch"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_json_round_trip() {
        let src = r#"{"kind":"print","line":3,"args":{"msg":"hi"}}"#;
        let op: Op = serde_json::from_str(src).unwrap();
        assert_eq!(op.kind, "print");
        assert!(!op.is_block());
        assert_eq!(serde_json::to_string(&op).unwrap(), src);
    }

    #[test]
    fn block_kinds() {
        let op = Op { kind: "for_each".into(), ..Default::default() };
        assert!(op.is_block());
    }
}
