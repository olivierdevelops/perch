// Package ops registers all built-in op handlers for the perch
// interpreter. Each category contributes its handlers to AllHandlers.
package ops

import "github.com/luowensheng/perch/infra/interpreter"

// AllHandlers returns a fresh map of every op kind perch knows about.
// The orchestrator hands this to interpreter.New.
func AllHandlers() map[string]interpreter.Handler {
	m := map[string]interpreter.Handler{}
	registerProcess(m)
	registerFlow(m)
	registerContexts(m)
	registerCache(m)
	registerAssertions(m)
	registerWasm(m)
	registerFiles(m)
	registerSystem(m)
	registerCompression(m)
	registerHTTP(m)
	registerHash(m)
	registerStrings(m)
	registerEncoding(m)
	registerTime(m)
	registerRegex(m)
	registerNetwork(m)
	registerArchive(m)
	registerBundle(m)
	registerPaths(m)
	registerInstall(m)
	registerErrorOps(m)
	registerVersion(m)
	registerTextLines(m)
	registerCatalog(m)
	// Wrap filesystem ops so a declared `requires` block gates their
	// read/write paths against the declared roots. No-op without the block.
	ApplyRequiresPathGating(m)
	return m
}
