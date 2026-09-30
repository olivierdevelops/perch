//! Compiles a perch `.perch` source file into a [`perch_domain::Program`]. The
//! pipeline:
//!
//!  1. Run the source through the embedded `lib.capy` via the capy engine
//!     (the native `capy-core` crate). Output is an NDJSON event stream.
//!  2. Stream-parse events into a Program: each line corresponds to one of the
//!     lib's `write` calls (name, command_begin, config, op, ...).
//!  3. Fold flat `_enter` / `_leave` op markers into nested `Op::body` vectors.
mod enforce;
mod error;
mod loader;
mod registry;

pub use error::{errors_as, CapyParseError, Error, WrappedError};
pub use loader::{library_source, load, load_from_string, op_kinds};
