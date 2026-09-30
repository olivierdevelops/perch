//! wasm_run and the wasm_* marker ops (wasm.go / wasm_http.go).
use perch_interpreter::Handler;
use std::collections::HashMap;

/// TODO(wasm phase): port wasm.go + wasm_http.go. Until then no wasm ops are
/// registered (`wasm_run`, `wasm_arg`, `wasm_mount_read`, `wasm_mount_write`,
/// `wasm_env`, `wasm_allow_host`).
pub fn register_wasm(_m: &mut HashMap<String, Handler>) {}
