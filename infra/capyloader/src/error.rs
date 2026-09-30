//! Error plumbing. The Go loader returns plain `error` values, sometimes
//! wrapped with `%w`; callers reach the cause with `errors.As`. Here the
//! error is a boxed `std::error::Error` and [`errors_as`] walks the source
//! chain the same way.
use std::fmt;

/// The loader's error type: any error, `Send + Sync` so it can cross threads.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// A structured error from the capy engine, carrying the position and hint the
/// native engine produces. `Display` is the bare message (no `line:col:`
/// prefix), matching the Go binding's `Error.Error()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapyParseError {
    pub msg: String,
    pub hint: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for CapyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for CapyParseError {}

/// `fmt.Errorf("ctx: %w", source)`: prefixes a message and keeps the cause
/// reachable through [`std::error::Error::source`].
#[derive(Debug)]
pub struct WrappedError {
    pub context: String,
    pub source: Error,
}

impl fmt::Display for WrappedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.context, self.source)
    }
}

impl std::error::Error for WrappedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Wraps `source` with a context prefix (`%w`).
pub(crate) fn wrap(context: impl Into<String>, source: impl Into<Error>) -> Error {
    Box::new(WrappedError { context: context.into(), source: source.into() })
}

/// Walks the error chain looking for a `T` (Go's `errors.As`).
pub fn errors_as<'a, T: std::error::Error + 'static>(
    err: &'a (dyn std::error::Error + 'static),
) -> Option<&'a T> {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = cur {
        if let Some(t) = e.downcast_ref::<T>() {
            return Some(t);
        }
        cur = e.source();
    }
    None
}
