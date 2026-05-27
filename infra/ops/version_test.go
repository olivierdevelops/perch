package ops

import "testing"

func TestVersionCompare(t *testing.T) {
	cases := []struct {
		a, b string
		want int
	}{
		// Identity
		{"1.0.0", "1.0.0", 0},
		{"v1.0.0", "1.0.0", 0},
		{"1.0", "1.0.0", 0},

		// Strictly numeric
		{"1.2.3", "1.2.4", -1},
		{"1.2.4", "1.2.3", +1},
		{"1.10.0", "1.9.0", +1}, // numeric, not lex
		{"2.0.0", "1.99.99", +1},

		// v prefix doesn't change result
		{"v1.29.3", "v1.28.0", +1},

		// Pre-release: tail-less > tailed
		{"1.0.0", "1.0.0-rc.1", +1},
		{"1.0.0-rc.1", "1.0.0", -1},
		{"1.0.0-rc.1", "1.0.0-rc.2", -1},

		// Build metadata ignored
		{"1.0.0+build1", "1.0.0+build2", 0},

		// Mixed segment counts
		{"3.14", "3.14.0", 0},
		{"3.14.1", "3.14", +1},
	}
	for _, c := range cases {
		got := versionCompare(c.a, c.b)
		if got != c.want {
			t.Errorf("versionCompare(%q, %q) = %d, want %d", c.a, c.b, got, c.want)
		}
	}
}

func TestSplitVersion(t *testing.T) {
	cases := []struct {
		in        string
		wantParts []int
		wantTail  string
	}{
		{"1.2.3", []int{1, 2, 3}, ""},
		{"v1.2.3", []int{1, 2, 3}, ""},
		{"1.0.0-rc.1", []int{1, 0, 0}, "rc.1"},
		{"v20.10.0+build123", []int{20, 10, 0}, ""},
		{"  v3.14 ", []int{3, 14}, ""},
		{"not a version", nil, ""},
	}
	for _, c := range cases {
		pts, tail := splitVersion(c.in)
		if !sameInts(pts, c.wantParts) || tail != c.wantTail {
			t.Errorf("splitVersion(%q) = %v, %q ; want %v, %q",
				c.in, pts, tail, c.wantParts, c.wantTail)
		}
	}
}

func sameInts(a, b []int) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
