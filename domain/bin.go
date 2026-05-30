package domain

import "strings"

// binBase returns the basename of a bin token (strips any path prefix), so a
// declared `go` matches an exec of `/usr/local/go/bin/go` and a declared path
// matches its own basename invocation.
func binBase(s string) string {
	if i := strings.LastIndexAny(s, "/\\"); i >= 0 {
		return s[i+1:]
	}
	return s
}

// BinAllowed reports whether a bin token (the first token of an `exec` / shell
// command, post-interpolation) is permitted by the manifest. A token matches a
// declared bin when it equals the bin's alias, equals its Name exactly (covers
// path-form names like `./bins/tool.exe`), or shares its basename (covers
// PATH-resolved absolute paths and bare-name invocations of a declared path).
func (r Requirements) BinAllowed(token string) bool {
	if token == "" {
		return true
	}
	base := binBase(token)
	for _, b := range r.Bins {
		if b.Alias != "" && token == b.Alias {
			return true
		}
		if token == b.Name {
			return true
		}
		if base == binBase(b.Name) {
			return true
		}
	}
	return false
}

// ResolveBin maps a bin token to the actual executable to spawn. An alias
// resolves to its declared path/name; everything else passes through
// unchanged. Path resolution relative to the script directory happens in the
// op layer (which knows the script path) — this only un-aliases.
func (r Requirements) ResolveBin(token string) string {
	for _, b := range r.Bins {
		if b.Alias != "" && token == b.Alias {
			return b.Name
		}
	}
	return token
}
