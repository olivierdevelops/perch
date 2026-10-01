//! Error model: every failing op returns an [`OpError`] carrying a finite
//! [`ErrorKind`], surfaced to `catch` blocks as `${err.*}`.
use std::fmt;

/// The finite enum users match against in `case KIND ... end`. The string form
/// is the user-facing snake_case identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    ShellExitNonzero,
    ShellMetacharsDenied,
    ShellBinNotAllowed,
    ShellSignalKilled,
    HTTP4xx,
    HTTP5xx,
    HTTPRedirectRefused,
    HTTPSSRFBlocked,
    HTTPDNSFailed,
    HTTPTimeout,
    FileNotFound,
    FilePermissionDenied,
    FilePathDisallowed,
    FileAlreadyExists,
    CapShellDenied,
    CapNetworkDenied,
    CapSubprocessDenied,
    CapWriteDenied,
    WasmCompileFailed,
    WasmModuleExited,
    WasmCapabilityDenied,
    WasmHTTPRefused,
    ConfinementUnavailable,
    UnresolvedVar,
    UnresolvedTemplate,
    TimeoutExceeded,
    SignalReceived,
    UserFail,
    AssertFailed,
    CommandNotFound,
    BinNotFound,
    BinNotDeclared,
    EnvNotDeclared,
    HostNotDeclared,
    ReadNotDeclared,
    WriteNotDeclared,
    RequirementUnmet,
    ShellNotPermitted,
    ReadNotPermitted,
    WriteNotPermitted,
    NetNotPermitted,
    EnvNotPermitted,
    SubprocessNotPermitted,
    UnknownCapability,
    CapabilityKindMismatch,
    Unclassified,
    RequirementBlockMissing,
}

impl ErrorKind {
    /// Every user-visible kind, for `--check` and LSP completion. Excludes the
    /// reserved `RequirementBlockMissing` (a missing block is normalized at load).
    pub const ALL: &'static [ErrorKind] = &[
        ErrorKind::ShellExitNonzero,
        ErrorKind::ShellMetacharsDenied,
        ErrorKind::ShellBinNotAllowed,
        ErrorKind::ShellSignalKilled,
        ErrorKind::HTTP4xx,
        ErrorKind::HTTP5xx,
        ErrorKind::HTTPRedirectRefused,
        ErrorKind::HTTPSSRFBlocked,
        ErrorKind::HTTPDNSFailed,
        ErrorKind::HTTPTimeout,
        ErrorKind::FileNotFound,
        ErrorKind::FilePermissionDenied,
        ErrorKind::FilePathDisallowed,
        ErrorKind::FileAlreadyExists,
        ErrorKind::CapShellDenied,
        ErrorKind::CapNetworkDenied,
        ErrorKind::CapSubprocessDenied,
        ErrorKind::CapWriteDenied,
        ErrorKind::WasmCompileFailed,
        ErrorKind::WasmModuleExited,
        ErrorKind::WasmCapabilityDenied,
        ErrorKind::WasmHTTPRefused,
        ErrorKind::ConfinementUnavailable,
        ErrorKind::UnresolvedVar,
        ErrorKind::UnresolvedTemplate,
        ErrorKind::TimeoutExceeded,
        ErrorKind::SignalReceived,
        ErrorKind::UserFail,
        ErrorKind::AssertFailed,
        ErrorKind::CommandNotFound,
        ErrorKind::BinNotFound,
        ErrorKind::BinNotDeclared,
        ErrorKind::EnvNotDeclared,
        ErrorKind::HostNotDeclared,
        ErrorKind::ReadNotDeclared,
        ErrorKind::WriteNotDeclared,
        ErrorKind::RequirementUnmet,
        ErrorKind::ShellNotPermitted,
        ErrorKind::ReadNotPermitted,
        ErrorKind::WriteNotPermitted,
        ErrorKind::NetNotPermitted,
        ErrorKind::EnvNotPermitted,
        ErrorKind::SubprocessNotPermitted,
        ErrorKind::UnknownCapability,
        ErrorKind::CapabilityKindMismatch,
        ErrorKind::Unclassified,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::ShellExitNonzero => "shell_exit_nonzero",
            ErrorKind::ShellMetacharsDenied => "shell_metachars_denied",
            ErrorKind::ShellBinNotAllowed => "shell_bin_not_allowed",
            ErrorKind::ShellSignalKilled => "shell_signal_killed",
            ErrorKind::HTTP4xx => "http_4xx",
            ErrorKind::HTTP5xx => "http_5xx",
            ErrorKind::HTTPRedirectRefused => "http_redirect_refused",
            ErrorKind::HTTPSSRFBlocked => "http_ssrf_blocked",
            ErrorKind::HTTPDNSFailed => "http_dns_failed",
            ErrorKind::HTTPTimeout => "http_timeout",
            ErrorKind::FileNotFound => "file_not_found",
            ErrorKind::FilePermissionDenied => "file_permission_denied",
            ErrorKind::FilePathDisallowed => "file_path_disallowed",
            ErrorKind::FileAlreadyExists => "file_already_exists",
            ErrorKind::CapShellDenied => "cap_shell_denied",
            ErrorKind::CapNetworkDenied => "cap_network_denied",
            ErrorKind::CapSubprocessDenied => "cap_subprocess_denied",
            ErrorKind::CapWriteDenied => "cap_write_denied",
            ErrorKind::WasmCompileFailed => "wasm_compile_failed",
            ErrorKind::WasmModuleExited => "wasm_module_exited",
            ErrorKind::WasmCapabilityDenied => "wasm_capability_denied",
            ErrorKind::WasmHTTPRefused => "wasm_http_refused",
            ErrorKind::ConfinementUnavailable => "confinement_unavailable",
            ErrorKind::UnresolvedVar => "unresolved_var",
            ErrorKind::UnresolvedTemplate => "unresolved_template",
            ErrorKind::TimeoutExceeded => "timeout_exceeded",
            ErrorKind::SignalReceived => "signal_received",
            ErrorKind::UserFail => "user_fail",
            ErrorKind::AssertFailed => "assert_failed",
            ErrorKind::CommandNotFound => "command_not_found",
            ErrorKind::BinNotFound => "bin_not_found",
            ErrorKind::BinNotDeclared => "bin_not_declared",
            ErrorKind::EnvNotDeclared => "env_not_declared",
            ErrorKind::HostNotDeclared => "host_not_declared",
            ErrorKind::ReadNotDeclared => "read_not_declared",
            ErrorKind::WriteNotDeclared => "write_not_declared",
            ErrorKind::RequirementUnmet => "requirement_unmet",
            ErrorKind::ShellNotPermitted => "shell_not_permitted",
            ErrorKind::ReadNotPermitted => "read_not_permitted",
            ErrorKind::WriteNotPermitted => "write_not_permitted",
            ErrorKind::NetNotPermitted => "net_not_permitted",
            ErrorKind::EnvNotPermitted => "env_not_permitted",
            ErrorKind::SubprocessNotPermitted => "subprocess_not_permitted",
            ErrorKind::UnknownCapability => "unknown_capability",
            ErrorKind::CapabilityKindMismatch => "capability_kind_mismatch",
            ErrorKind::Unclassified => "unclassified",
            ErrorKind::RequirementBlockMissing => "requirement_block_missing",
        }
    }

    pub fn parse(s: &str) -> Option<ErrorKind> {
        Self::ALL.iter().copied().find(|k| k.as_str() == s)
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether `s` is one of the enum members.
pub fn is_known_error_kind(s: &str) -> bool {
    ErrorKind::parse(s).is_some()
}

/// Structured error returned by every failure-tagged op.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpError {
    pub kind: ErrorKind,
    /// Human-readable.
    pub message: String,
    /// Op-specific code (e.g. "500" for http_4xx, exit status for shell).
    pub code: String,
    /// Op kind that failed (e.g. "http_get").
    pub op: String,
    /// Structured extra info (e.g. blocked IP, denied capability name).
    pub detail: String,
}

impl OpError {
    /// Canonical constructor; an empty message defaults to the kind's name.
    pub fn new(op: &str, kind: ErrorKind, msg: &str) -> Self {
        let message = if msg.is_empty() { kind.as_str().to_string() } else { msg.to_string() };
        OpError { kind, message, code: String::new(), op: op.to_string(), detail: String::new() }
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = code.into();
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    /// Coerces any error to an `OpError` tagged `Unclassified`.
    pub fn classify(op_kind: &str, err: &(dyn std::error::Error + 'static)) -> OpError {
        match err.downcast_ref::<OpError>() {
            Some(oe) => {
                let mut oe = oe.clone();
                if oe.op.is_empty() {
                    oe.op = op_kind.to_string();
                }
                oe
            }
            None => OpError::new(op_kind, ErrorKind::Unclassified, &err.to_string()),
        }
    }
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.detail.is_empty() {
            write!(f, "{}: {}", self.kind, self.message)
        } else {
            write!(f, "{}: {} ({})", self.kind, self.message, self.detail)
        }
    }
}

impl std::error::Error for OpError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip() {
        for k in ErrorKind::ALL {
            assert_eq!(ErrorKind::parse(k.as_str()), Some(*k));
        }
        assert!(!is_known_error_kind("nope"));
    }

    #[test]
    fn display_with_detail() {
        let e = OpError::new("shell", ErrorKind::ShellExitNonzero, "boom").with_detail("x");
        assert_eq!(e.to_string(), "shell_exit_nonzero: boom (x)");
    }
}
