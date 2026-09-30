use crate::all_handlers;

/// Sorted names of built-in op handlers exposed to .perch authors. Internal
/// handlers (`_prefix`) are omitted.
pub fn builtin_kinds() -> Vec<String> {
    let mut out: Vec<String> = all_handlers().into_keys().filter(|k| !k.starts_with('_')).collect();
    out.sort();
    out
}
