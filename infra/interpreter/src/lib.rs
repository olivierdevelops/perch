//! Walks a parsed perch program and dispatches each op to a handler.
//! Bindings hold the per-invocation runtime state.
//!
//! Ops register by building a `HashMap<String, Handler>` (see [`handler`]) and
//! passing it to [`Interpreter::new`]; the `infra/ops` crate owns the concrete
//! handlers, the interpreter only knows the registry.
mod autovars;
mod bindings;
mod cliargs;
mod interpolate;
mod interpreter;
mod io;

pub use autovars::*;
pub use bindings::*;
pub use cliargs::go_quote;
pub use interpolate::*;
pub use interpreter::*;
pub use io::*;
