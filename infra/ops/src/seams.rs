//! Seams onto group-B code (version.go / bundle.go): group A needs
//! `versionCompare` (flow.rs) and `BundleReadFile` (requires.rs hash_file).

/// Go `versionCompare` (version.go): semver-aware ordering of two version
/// strings; unparseable input falls back to plain string comparison.
pub fn version_compare(a: &str, b: &str) -> i32 {
    crate::group_b::version_compare(a, b)
}

/// Go `BundleReadFile`: `None` = no bundle loaded, `Some(Ok(bytes))` = entry
/// found, `Some(Err(msg))` = bundle present but the entry is unreadable.
pub fn bundle_read_file(entry: &str) -> Option<Result<Vec<u8>, String>> {
    crate::group_b::bundle_read_file(entry)
}
