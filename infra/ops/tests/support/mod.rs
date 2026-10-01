//! Shared helpers for the integration tests.
use std::path::PathBuf;

/// A canonical path spelled so it can be embedded in `.perch` source and shell
/// commands: on Windows the `\\?\` verbatim prefix is dropped and `\` becomes
/// `/` (a backslash starts an escape inside a perch string literal). Identity
/// on Unix.
pub fn portable(p: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
        PathBuf::from(s.replace('\\', "/"))
    }
    #[cfg(not(windows))]
    {
        p
    }
}
