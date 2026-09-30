//! Port of infra/ops: registers every built-in op handler for the interpreter.
//! Group A (process, flow, contexts, cache, assertions, system, install,
//! requires, restrict, errors, hookcat) is in src/*.rs; group B (archive,
//! bundle, compression, encoding, files, hash, http, network, paths, regex,
//! strings, textlines, time, version) is in src/group_b/.
mod group_b;

use perch_interpreter::Handler;
use std::collections::HashMap;

/// Fresh map of every op kind perch knows about (Go: AllHandlers).
pub fn all_handlers() -> HashMap<String, Handler> {
    let mut m = HashMap::new();
    group_b::register_all(&mut m);
    m
}
