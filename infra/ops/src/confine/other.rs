//! Fallback backend: no confinement mechanism.
use super::{ConfineError, Scopes, Support};
use std::process::Command;

const REASON: &str = "no confinement mechanism on this platform";

#[allow(dead_code)]
pub fn probe() -> Support {
    Support::Unsupported(REASON.to_string())
}

#[allow(dead_code)]
pub fn confine(_cmd: &mut Command, _scopes: &Scopes) -> Result<(), ConfineError> {
    Err(ConfineError::Unsupported(REASON.to_string()))
}
