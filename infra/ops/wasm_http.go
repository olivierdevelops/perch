// Host-provided HTTP for WASM modules.
//
// WASI Preview 1 has no network. Rather than wait for Preview 2 / wazero's
// sockets surface, we provide HTTP via wazero host imports under the
// module name "perch". The module's developer pulls in a tiny SDK
// (`wasm-sdk/perchhttp` for Go; equivalent stubs for other languages)
// that wraps the raw imports as `perch.Get(url) ([]byte, error)`.
//
// Capability gating composes cleanly:
//
//  1. `wasm_allow_host "HOST"` declarations in the wasm_run body —
//     per-module allowlist. NO declaration = no network for this module
//     (every URL refused at the host-function entry).
//  2. The outer interpreter HTTPPolicy (--allow-host, --no-redirects,
//     --allow-private-ips, --max-redirects, --allow-scheme-downgrade) —
//     applies via the shared `newHTTPClient` / `validateRequestURL`.
//
// The intersection wins: a host must be allowed by BOTH the wasm_run
// body AND the outer policy. SSRF guard runs on every call + every
// redirect hop, same as native `http_get`.
//
// Wazero's host modules are runtime-scoped (installed once on the
// shared `wasmRuntime`). The per-call policy + per-call handle table
// are threaded through via context.Context — wazero passes the same
// context into every host function, so a `wasm_run` invocation:
//
//   1. builds an `httpCallState` with the call's allowed hosts + a
//      fresh handle table
//   2. attaches it to ctx via context.WithValue(ctx, httpStateKey{}, …)
//   3. passes that ctx to wasmRuntime.InstantiateModule
//
// Host functions retrieve the state via stateFromCtx(ctx). When no
// state is in context (e.g. a module that doesn't declare
// `wasm_allow_host` calls http_get anyway), every call returns -1.
package ops

import (
	"context"
	"io"
	"net/http"
	"net/url"
	"sync"
	"sync/atomic"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"

	"github.com/luowensheng/perch/infra/interpreter"
)

// httpStateKey is the context key for httpCallState. Unexported to
// prevent accidental collisions in callers.
type httpStateKey struct{}

type httpCallState struct {
	policy  interpreter.HTTPPolicy
	enabled bool // false → every http_get returns -1

	handlesMu sync.Mutex
	handles   map[int32]*httpHandle
	nextID    int32
}

type httpHandle struct {
	statusCode int
	body       []byte
	pos        int
}

func (s *httpCallState) put(h *httpHandle) int32 {
	id := atomic.AddInt32(&s.nextID, 1)
	s.handlesMu.Lock()
	s.handles[id] = h
	s.handlesMu.Unlock()
	return id
}
func (s *httpCallState) get(id int32) *httpHandle {
	s.handlesMu.Lock()
	defer s.handlesMu.Unlock()
	return s.handles[id]
}
func (s *httpCallState) del(id int32) {
	s.handlesMu.Lock()
	delete(s.handles, id)
	s.handlesMu.Unlock()
}

func stateFromCtx(ctx context.Context) *httpCallState {
	if v, ok := ctx.Value(httpStateKey{}).(*httpCallState); ok {
		return v
	}
	return nil
}

// buildHTTPCallState constructs the per-call state for a wasm_run
// invocation. moduleAllowedHosts comes from `wasm_allow_host` body
// declarations. When empty, HTTP is disabled for this call (every
// http_get returns -1).
func buildHTTPCallState(i *interpreter.Interpreter, moduleAllowedHosts []string) *httpCallState {
	s := &httpCallState{
		handles: map[int32]*httpHandle{},
		nextID:  0,
	}
	if len(moduleAllowedHosts) == 0 {
		s.enabled = false
		return s
	}
	s.enabled = true
	s.policy = httpPolicy(i)
	// Intersect with outer policy if the outer also restricts hosts.
	if len(s.policy.AllowedHosts) == 0 {
		s.policy.AllowedHosts = append([]string{}, moduleAllowedHosts...)
	} else {
		intersected := intersectHosts(s.policy.AllowedHosts, moduleAllowedHosts)
		if len(intersected) == 0 {
			// Empty intersection — module asks for hosts the outer policy
			// forbids. Disable rather than silently allow nothing.
			s.enabled = false
		}
		s.policy.AllowedHosts = intersected
	}
	return s
}

// installPerchHostModule installs the "perch" host module ONCE on the
// shared wazero runtime. Host functions read per-call state from the
// context (which wazero threads through every invocation).
func installPerchHostModule(ctx context.Context, runtime wazero.Runtime) error {
	_, err := runtime.NewHostModuleBuilder("perch").
		// http_get(url_ptr, url_len) → handle | -1 on error.
		// -1 also when http isn't enabled for this call (no
		// wasm_allow_host declarations, or empty allow-intersection).
		NewFunctionBuilder().WithFunc(func(ctx context.Context, mod api.Module, urlPtr, urlLen uint32) int32 {
			s := stateFromCtx(ctx)
			if s == nil || !s.enabled {
				return -1
			}
			rawURL, ok := readString(mod, urlPtr, urlLen)
			if !ok {
				return -1
			}
			body, status, err := doHTTPGet(ctx, s.policy, rawURL)
			if err != nil {
				return -1
			}
			return s.put(&httpHandle{statusCode: status, body: body})
		}).Export("http_get").

		// http_status(handle) → status code (or 0 if unknown handle)
		NewFunctionBuilder().WithFunc(func(ctx context.Context, _ api.Module, h int32) int32 {
			s := stateFromCtx(ctx)
			if s == nil {
				return 0
			}
			if rec := s.get(h); rec != nil {
				return int32(rec.statusCode)
			}
			return 0
		}).Export("http_status").

		// http_body_len(handle) → total response body length (-1 if unknown)
		NewFunctionBuilder().WithFunc(func(ctx context.Context, _ api.Module, h int32) int32 {
			s := stateFromCtx(ctx)
			if s == nil {
				return -1
			}
			if rec := s.get(h); rec != nil {
				return int32(len(rec.body))
			}
			return -1
		}).Export("http_body_len").

		// http_read_body(handle, dst_ptr, dst_cap) → bytes copied (or -1).
		// Reads up to dst_cap bytes from the response body, advancing
		// the handle's read cursor. Returns 0 when fully consumed.
		NewFunctionBuilder().WithFunc(func(ctx context.Context, mod api.Module, h int32, dstPtr, dstCap uint32) int32 {
			s := stateFromCtx(ctx)
			if s == nil {
				return -1
			}
			rec := s.get(h)
			if rec == nil {
				return -1
			}
			remaining := rec.body[rec.pos:]
			n := uint32(len(remaining))
			if n > dstCap {
				n = dstCap
			}
			if !mod.Memory().Write(dstPtr, remaining[:n]) {
				return -1
			}
			rec.pos += int(n)
			return int32(n)
		}).Export("http_read_body").

		// http_close(handle) → 0 success, -1 unknown
		NewFunctionBuilder().WithFunc(func(ctx context.Context, _ api.Module, h int32) int32 {
			s := stateFromCtx(ctx)
			if s == nil {
				return -1
			}
			if s.get(h) == nil {
				return -1
			}
			s.del(h)
			return 0
		}).Export("http_close").

		Instantiate(ctx)
	return err
}

func readString(mod api.Module, ptr, ln uint32) (string, bool) {
	buf, ok := mod.Memory().Read(ptr, ln)
	if !ok {
		return "", false
	}
	return string(buf), true
}

// doHTTPGet runs one HTTP GET through perch's secure HTTP path —
// SSRF + redirect + host allowlist enforcement — and returns the full
// body + status. The whole body is buffered in memory; suitable for
// the API-shaped responses WASM modules typically consume. 32 MB cap
// keeps a misbehaving response from exhausting host memory.
func doHTTPGet(ctx context.Context, policy interpreter.HTTPPolicy, rawURL string) ([]byte, int, error) {
	u, err := url.Parse(rawURL)
	if err != nil {
		return nil, 0, err
	}
	if err := validateRequestURL(u, policy); err != nil {
		return nil, 0, err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, rawURL, nil)
	if err != nil {
		return nil, 0, err
	}
	resp, err := newHTTPClient(policy).Do(req)
	if err != nil {
		return nil, 0, err
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, 32<<20)) // 32 MB cap
	if err != nil {
		return nil, resp.StatusCode, err
	}
	return body, resp.StatusCode, nil
}

func intersectHosts(a, b []string) []string {
	in := map[string]bool{}
	for _, x := range a {
		in[x] = true
	}
	var out []string
	for _, y := range b {
		if in[y] {
			out = append(out, y)
		}
	}
	return out
}
