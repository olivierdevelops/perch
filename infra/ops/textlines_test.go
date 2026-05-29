package ops

import "testing"

const sample = "apple\nbanana\napple\ncherry\n"

func TestGrepReject(t *testing.T) {
	got, err := opGrep(nil, nil, map[string]any{"_0": "a", "_1": sample})
	if err != nil {
		t.Fatal(err)
	}
	if got != "apple\nbanana\napple" {
		t.Errorf("grep a: got %q", got)
	}
	got2, _ := opReject(nil, nil, map[string]any{"_0": "a", "_1": sample})
	if got2 != "cherry" {
		t.Errorf("reject a: got %q", got2)
	}
}

func TestHeadTail(t *testing.T) {
	h, _ := opHead(nil, nil, map[string]any{"_0": 2, "_1": sample})
	if h != "apple\nbanana" {
		t.Errorf("head 2: got %q", h)
	}
	tl, _ := opTail(nil, nil, map[string]any{"_0": 2, "_1": sample})
	if tl != "apple\ncherry" {
		t.Errorf("tail 2: got %q", tl)
	}
}

func TestSortUniqCount(t *testing.T) {
	s, _ := opSortLines(nil, nil, map[string]any{"_0": sample})
	if s != "apple\napple\nbanana\ncherry" {
		t.Errorf("sort: got %q", s)
	}
	u, _ := opUniqLines(nil, nil, map[string]any{"_0": s})
	if u != "apple\nbanana\ncherry" {
		t.Errorf("uniq after sort: got %q", u)
	}
	n, _ := opCountLines(nil, nil, map[string]any{"_0": sample})
	if n != "4" {
		t.Errorf("count: got %q", n)
	}
}

func TestCut(t *testing.T) {
	text := "alice 30 nyc\nbob 25 sf\n"
	c, _ := opCut(nil, nil, map[string]any{"_0": 2, "_1": text})
	if c != "30\n25" {
		t.Errorf("cut 2: got %q", c)
	}
}
