package domain

import "testing"

func TestBinAllowed(t *testing.T) {
	r := Requirements{Bins: []BinReq{
		{Name: "go"},
		{Name: "./bins/tool.exe", Alias: "tool"},
	}}
	cases := []struct {
		token string
		want  bool
	}{
		{"go", true},                 // exact bare name
		{"/usr/local/go/bin/go", true}, // PATH-resolved → basename match
		{"./bins/tool.exe", true},    // exact path
		{"tool", true},               // alias
		{"tool.exe", true},           // basename of the declared path
		{"docker", false},            // undeclared
		{"", true},                   // empty defers
	}
	for _, c := range cases {
		if got := r.BinAllowed(c.token); got != c.want {
			t.Errorf("BinAllowed(%q) = %v, want %v", c.token, got, c.want)
		}
	}
}

func TestResolveBin(t *testing.T) {
	r := Requirements{Bins: []BinReq{{Name: "./bins/tool.exe", Alias: "tool"}}}
	if got := r.ResolveBin("tool"); got != "./bins/tool.exe" {
		t.Errorf("alias should resolve to path, got %q", got)
	}
	if got := r.ResolveBin("go"); got != "go" {
		t.Errorf("non-alias should pass through, got %q", got)
	}
}
