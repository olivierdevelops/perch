package interpreter

import "testing"

func TestInterpolatePassthrough(t *testing.T) {
	b := NewBindings("/tmp")
	got, err := Interpolate("hello world", b)
	if err != nil {
		t.Fatal(err)
	}
	if got != "hello world" {
		t.Errorf("want %q got %q", "hello world", got)
	}
}

func TestInterpolateSubst(t *testing.T) {
	b := NewBindings("/tmp")
	b.Set("name", "Alice")
	b.Set("n", 7)
	got, err := Interpolate("Hello {{name}} #{{n}}", b)
	if err != nil {
		t.Fatal(err)
	}
	if got != "Hello Alice #7" {
		t.Errorf("got %q", got)
	}
}

func TestInterpolateEnvFallback(t *testing.T) {
	b := NewBindings("/tmp")
	t.Setenv("PERCH_TEST_XYZ", "FROM_ENV")
	got, err := Interpolate("v={{PERCH_TEST_XYZ}}", b)
	if err != nil {
		t.Fatal(err)
	}
	if got != "v=FROM_ENV" {
		t.Errorf("got %q", got)
	}
}

func TestInterpolateUnknown(t *testing.T) {
	b := NewBindings("/tmp")
	if _, err := Interpolate("hi {{nope}}", b); err == nil {
		t.Error("expected error for unknown name")
	}
}

func TestInterpolateUnterminated(t *testing.T) {
	b := NewBindings("/tmp")
	if _, err := Interpolate("hi {{nope", b); err == nil {
		t.Error("expected unterminated error")
	}
}

func TestInterpolateEscape(t *testing.T) {
	// `\{` escapes a single brace; the `{{...}}` opener thus never forms.
	b := NewBindings("/tmp")
	got, err := Interpolate(`literal \{{x}}`, b)
	if err != nil {
		t.Fatal(err)
	}
	if got != "literal {{x}}" {
		t.Errorf("got %q", got)
	}
}

func TestToStringValue(t *testing.T) {
	cases := []struct {
		in   any
		want string
	}{
		{"hi", "hi"},
		{true, "true"},
		{false, "false"},
		{42, "42"},
		{int64(99), "99"},
		{3.14, "3.14"},
		{float64(7), "7"},
		{nil, ""},
	}
	for _, tc := range cases {
		if got := ToStringValue(tc.in); got != tc.want {
			t.Errorf("ToStringValue(%v): want %q got %q", tc.in, tc.want, got)
		}
	}
}
