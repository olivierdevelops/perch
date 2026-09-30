//! JSON catalog of the built-in perch ops (signature, docs, capability
//! requirements, examples) plus the auto-bound `${name}` variables.
mod argdesc;
mod argdesc_data;
mod catalog;
mod docs;
mod docs_data;
mod examples;
mod examples_data;
mod parse_capy;
mod requirements;

pub use catalog::{build, marshal_json, Arg, Catalog, Op, Var};
pub use requirements::Requirements;

#[cfg(test)]
pub(crate) use parse_capy::parse_capy_args;
