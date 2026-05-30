package ops_test

import (
	"testing"

	"github.com/olivierdevelops/perch/infra/capyloader"
	"github.com/olivierdevelops/perch/infra/ops"
)

// TestOpKindsInSync guards the canonical op-kind list the loader embeds
// (infra/capyloader/opkinds.txt) against drift from the handler registry.
//
// The loader needs this list to tell a bare capture's leading name apart —
// `let s = sha256 "x"` (a built-in op) vs `let r = docker ps` (a declared
// bin) — at load time, before any interpreter handler map exists. Most
// value-returning ops have no dedicated grammar keyword, so the grammar-derived
// vocabulary can't supply them; the embedded list does.
//
// If this fails, regenerate the file:
//
//	go run ./internal/genopkinds  (or: print ops.BuiltinKinds() one per line)
//	> infra/capyloader/opkinds.txt
func TestOpKindsInSync(t *testing.T) {
	want := map[string]bool{}
	for _, k := range ops.BuiltinKinds() {
		want[k] = true
	}
	got := map[string]bool{}
	for _, k := range capyloader.OpKinds() {
		got[k] = true
	}

	for k := range want {
		if !got[k] {
			t.Errorf("op %q is a registered handler but missing from infra/capyloader/opkinds.txt — add it (captures of this op would be misread as a bin)", k)
		}
	}
	for k := range got {
		if !want[k] {
			t.Errorf("op %q is in infra/capyloader/opkinds.txt but is NOT a registered handler — remove it (stale entry)", k)
		}
	}
}
