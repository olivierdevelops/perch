//! Data types that describe a perch program. Pure data; imports nothing from
//! the rest of the project.
mod bin;
mod errors;
mod program;

pub use bin::*;
pub use errors::*;
pub use program::*;
