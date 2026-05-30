package capyloader

import (
	"strings"
	"testing"

	"github.com/olivierdevelops/perch/domain"
)

// TestRequiresBlockParsing verifies the loader hydrates a full `requires`
// block into domain.Requirements: bins (plain / optional / hash / hash_file),
// env (plain / optional), host, read/write filesystem scopes, os, arch.
func TestRequiresBlockParsing(t *testing.T) {
	src := `name "app"

requires
    bin "git"
    bin "docker" optional
    bin "kubectl"
        hash "sha256:abc123"
    end
    bin "internal-tool"
        hash_file "bundle:checksums/tool.sha256"
    end
    env "HOME"
    env "DEBUG" optional
    host "api.github.com"
    host "*.amazonaws.com"
    read  "./src"
    read  "./config"
    write "./build"
    os   "linux"
    os   "darwin"
    arch "amd64"
end

command t
    do
        print "hi"
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	r := p.Requirements
	if !r.Declared {
		t.Fatal("Requirements.Declared should be true")
	}

	// bins
	if len(r.Bins) != 4 {
		t.Fatalf("bins: want 4, got %d (%+v)", len(r.Bins), r.Bins)
	}
	byName := map[string]int{}
	for i, b := range r.Bins {
		byName[b.Name] = i
	}
	if r.Bins[byName["docker"]].Optional != true {
		t.Error("docker should be optional")
	}
	if got := r.Bins[byName["kubectl"]].Hash; got != "sha256:abc123" {
		t.Errorf("kubectl hash: want sha256:abc123, got %q", got)
	}
	if got := r.Bins[byName["internal-tool"]].HashFile; got != "bundle:checksums/tool.sha256" {
		t.Errorf("internal-tool hash_file: got %q", got)
	}

	// Version feature removed: BinReq now only carries Name/Optional/Hash/HashFile.
	// (The struct no longer has Op/VersionRequired/Probe/Regex fields — enforced
	// at compile time. Here we just sanity-check the surviving fields.)
	if r.Bins[byName["git"]].Hash != "" || r.Bins[byName["git"]].HashFile != "" {
		t.Errorf("git should carry no hash metadata: %+v", r.Bins[byName["git"]])
	}

	// env
	if len(r.Envs) != 2 {
		t.Fatalf("envs: want 2, got %d", len(r.Envs))
	}
	var debugOptional bool
	for _, e := range r.Envs {
		if e.Name == "DEBUG" {
			debugOptional = e.Optional
		}
	}
	if !debugOptional {
		t.Error("DEBUG env should be optional")
	}

	// hosts
	if len(r.Hosts) != 2 {
		t.Fatalf("hosts: want 2, got %d", len(r.Hosts))
	}

	// fs scopes
	if len(r.ReadRoots) != 2 || r.ReadRoots[0] != "./src" {
		t.Errorf("read roots: got %v", r.ReadRoots)
	}
	if len(r.WriteRoots) != 1 || r.WriteRoots[0] != "./build" {
		t.Errorf("write roots: got %v", r.WriteRoots)
	}

	// os / arch
	if len(r.OS) != 2 || len(r.Arch) != 1 {
		t.Errorf("os=%v arch=%v", r.OS, r.Arch)
	}
}

// TestNoRequiresBlockIsNormalized confirms a file without a `requires`
// block loads as if it had an empty `requires`/`end`: Declared==true, no
// bins, pure ops allowed.
func TestNoRequiresBlockIsNormalized(t *testing.T) {
	p, err := LoadFromString("name \"x\"\ncommand t\n    do\n        print \"hi\"\n    end\nend\n")
	if err != nil {
		t.Fatal(err)
	}
	if !p.Requirements.Declared {
		t.Error("missing requires must normalize to Declared=true")
	}
	if len(p.Requirements.Bins) != 0 {
		t.Errorf("implicit empty manifest must declare no bins, got %v", p.Requirements.Bins)
	}
}

// A file with no requires block rejects undeclared exec at load (same as empty).
func TestNoRequiresBlockRejectsUndeclaredExec(t *testing.T) {
	_, err := LoadFromString(`name "x"
command t
    do
        exec echo hi
    end
end
`)
	oe, ok := err.(*domain.OpError)
	if !ok || oe.Kind != domain.ErrBinNotDeclared {
		t.Fatalf("want bin_not_declared at load, got %v", err)
	}
}

// An empty `requires`/`end` block is the legal "pure ops only" manifest: it
// loads cleanly and marks the program Declared with no spawnable surface.
func TestEmptyRequiresBlockIsDeclared(t *testing.T) {
	p, err := LoadFromString("name \"x\"\nrequires\nend\ncommand t\n    do\n        print \"hi\"\n    end\nend\n")
	if err != nil {
		t.Fatal(err)
	}
	if !p.Requirements.Declared {
		t.Error("an empty requires block must set Declared=true")
	}
	if len(p.Requirements.Bins) != 0 {
		t.Errorf("empty requires must declare no bins, got %v", p.Requirements.Bins)
	}
}

// TestVersionComparatorInRequiresRejected confirms the removed version
// feature is gone: `bin "x" >= "1.0.0"` no longer parses as a requires entry.
func TestVersionComparatorInRequiresRejected(t *testing.T) {
	src := `name "x"
requires
    bin "git" >= "2.0.0"
end
command t
    do
        print "hi"
    end
end
`
	_, err := LoadFromString(src)
	if err == nil {
		t.Fatal("expected a parse error for the removed version-comparator syntax, got nil")
	}
	if !strings.Contains(err.Error(), "parse") && !strings.Contains(err.Error(), "library function") {
		t.Logf("note: rejected with: %v", err) // any parse-time rejection is acceptable
	}
}
