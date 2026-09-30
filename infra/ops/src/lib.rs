//! Port of infra/ops: registers every built-in op handler for the interpreter.
//! Group A (process, flow, contexts, cache, assertions, system, install,
//! requires, restrict, errors, hookcat) is in src/*.rs; group B (archive,
//! bundle, compression, encoding, files, hash, http, network, paths, regex,
//! strings, textlines, time, version) is in src/group_b/.
mod assertions;
mod cache;
pub mod common;
mod contexts;
mod errors;
mod flow;
mod group_b;
mod hookcat;
mod install;
mod kinds;
mod process;
mod requires;
mod restrict;
pub mod seams;
mod system;
mod wasm;

pub use group_b::{bundle_hash, bundle_read_file, set_bundle};
pub use flow::{arch_target_matches, compare_values, looks_like_version, os_target_matches};
pub use hookcat::{
    hook_categories, hook_category_of, HOOK_CAT_ENV, HOOK_CAT_EXEC, HOOK_CAT_NET, HOOK_CAT_READ, HOOK_CAT_WRITE,
};
pub use install::{contains_line, copy_file, ensure_line_in_file};
pub use kinds::builtin_kinds;
pub use process::{apply_env, build_env, check_shell, default_bin_env};
pub use requires::{
    abs_under, apply_requires_path_gating, check_bin_hash, check_env_declared, check_exec_bin, check_host_declared,
    check_net_declared, check_path_declared, check_shell_bin_declared, check_subprocess_bin, first_shell_token,
    host_of_url, load_hash_file, path_within_any, preflight, resolve_exec_path,
};
pub use restrict::{
    apply_mask_gating, apply_restrictions, blocked_by_restriction, restriction_list, summarise_restrictions,
    Restrictions, RESTRICT_NO_NETWORK, RESTRICT_NO_SHELL, RESTRICT_NO_SUBPROCESS, RESTRICT_NO_WRITE,
};

use perch_interpreter::Handler;
use std::collections::HashMap;

/// Fresh map of every op kind perch knows about (Go: AllHandlers). The
/// orchestrator hands this to `Interpreter::new`.
pub fn all_handlers() -> HashMap<String, Handler> {
    let mut m = HashMap::new();
    process::register_process(&mut m);
    flow::register_flow(&mut m);
    contexts::register_contexts(&mut m);
    cache::register_cache(&mut m);
    assertions::register_assertions(&mut m);
    wasm::register_wasm(&mut m);
    system::register_system(&mut m);
    group_b::register_all(&mut m);
    install::register_install(&mut m);
    errors::register_error_ops(&mut m);
    // Wrap filesystem ops so a declared `requires` block gates their read/write
    // paths against the declared roots. No-op without the block.
    requires::apply_requires_path_gating(&mut m);
    m
}
