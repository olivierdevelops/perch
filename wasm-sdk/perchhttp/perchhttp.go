// Package perchhttp is the WASM-side SDK for perch's host-provided HTTP.
//
// Usage from a TinyGo / Go-wasi module:
//
//	import "github.com/olivierdevelops/perch/wasm-sdk/perchhttp"
//
//	body, status, err := perchhttp.Get("https://api.example.com/health")
//	if err != nil { /* policy violation, network error, or out of cap */ }
//	fmt.Printf("got %d, %d bytes\n", status, len(body))
//
// The host (perch) must allow the destination via `wasm_allow_host`
// inside the calling `wasm_run` block. perch also applies the outer
// HTTP policy (SSRF guard, redirect rules, --allow-host) to every
// request and every redirect hop.
//
// This package compiles for `GOOS=wasip1 GOARCH=wasm` (or TinyGo's
// `wasm-unknown` / wazero's WASI). It is NOT usable from native Go;
// the imports it declares resolve to "perch" host functions provided
// at module instantiation.
//
//go:build wasip1
// +build wasip1

package perchhttp

import (
	"errors"
	"unsafe"
)

// ErrRefused is returned when the host refuses the request — either
// because no wasm_allow_host matched, or because the outer HTTPPolicy
// (SSRF / redirect / --allow-host) blocked it. The specific reason is
// kept on the host side and not exposed to the module by design (a
// hostile module shouldn't be able to probe the policy).
var ErrRefused = errors.New("perchhttp: refused by host")

// Get fetches `url` via the perch host's HTTP client. Returns the full
// response body, the status code, and an error. The body is read in
// 32 KB chunks to avoid one huge allocation; the host caps total size
// at 32 MB.
func Get(url string) ([]byte, int, error) {
	urlBytes := []byte(url)
	handle := httpGet(unsafe.Pointer(&urlBytes[0]), uint32(len(urlBytes)))
	if handle < 0 {
		return nil, 0, ErrRefused
	}
	defer httpClose(handle)

	status := int(httpStatus(handle))
	total := httpBodyLen(handle)
	if total < 0 {
		return nil, status, ErrRefused
	}
	body := make([]byte, total)
	read := 0
	const chunk = 32 * 1024
	for read < int(total) {
		want := int(total) - read
		if want > chunk {
			want = chunk
		}
		n := httpReadBody(handle, unsafe.Pointer(&body[read]), uint32(want))
		if n <= 0 {
			break
		}
		read += int(n)
	}
	return body[:read], status, nil
}

// Raw imports. The function names match what installPerchHostModule
// exports in infra/ops/wasm_http.go.

//go:wasmimport perch http_get
//go:noescape
func httpGet(urlPtr unsafe.Pointer, urlLen uint32) int32

//go:wasmimport perch http_status
//go:noescape
func httpStatus(handle int32) int32

//go:wasmimport perch http_body_len
//go:noescape
func httpBodyLen(handle int32) int32

//go:wasmimport perch http_read_body
//go:noescape
func httpReadBody(handle int32, dstPtr unsafe.Pointer, dstCap uint32) int32

//go:wasmimport perch http_close
//go:noescape
func httpClose(handle int32) int32
