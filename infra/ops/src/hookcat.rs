//! Hook categories — the capability classes a `hooks` block may target
//! (`before write …`, `after net …`). They mirror the `requires` capabilities so
//! the vocabulary stays small: exec · net · write · read · env. An op can belong
//! to more than one (e.g. `cp` is both read and write).
use crate::restrict::{
    restrict_blocks, RESTRICT_NO_NETWORK, RESTRICT_NO_SHELL, RESTRICT_NO_SUBPROCESS, RESTRICT_NO_WRITE,
};
use std::collections::HashMap;
use std::sync::OnceLock;

pub const HOOK_CAT_EXEC: &str = "exec"; // any subprocess spawn (shell + exec + process mgmt)
pub const HOOK_CAT_NET: &str = "net"; // any network op
pub const HOOK_CAT_WRITE: &str = "write"; // any filesystem-mutating op
pub const HOOK_CAT_READ: &str = "read"; // any filesystem-reading op
pub const HOOK_CAT_ENV: &str = "env"; // any environment read/write

/// Not covered by the restrict flags (there is no --no-read / --no-env), so
/// listed explicitly. Source of truth is the capability-gating table.
const HOOK_READ_KINDS: &[&str] = &[
    "read_file", "exists", "is_dir", "is_file", "file_size", "list_dir", "walk_dir", "read_link", "sha256_file",
    "sha1_file", "md5_file", "glob", "verify_sha256", "cp", "mv", "copy_dir",
];

const HOOK_ENV_KINDS: &[&str] = &["get_env", "set_env", "unset_env", "env_has", "env_default"];

fn build_hook_cat() -> HashMap<String, Vec<String>> {
    let mut m: HashMap<String, Vec<&'static str>> = HashMap::new();
    let mut add = |kind: &str, cat: &'static str| {
        let e = m.entry(kind.to_string()).or_default();
        if !e.contains(&cat) {
            e.push(cat);
        }
    };
    for k in restrict_blocks(RESTRICT_NO_SHELL) {
        add(k, HOOK_CAT_EXEC);
    }
    for k in restrict_blocks(RESTRICT_NO_SUBPROCESS) {
        add(k, HOOK_CAT_EXEC);
    }
    for k in restrict_blocks(RESTRICT_NO_NETWORK) {
        add(k, HOOK_CAT_NET);
    }
    for k in restrict_blocks(RESTRICT_NO_WRITE) {
        add(k, HOOK_CAT_WRITE);
    }
    for k in HOOK_READ_KINDS {
        add(k, HOOK_CAT_READ);
    }
    for k in HOOK_ENV_KINDS {
        add(k, HOOK_CAT_ENV);
    }
    m.into_iter().map(|(k, v)| (k, v.into_iter().map(String::from).collect())).collect()
}

/// The capability categories an op kind belongs to (may be empty for a pure op,
/// or multiple for a read+write op). Wired into the interpreter so a
/// `hooks before write …` line matches every write op without the interpreter
/// needing to depend on the ops crate.
pub fn hook_category_of(kind: &str) -> Vec<String> {
    static MAP: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    MAP.get_or_init(build_hook_cat).get(kind).cloned().unwrap_or_default()
}

/// The set of valid category names a hooks line may target — used by `--check`
/// to validate a hook's target.
pub fn hook_categories() -> Vec<String> {
    [HOOK_CAT_EXEC, HOOK_CAT_NET, HOOK_CAT_WRITE, HOOK_CAT_READ, HOOK_CAT_ENV].map(String::from).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(hook_category_of("write_file"), vec!["write"]);
        let mut cp = hook_category_of("cp");
        cp.sort();
        assert_eq!(cp, vec!["read", "write"]);
        assert!(hook_category_of("print").is_empty());
        assert_eq!(hook_category_of("exec"), vec!["exec"]);
    }
}
