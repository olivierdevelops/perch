// Package domain — error model.
//
// Every op that can fail returns an *OpError* carrying a *Kind* (from a
// finite enum), a human message, an op-specific code, and structured
// detail. The interpreter populates `${err.kind}`, `${err.message}`,
// `${err.code}`, `${err.op}`, `${err.detail}` inside `catch` blocks so
// users can discriminate via `match err.kind / case <kind>`.
package domain

import "fmt"

// ErrorKind is the finite enum users match against in `case KIND ... end`.
//
// Adding a new kind:
//
//  1. Add the constant below in the appropriate group.
//  2. Add it to AllErrorKinds() so --check / LSP know it.
//  3. Add a paragraph to docs/errors.md documenting when it fires.
//
// The string form is the user-facing identifier — it MUST be a valid
// perch identifier (snake_case, no leading digit).
type ErrorKind string

const (
	// Shell ops
	ErrShellExitNonzero     ErrorKind = "shell_exit_nonzero"
	ErrShellMetacharsDenied ErrorKind = "shell_metachars_denied"
	ErrShellBinNotAllowed   ErrorKind = "shell_bin_not_allowed"
	ErrShellSignalKilled    ErrorKind = "shell_signal_killed"

	// HTTP ops
	ErrHTTP4xx             ErrorKind = "http_4xx"
	ErrHTTP5xx             ErrorKind = "http_5xx"
	ErrHTTPRedirectRefused ErrorKind = "http_redirect_refused"
	ErrHTTPSSRFBlocked     ErrorKind = "http_ssrf_blocked"
	ErrHTTPDNSFailed       ErrorKind = "http_dns_failed"
	ErrHTTPTimeout         ErrorKind = "http_timeout"

	// File ops
	ErrFileNotFound         ErrorKind = "file_not_found"
	ErrFilePermissionDenied ErrorKind = "file_permission_denied"
	ErrFilePathDisallowed   ErrorKind = "file_path_disallowed"
	ErrFileAlreadyExists    ErrorKind = "file_already_exists"

	// Capability denials (runtime restriction layer)
	ErrCapShellDenied      ErrorKind = "cap_shell_denied"
	ErrCapNetworkDenied    ErrorKind = "cap_network_denied"
	ErrCapSubprocessDenied ErrorKind = "cap_subprocess_denied"
	ErrCapWriteDenied      ErrorKind = "cap_write_denied"

	// WASM ops
	ErrWasmCompileFailed     ErrorKind = "wasm_compile_failed"
	ErrWasmModuleExited      ErrorKind = "wasm_module_exited"
	ErrWasmCapabilityDenied  ErrorKind = "wasm_capability_denied"
	ErrWasmHTTPRefused       ErrorKind = "wasm_http_refused"

	// Interpolation
	ErrUnresolvedVar      ErrorKind = "unresolved_var"
	ErrUnresolvedTemplate ErrorKind = "unresolved_template"

	// Runtime
	ErrTimeoutExceeded ErrorKind = "timeout_exceeded"
	ErrSignalReceived  ErrorKind = "signal_received"
	ErrUserFail        ErrorKind = "user_fail"
	ErrAssertFailed    ErrorKind = "assert_failed"

	// Lookup
	ErrCommandNotFound ErrorKind = "command_not_found"
	ErrBinNotFound     ErrorKind = "bin_not_found"

	// Requires-block enforcement (file-declared manifest)
	ErrBinNotDeclared    ErrorKind = "bin_not_declared"
	ErrEnvNotDeclared    ErrorKind = "env_not_declared"
	ErrHostNotDeclared   ErrorKind = "host_not_declared"
	ErrReadNotDeclared   ErrorKind = "read_not_declared"
	ErrWriteNotDeclared  ErrorKind = "write_not_declared"
	ErrRequirementUnmet  ErrorKind = "requirement_unmet"

	// Catch-all for handlers that haven't been migrated to tagged errors
	// yet. Tagging is mechanical (see infra/ops/errs.go) and proceeds
	// incrementally; this is the fallback so users always get *some*
	// kind even before every op is migrated.
	ErrUnclassified ErrorKind = "unclassified"
)

// AllErrorKinds returns every defined ErrorKind. Used by `--check` to
// validate `case KIND` arms in user code, and by the LSP for
// completion. Keep in sync with the constants above.
func AllErrorKinds() []ErrorKind {
	return []ErrorKind{
		ErrShellExitNonzero, ErrShellMetacharsDenied, ErrShellBinNotAllowed, ErrShellSignalKilled,
		ErrHTTP4xx, ErrHTTP5xx, ErrHTTPRedirectRefused, ErrHTTPSSRFBlocked, ErrHTTPDNSFailed, ErrHTTPTimeout,
		ErrFileNotFound, ErrFilePermissionDenied, ErrFilePathDisallowed, ErrFileAlreadyExists,
		ErrCapShellDenied, ErrCapNetworkDenied, ErrCapSubprocessDenied, ErrCapWriteDenied,
		ErrWasmCompileFailed, ErrWasmModuleExited, ErrWasmCapabilityDenied, ErrWasmHTTPRefused,
		ErrUnresolvedVar, ErrUnresolvedTemplate,
		ErrTimeoutExceeded, ErrSignalReceived, ErrUserFail, ErrAssertFailed,
		ErrCommandNotFound, ErrBinNotFound,
		ErrBinNotDeclared, ErrEnvNotDeclared, ErrHostNotDeclared,
		ErrReadNotDeclared, ErrWriteNotDeclared, ErrRequirementUnmet,
		ErrUnclassified,
	}
}

// IsKnownErrorKind reports whether the given string is one of the
// enum members.
func IsKnownErrorKind(s string) bool {
	k := ErrorKind(s)
	for _, kk := range AllErrorKinds() {
		if k == kk {
			return true
		}
	}
	return false
}

// OpError is the structured error returned by every failure-tagged op.
// The interpreter populates `${err.*}` bindings inside `catch` blocks
// from these fields.
//
// Plain Go errors that aren't *OpError are coerced to ErrUnclassified at
// the catch boundary so user code always sees a kind.
type OpError struct {
	Kind    ErrorKind
	Message string // human-readable
	Code    string // op-specific code (e.g. "500" for http_4xx, exit status for shell)
	Op      string // op kind that failed (e.g. "http_get")
	Detail  string // structured extra info (e.g. SSRF blocked IP, denied capability name)
}

func (e *OpError) Error() string {
	if e.Detail != "" {
		return fmt.Sprintf("%s: %s (%s)", e.Kind, e.Message, e.Detail)
	}
	return fmt.Sprintf("%s: %s", e.Kind, e.Message)
}

// NewOpError is the canonical constructor. Op + Kind are required;
// Message defaults to a sane derivation if empty.
func NewOpError(op string, kind ErrorKind, msg string) *OpError {
	if msg == "" {
		msg = string(kind)
	}
	return &OpError{Op: op, Kind: kind, Message: msg}
}

// WithCode returns e with Code set.
func (e *OpError) WithCode(code string) *OpError {
	e.Code = code
	return e
}

// WithDetail returns e with Detail set.
func (e *OpError) WithDetail(detail string) *OpError {
	e.Detail = detail
	return e
}

// ClassifyError coerces a plain error to *OpError. If err is already an
// *OpError, returned as-is. nil → nil.
func ClassifyError(opKind string, err error) *OpError {
	if err == nil {
		return nil
	}
	if oe, ok := err.(*OpError); ok {
		if oe.Op == "" {
			oe.Op = opKind
		}
		return oe
	}
	return &OpError{
		Kind:    ErrUnclassified,
		Message: err.Error(),
		Op:      opKind,
	}
}
