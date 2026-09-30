//! Kernel confinement of spawned binaries to the declared `read`/`write`
//! scopes of a `requires` block (PLAN-2026-0001 R03).
//!
//! VHCO: everything is passed as parameters; there is no global state. The
//! platform backends (`macos`, `linux`, `other`) each expose the same two
//! functions, `probe()` and `confine()`, and exactly one is compiled in.
//!
//! Caller contract: call `confine` only when the program declared at least one
//! scope (`!scopes.is_empty()`). With nothing declared there is nothing to
//! enforce and confining would only break ordinary binaries.
use std::path::PathBuf;
use std::process::Command;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod other;

#[cfg(target_os = "linux")]
use linux as backend;
#[cfg(target_os = "macos")]
use macos as backend;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use other as backend;

// The `other` backend is always compiled for tests so it can be unit tested on
// any OS.
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[path = "other.rs"]
mod other_for_tests;

/// Declared scopes, already resolved to concrete paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scopes {
    pub read: Vec<PathBuf>,
    pub write: Vec<PathBuf>,
    pub hosts: Vec<String>,
}

impl Scopes {
    /// True when nothing is declared (callers must not confine then).
    pub fn is_empty(&self) -> bool {
        self.read.is_empty() && self.write.is_empty() && self.hosts.is_empty()
    }
}

/// Whether this platform can enforce scopes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Support {
    Enforced,
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfineError {
    Unsupported(String),
}

impl std::fmt::Display for ConfineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfineError::Unsupported(r) => write!(f, "confinement unsupported: {r}"),
        }
    }
}
impl std::error::Error for ConfineError {}

/// Probes the platform mechanism once.
pub fn probe() -> Support {
    backend::probe()
}

/// Mutates `cmd` so the spawned process is confined to `scopes`.
pub fn confine(cmd: &mut Command, scopes: &Scopes) -> Result<(), ConfineError> {
    backend::confine(cmd, scopes)
}

/// Builds `Scopes` from a program's declared requirements. `resolve` turns a
/// raw declared root (possibly with `${…}` placeholders or relative) into a
/// concrete absolute path; the caller owns interpolation and cwd.
pub fn scopes_from_requirements(req: &perch_domain::Requirements, resolve: &dyn Fn(&str) -> PathBuf) -> Scopes {
    let norm = |raw: &String| {
        let p = resolve(raw);
        // Reuse the gate's cleaning so confinement matches perch's own checks.
        PathBuf::from(crate::requires::abs_under(&p.to_string_lossy(), "/"))
    };
    Scopes {
        read: req.read_roots.iter().map(norm).collect(),
        write: req.write_roots.iter().map(norm).collect(),
        hosts: req.hosts.iter().map(|h| h.name.clone()).collect(),
    }
}

/// Exact stderr text for the opt-out path.
pub fn advisory_banner(reason: &str) -> String {
    format!("perch: declared read/write scopes are advisory on this platform ({reason}); spawned binaries are NOT confined")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_text() {
        assert_eq!(
            advisory_banner("x"),
            "perch: declared read/write scopes are advisory on this platform (x); spawned binaries are NOT confined"
        );
    }

    #[test]
    fn other_backend_is_unsupported() {
        let mut c = Command::new("true");
        let e = other_for_tests_confine(&mut c);
        assert!(matches!(e, Err(ConfineError::Unsupported(r)) if r.contains("no confinement mechanism")));
        assert!(matches!(other_for_tests_probe(), Support::Unsupported(_)));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn other_for_tests_confine(c: &mut Command) -> Result<(), ConfineError> {
        other_for_tests::confine(c, &Scopes::default())
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn other_for_tests_probe() -> Support {
        other_for_tests::probe()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn other_for_tests_confine(c: &mut Command) -> Result<(), ConfineError> {
        backend::confine(c, &Scopes::default())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn other_for_tests_probe() -> Support {
        backend::probe()
    }

    #[test]
    fn scopes_normalised() {
        let req = perch_domain::Requirements {
            read_roots: vec!["r/../r2".into()],
            write_roots: vec!["w".into()],
            hosts: vec![perch_domain::HostReq { name: "example.com".into(), optional: false }],
            ..Default::default()
        };
        let s = scopes_from_requirements(&req, &|raw| PathBuf::from("/base").join(raw));
        assert_eq!(s.read, vec![PathBuf::from("/base/r2")]);
        assert_eq!(s.write, vec![PathBuf::from("/base/w")]);
        assert_eq!(s.hosts, vec!["example.com".to_string()]);
        assert!(!s.is_empty());
        assert!(Scopes::default().is_empty());
    }
}
