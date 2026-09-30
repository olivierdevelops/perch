use crate::docs_data::OP_DOCS;

/// Returns signature and description for a built-in op.
pub(crate) fn doc_for(kind: &str) -> (&'static str, &'static str) {
    match OP_DOCS.iter().find(|(k, _, _)| *k == kind) {
        Some((_, sig, desc)) => (sig, desc),
        None => ("", "Built-in perch op."),
    }
}
