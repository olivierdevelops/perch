//! Group B op handlers (files, http, network, archive, bundle, cache-free utilities). Owned by agent B.
use perch_interpreter::Handler;
use std::collections::HashMap;

pub fn register_all(_m: &mut HashMap<String, Handler>) {}
