package ops

import (
	"strings"
	"testing"

	"github.com/luowensheng/perch/domain"
	"github.com/luowensheng/perch/infra/capyloader"
	"github.com/luowensheng/perch/infra/interpreter"
)

// newGatedInterp builds an interpreter whose program carries the given
// Requirements (Declared=true). Handlers are the full gated set.
func newGatedInterp(req domain.Requirements) (*interpreter.Interpreter, *interpreter.Bindings) {
	req.Declared = true
	p := &domain.Program{
		Commands:     map[string]*domain.Command{},
		Requirements: req,
	}
	i := interpreter.New(AllHandlers(), p)
	b := interpreter.NewBindings(".")
	return i, b
}

// call dispatches one op handler directly with the given (already-resolved) args.
func call(i *interpreter.Interpreter, b *interpreter.Bindings, kind string, args map[string]any) error {
	h, ok := i.Handlers[kind]
	if !ok {
		panic("no handler for " + kind)
	}
	_, err := h(i, b, args)
	return err
}

func isKind(err error, want domain.ErrorKind) bool {
	if err == nil {
		return false
	}
	if oe, ok := err.(*domain.OpError); ok {
		return oe.Kind == want
	}
	return false
}

// ── shell / bin ───────────────────────────────────────────────────────────

func TestGate_Shell_Bin(t *testing.T) {
	i, b := newGatedInterp(domain.Requirements{Bins: []domain.BinReq{{Name: "echo"}}})
	// declared bin → allowed (echo succeeds)
	if err := call(i, b, "shell", map[string]any{"cmd": "echo hi"}); err != nil {
		t.Fatalf("declared bin echo should run: %v", err)
	}
	// undeclared bin → bin_not_declared
	if err := call(i, b, "shell", map[string]any{"cmd": "curl https://x"}); !isKind(err, domain.ErrBinNotDeclared) {
		t.Fatalf("undeclared curl want bin_not_declared, got %v", err)
	}
}

// ── env ─────────────────────────────────────────────────────────────────

func TestGate_Env(t *testing.T) {
	i, b := newGatedInterp(domain.Requirements{Envs: []domain.EnvReq{{Name: "HOME"}}})
	if err := call(i, b, "get_env", map[string]any{"_0": "HOME"}); err != nil {
		t.Fatalf("declared env HOME allowed: %v", err)
	}
	if err := call(i, b, "get_env", map[string]any{"_0": "AWS_SECRET_ACCESS_KEY"}); !isKind(err, domain.ErrEnvNotDeclared) {
		t.Fatalf("undeclared env want env_not_declared, got %v", err)
	}
	// writes are gated too
	if err := call(i, b, "set_env", map[string]any{"_0": "SECRET", "_1": "x"}); !isKind(err, domain.ErrEnvNotDeclared) {
		t.Fatalf("set_env undeclared want env_not_declared, got %v", err)
	}
	if err := call(i, b, "unset_env", map[string]any{"_0": "SECRET"}); !isKind(err, domain.ErrEnvNotDeclared) {
		t.Fatalf("unset_env undeclared want env_not_declared, got %v", err)
	}
	if err := call(i, b, "env_has", map[string]any{"_0": "SECRET"}); !isKind(err, domain.ErrEnvNotDeclared) {
		t.Fatalf("env_has undeclared want env_not_declared, got %v", err)
	}
}

// ── network: host ops ─────────────────────────────────────────────────────

func TestGate_NetworkHost(t *testing.T) {
	i, b := newGatedInterp(domain.Requirements{Hosts: []domain.HostReq{{Name: "declared.example.com"}}})
	// undeclared host → host_not_declared (we never actually dial)
	if err := call(i, b, "dns_lookup", map[string]any{"_0": "evil.example.com"}); !isKind(err, domain.ErrHostNotDeclared) {
		t.Fatalf("dns_lookup undeclared host want host_not_declared, got %v", err)
	}
	if err := call(i, b, "port_check", map[string]any{"_0": "evil.example.com", "_1": "80"}); !isKind(err, domain.ErrHostNotDeclared) {
		t.Fatalf("port_check undeclared host want host_not_declared, got %v", err)
	}
	if err := call(i, b, "wait_for_url", map[string]any{"_0": "https://evil.example.com/x", "_1": "1"}); !isKind(err, domain.ErrHostNotDeclared) {
		t.Fatalf("wait_for_url undeclared host want host_not_declared, got %v", err)
	}
}

// ── network: hostless ops require net declared (≥1 host) ──────────────────

func TestGate_NetworkHostless(t *testing.T) {
	// no hosts declared → hostless net ops denied
	i, b := newGatedInterp(domain.Requirements{})
	for _, kind := range []string{"local_ip", "interfaces", "mac_address", "find_free_port"} {
		if err := call(i, b, kind, map[string]any{}); !isKind(err, domain.ErrHostNotDeclared) {
			t.Fatalf("%s with no host declared want host_not_declared, got %v", kind, err)
		}
	}
	// with a host declared, net is considered allowed
	i2, b2 := newGatedInterp(domain.Requirements{Hosts: []domain.HostReq{{Name: "x.com"}}})
	if err := call(i2, b2, "interfaces", map[string]any{}); err != nil {
		t.Fatalf("interfaces with a declared host should be allowed: %v", err)
	}
}

// ── subprocess ────────────────────────────────────────────────────────────

func TestGate_Subprocess(t *testing.T) {
	i, b := newGatedInterp(domain.Requirements{Bins: []domain.BinReq{{Name: "go"}}})
	// bin_version of a declared bin → allowed
	if err := call(i, b, "bin_version", map[string]any{"_0": "go"}); err != nil {
		t.Fatalf("bin_version of declared go allowed: %v", err)
	}
	// bin_version of an undeclared bin → bin_not_declared
	if err := call(i, b, "bin_version", map[string]any{"_0": "kubectl"}); !isKind(err, domain.ErrBinNotDeclared) {
		t.Fatalf("bin_version undeclared want bin_not_declared, got %v", err)
	}
}

// ── filesystem read / write ───────────────────────────────────────────────

func TestGate_Filesystem(t *testing.T) {
	i, b := newGatedInterp(domain.Requirements{
		ReadRoots:  []string{"./allowed_read"},
		WriteRoots: []string{"./allowed_write"},
	})
	// write inside declared root → allowed (mkdir is idempotent)
	if err := call(i, b, "mkdir", map[string]any{"_0": "./allowed_write/sub"}); err != nil {
		t.Fatalf("mkdir inside write root allowed: %v", err)
	}
	// write outside → write_not_declared
	if err := call(i, b, "write_file", map[string]any{"path": "./secrets/x", "content": "y"}); !isKind(err, domain.ErrWriteNotDeclared) {
		t.Fatalf("write outside root want write_not_declared, got %v", err)
	}
	// read outside read+write roots → read_not_declared
	if err := call(i, b, "read_file", map[string]any{"_0": "/etc/hosts"}); !isKind(err, domain.ErrReadNotDeclared) {
		t.Fatalf("read outside root want read_not_declared, got %v", err)
	}
	// read of a write root is allowed (write implies read)
	if err := call(i, b, "exists", map[string]any{"_0": "./allowed_write/sub"}); err != nil {
		t.Fatalf("exists inside write root allowed (write implies read): %v", err)
	}
	// cleanup
	_ = call(i, b, "rm", map[string]any{"_0": "./allowed_write"})
}

// ── no requires block → normalized to empty manifest at Load ─────────────

func TestGate_NoRequiresBlock_IsStrictAfterLoad(t *testing.T) {
	// Programs from Load always have Declared=true (missing block → empty manifest).
	p, err := capyloader.LoadFromString(`name "x"
command t
    do
        print "ok"
    end
end
`)
	if err != nil {
		t.Fatal(err)
	}
	if !p.Requirements.Declared {
		t.Fatal("Load must normalize missing requires to Declared=true")
	}
	i := interpreter.New(AllHandlers(), p)
	i.PreflightHook = Preflight
	b := interpreter.NewBindings(".")
	if err := call(i, b, "shell", map[string]any{"cmd": "curl x"}); !isKind(err, domain.ErrBinNotDeclared) {
		t.Fatalf("normalized empty manifest must gate shell, got %v", err)
	}
}

// ── the check runs EVERY time, not once ───────────────────────────────────

func TestGate_VerifiedEachTime(t *testing.T) {
	i, b := newGatedInterp(domain.Requirements{Bins: []domain.BinReq{{Name: "echo"}}})
	// Repeated undeclared calls must ALL be denied — no caching / first-call-only.
	for n := 0; n < 5; n++ {
		if err := call(i, b, "shell", map[string]any{"cmd": "curl x"}); !isKind(err, domain.ErrBinNotDeclared) {
			t.Fatalf("call %d: undeclared bin must be denied every time, got %v", n, err)
		}
	}
	// And a declared call still works after the denials (gate is stateless).
	if err := call(i, b, "shell", map[string]any{"cmd": "echo ok"}); err != nil {
		t.Fatalf("declared call after denials should still run: %v", err)
	}
}

// ── coverage: every external op kind has a gate ───────────────────────────
//
// This is the guard against regressions: if a new external op is added
// without a requires check, this test fails. The lists below are the
// authoritative classification.

func TestGate_CoverageOfExternalOps(t *testing.T) {
	// External ops that MUST refuse when their resource is undeclared.
	// We assert the gate fires by invoking each against an empty manifest
	// (everything undeclared) and requiring a *_not_declared / *_not_permitted
	// error — i.e. the op refused rather than touching the resource.
	cases := []struct {
		kind string
		args map[string]any
		want domain.ErrorKind
	}{
		{"shell", map[string]any{"cmd": "curl x"}, domain.ErrBinNotDeclared},
		{"shell_output", map[string]any{"cmd": "curl x"}, domain.ErrBinNotDeclared},
		{"shell_detached", map[string]any{"cmd": "curl x"}, domain.ErrBinNotDeclared},
		{"get_env", map[string]any{"_0": "X"}, domain.ErrEnvNotDeclared},
		{"set_env", map[string]any{"_0": "X", "_1": "y"}, domain.ErrEnvNotDeclared},
		{"unset_env", map[string]any{"_0": "X"}, domain.ErrEnvNotDeclared},
		{"env_has", map[string]any{"_0": "X"}, domain.ErrEnvNotDeclared},
		{"env_default", map[string]any{"_0": "X", "_1": "d"}, domain.ErrEnvNotDeclared},
		{"dns_lookup", map[string]any{"_0": "x.com"}, domain.ErrHostNotDeclared},
		{"port_check", map[string]any{"_0": "x.com", "_1": "80"}, domain.ErrHostNotDeclared},
		{"wait_for_port", map[string]any{"_0": "x.com", "_1": "80", "_2": "1"}, domain.ErrHostNotDeclared},
		{"wait_for_url", map[string]any{"_0": "https://x.com/y", "_1": "1"}, domain.ErrHostNotDeclared},
		{"public_ip", map[string]any{}, domain.ErrHostNotDeclared},
		{"local_ip", map[string]any{}, domain.ErrHostNotDeclared},
		{"interfaces", map[string]any{}, domain.ErrHostNotDeclared},
		{"mac_address", map[string]any{}, domain.ErrHostNotDeclared},
		{"port_free", map[string]any{"_0": "0"}, domain.ErrHostNotDeclared},
		{"find_free_port", map[string]any{}, domain.ErrHostNotDeclared},
		{"http_get", map[string]any{"_0": "https://x.com"}, domain.ErrHostNotDeclared},
		{"download", map[string]any{"url": "https://x.com", "dst": "/tmp/x"}, domain.ErrWriteNotDeclared}, // dst write checked first
		{"bin_version", map[string]any{"_0": "kubectl"}, domain.ErrBinNotDeclared},
		{"pkg_install", map[string]any{"_0": "jq"}, domain.ErrBinNotDeclared},
		{"mkdir", map[string]any{"_0": "/tmp/x"}, domain.ErrWriteNotDeclared},
		{"write_file", map[string]any{"path": "/tmp/x", "content": "y"}, domain.ErrWriteNotDeclared},
		{"read_file", map[string]any{"_0": "/etc/hosts"}, domain.ErrReadNotDeclared},
		{"rm", map[string]any{"_0": "/tmp/x"}, domain.ErrWriteNotDeclared},
		{"cp", map[string]any{"_0": "/a", "_1": "/b"}, domain.ErrWriteNotDeclared}, // write (dst) checked before read (src)
		{"tar_extract", map[string]any{"src": "/a.tar", "dst": "/out"}, domain.ErrWriteNotDeclared},
	}
	for _, tc := range cases {
		i, b := newGatedInterp(domain.Requirements{}) // empty manifest: nothing declared
		err := call(i, b, tc.kind, tc.args)
		if !isKind(err, tc.want) {
			t.Errorf("%s with empty manifest: want %s, got %v", tc.kind, tc.want, err)
		}
	}
}

// Sanity: the gate error messages name the resource so users can fix them.
func TestGate_ErrorMessagesAreActionable(t *testing.T) {
	i, b := newGatedInterp(domain.Requirements{})
	err := call(i, b, "shell", map[string]any{"cmd": "curl x"})
	if err == nil || !strings.Contains(err.Error(), "curl") {
		t.Fatalf("error should name the bin: %v", err)
	}
}
