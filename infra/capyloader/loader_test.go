package capyloader

import (
	"os"
	"strings"
	"testing"

	"github.com/luowensheng/perch/domain"
)

func TestLoadMinimal(t *testing.T) {
	src := `name "app"
about "hi"
version "0.1"
requires
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	if p.Name != "app" {
		t.Errorf("Name: want %q got %q", "app", p.Name)
	}
	if p.Description != "hi" {
		t.Errorf("Description: want %q got %q", "hi", p.Description)
	}
	if p.Version != "0.1" {
		t.Errorf("Version: want %q got %q", "0.1", p.Version)
	}
}

// Bare top-level `NAME = value` bindings replace the removed globals block.
func TestBareTopLevelBindings(t *testing.T) {
	src := `name "x"
BUILD_DIR = "./out"
flag = true
requires
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	got := map[string]string{}
	for _, b := range p.Globals.Bindings {
		got[b.Name] = b.Type
	}
	if got["BUILD_DIR"] != "string" || got["flag"] != "bool" {
		t.Errorf("bare bindings not collected: %+v", got)
	}
}

// An interpolated exec bin (`exec ${tool} …`) can't be resolved statically,
// so the load-time gate must SKIP it (defer to the runtime guard) rather than
// flag bin_not_declared — consistent with how requires treats interpolated
// hosts/paths.
func TestInterpolatedExecBinSkipsStaticGate(t *testing.T) {
	src := `name "x"
TOOL = "echo"
requires
end
command t
    do
        exec ${TOOL} hi
        exec ${HOME}/bin/thing run
    end
end
`
	if _, err := LoadFromString(src); err != nil {
		t.Fatalf("interpolated exec bins must not be gated at load, got %v", err)
	}
}

// `bin "PATH" as NAME` parses into a BinReq carrying both the path and the
// alias handle (+ the optional variant).
func TestBinPathAlias(t *testing.T) {
	src := `name "x"
requires
    bin "./bins/tool.exe" as tool
    bin "${script_dir}/helper" as helper optional
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	bins := p.Requirements.Bins
	if len(bins) != 2 {
		t.Fatalf("want 2 bins, got %d (%+v)", len(bins), bins)
	}
	if bins[0].Name != "./bins/tool.exe" || bins[0].Alias != "tool" {
		t.Errorf("bin0 wrong: %+v", bins[0])
	}
	if bins[1].Alias != "helper" || !bins[1].Optional {
		t.Errorf("bin1 should be aliased+optional: %+v", bins[1])
	}
}

// The removed `globals` block must yield a clear migration error.
func TestGlobalsBlockRemoved(t *testing.T) {
	src := `name "x"
globals
    a = 1
end
requires
end
`
	_, err := LoadFromString(src)
	if err == nil || !strings.Contains(err.Error(), "globals") {
		t.Fatalf("expected a globals-removed migration error, got %v", err)
	}
}

// Bare-name requires forms: `write DIR` / `read SRC` resolve a binding.
func TestRequiresByName(t *testing.T) {
	src := `name "x"
DIR = "./out"
SRC = "./cmd"
requires
    write DIR
    read SRC
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	if len(p.Requirements.WriteRoots) != 1 || p.Requirements.WriteRoots[0] != "${DIR}" {
		t.Errorf("write-by-name root wrong: %+v", p.Requirements.WriteRoots)
	}
	if len(p.Requirements.ReadRoots) != 1 || p.Requirements.ReadRoots[0] != "${SRC}" {
		t.Errorf("read-by-name root wrong: %+v", p.Requirements.ReadRoots)
	}
}

func TestGlobals(t *testing.T) {
	src := `name "x"
requires
end
flag = true
n = 42
s = "hello"
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]string{"flag": "bool", "n": "int", "s": "string"}
	got := map[string]string{}
	for _, b := range p.Globals.Bindings {
		got[b.Name] = b.Type
	}
	for k, v := range want {
		if got[k] != v {
			t.Errorf("global %q: type want %q got %q", k, v, got[k])
		}
	}
}

func TestCommandWithArgsAndOps(t *testing.T) {
	src := `name "x"
requires
end
command build
    description "compile"
    arg target
        type string
        default "darwin"
        description "Target OS"
    end
    private
    do
        print "Building ${target}"
        shell "go build"
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	c, ok := p.Commands["build"]
	if !ok {
		t.Fatal("command build missing")
	}
	if c.Description != "compile" {
		t.Errorf("Description: want %q got %q", "compile", c.Description)
	}
	if !c.Modifiers.Private {
		t.Error("Private modifier not set")
	}
	if len(c.Args) != 1 || c.Args[0].Name != "target" || c.Args[0].Type != "string" {
		t.Errorf("Args: %+v", c.Args)
	}
	if !c.Args[0].HasDefault || c.Args[0].Default != "darwin" {
		t.Errorf("default: hasDefault=%v default=%v", c.Args[0].HasDefault, c.Args[0].Default)
	}
	if c.Args[0].Description != "Target OS" {
		t.Errorf("arg desc: %q", c.Args[0].Description)
	}
	if len(c.Ops) != 2 {
		t.Fatalf("Ops: want 2 got %d", len(c.Ops))
	}
	if c.Ops[0].Kind != "print" || c.Ops[1].Kind != "shell" {
		t.Errorf("Op kinds: %s, %s", c.Ops[0].Kind, c.Ops[1].Kind)
	}
}

func TestArgBlockFields(t *testing.T) {
	src := `name "x"
requires
end
command t
    arg count
        type int
        default 3
        description "Iterations"
    end
    arg path
        type string
        index 0
        description "Input file"
    end
    arg port
        type int
        optional
        description "Override default port"
    end
    do
        print ""
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	args := p.Commands["t"].Args
	if len(args) != 3 {
		t.Fatalf("want 3 args, got %d", len(args))
	}
	if args[0].Name != "count" || args[0].Type != "int" {
		t.Errorf("count arg: %+v", args[0])
	}
	if !args[0].HasDefault {
		t.Errorf("count: missing default flag")
	}
	if args[1].Index == nil || *args[1].Index != 0 {
		t.Errorf("path: want index=0 got %v", args[1].Index)
	}
	if !args[2].Optional {
		t.Errorf("port: not optional")
	}
}

func TestArgMissingTypeIsError(t *testing.T) {
	src := `name "x"
command t
    arg foo
        default "bar"
    end
    do
        print ""
    end
end
`
	if _, err := LoadFromString(src); err == nil {
		t.Error("expected error for arg without type")
	}
}

func TestNestedBlockOps(t *testing.T) {
	src := `name "x"
requires
end
command setup
    do
        if os == "darwin"
            shell "brew install jq"
        end
        if os == "linux"
            shell "apt install jq"
        end
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	c := p.Commands["setup"]
	if len(c.Ops) != 2 {
		t.Fatalf("Ops: want 2 got %d", len(c.Ops))
	}
	for i, op := range c.Ops {
		if op.Kind != "if" {
			t.Errorf("ops[%d].Kind: want if got %s", i, op.Kind)
		}
		if len(op.Body) != 1 || op.Body[0].Kind != "shell" {
			t.Errorf("ops[%d].Body: want one shell, got %+v", i, op.Body)
		}
		if op.Args["op"] != "eq" || op.Args["lhs"] != "os" {
			t.Errorf("ops[%d].Args: %+v", i, op.Args)
		}
	}
}

func TestLetCapture(t *testing.T) {
	src := `name "x"
requires
end
command t
    do
        let h = sha256_file "./bin"
        let report = json_count "{}" "[?]"
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	c := p.Commands["t"]
	if len(c.Ops) != 2 {
		t.Fatalf("want 2 ops, got %d", len(c.Ops))
	}
	if c.Ops[0].Kind != "sha256_file" || c.Ops[0].CaptureInto != "h" {
		t.Errorf("op[0]: %+v", c.Ops[0])
	}
	if c.Ops[1].Kind != "json_count" || c.Ops[1].CaptureInto != "report" {
		t.Errorf("op[1]: %+v", c.Ops[1])
	}
}

func TestCatch(t *testing.T) {
	src := `name "x"
requires
end
catch unknown
    description "fallback"
    do
        print "no such: ${unknown}"
        exit 1
    end
end
`
	p, err := LoadFromString(src)
	if err != nil {
		t.Fatal(err)
	}
	if p.Catch == nil {
		t.Fatal("catch missing")
	}
	if p.Catch.Bind != "unknown" {
		t.Errorf("Bind: want unknown, got %q", p.Catch.Bind)
	}
	if p.Catch.Description != "fallback" {
		t.Errorf("Description: %q", p.Catch.Description)
	}
	if len(p.Catch.Ops) != 2 {
		t.Errorf("ops len: %d", len(p.Catch.Ops))
	}
}

func TestParseError(t *testing.T) {
	src := `command foo
    blorp_unknown "x"
end
`
	if _, err := LoadFromString(src); err == nil {
		t.Error("expected parse error for unknown function")
	}
}

func TestImportFlat(t *testing.T) {
	dir := t.TempDir()
	mustWrite := func(name, src string) string {
		p := dir + "/" + name
		if err := os.WriteFile(p, []byte(src), 0644); err != nil {
			t.Fatal(err)
		}
		return p
	}
	mustWrite("child.perch", `name "child"
command imported_cmd
    description "from child.perch"
    do
        print "ok"
    end
end
`)
	main := mustWrite("main.perch", `name "main"
requires
end
import "./child.perch"
command local_cmd
    description "from main.perch"
    do
        print "local"
    end
end
`)
	p, err := Load(main)
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := p.Commands["imported_cmd"]; !ok {
		t.Errorf("flat import: imported_cmd missing; got %v", keysOf(p.Commands))
	}
	if _, ok := p.Commands["local_cmd"]; !ok {
		t.Errorf("local_cmd missing; got %v", keysOf(p.Commands))
	}
}

func TestImportAliased(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(dir+"/aws.perch", []byte(`name "aws"
command upload
    description "u"
    do
        print "u"
    end
end
`), 0644); err != nil {
		t.Fatal(err)
	}
	mainPath := dir + "/main.perch"
	if err := os.WriteFile(mainPath, []byte(`name "main"
requires
end
import "./aws.perch" as aws
command deploy
    description "d"
    do
        run aws.upload
    end
end
`), 0644); err != nil {
		t.Fatal(err)
	}
	p, err := Load(mainPath)
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := p.Commands["aws.upload"]; !ok {
		t.Errorf("aliased import: aws.upload missing; got %v", keysOf(p.Commands))
	}
	if _, ok := p.Commands["upload"]; ok {
		t.Errorf("aliased import: bare 'upload' should NOT be present")
	}
}

func TestImportCycle(t *testing.T) {
	dir := t.TempDir()
	os.WriteFile(dir+"/a.perch", []byte(`name "a"
import "./b.perch"
command ca
    description "ca"
    do
        print "a"
    end
end
`), 0644)
	os.WriteFile(dir+"/b.perch", []byte(`name "b"
import "./a.perch"
command cb
    description "cb"
    do
        print "b"
    end
end
`), 0644)
	_, err := Load(dir + "/a.perch")
	if err == nil {
		t.Fatal("expected cycle error, got nil")
	}
	if !contains(err.Error(), "cycle") {
		t.Errorf("expected cycle in error; got %v", err)
	}
}

func TestImportConflict(t *testing.T) {
	dir := t.TempDir()
	os.WriteFile(dir+"/lib.perch", []byte(`name "lib"
command deploy
    description "from lib"
    do
        print "lib"
    end
end
`), 0644)
	os.WriteFile(dir+"/main.perch", []byte(`name "main"
import "./lib.perch"
command deploy
    description "from main"
    do
        print "main"
    end
end
`), 0644)
	_, err := Load(dir + "/main.perch")
	if err == nil {
		t.Fatal("expected conflict error, got nil")
	}
	if !contains(err.Error(), "already declared") {
		t.Errorf("expected 'already declared' in error; got %v", err)
	}
}

func keysOf(m map[string]*domain.Command) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
}

func contains(s, sub string) bool {
	return strings.Contains(s, sub)
}

func TestImportFileDir(t *testing.T) {
	dir := t.TempDir()
	subDir := dir + "/shared"
	os.MkdirAll(subDir, 0755)
	os.WriteFile(subDir+"/lib.perch", []byte(`name "lib"
command from_lib
    description "from shared/lib.perch"
    do
        print "lib"
    end
end
`), 0644)
	os.WriteFile(dir+"/main.perch", []byte(`name "main"
requires
end
import "${file_dir}/shared/lib.perch"
command main_cmd
    description "main"
    do
        run from_lib
    end
end
`), 0644)
	p, err := Load(dir + "/main.perch")
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if _, ok := p.Commands["from_lib"]; !ok {
		t.Errorf("from_lib missing after ${file_dir} expansion; got %v", keysOf(p.Commands))
	}
}

func TestImportEnvVar(t *testing.T) {
	dir := t.TempDir()
	os.WriteFile(dir+"/lib.perch", []byte(`name "lib"
command e
    description "e"
    do
        print "e"
    end
end
`), 0644)
	t.Setenv("PERCH_TEST_DIR", dir)
	mainPath := dir + "/main.perch"
	os.WriteFile(mainPath, []byte(`name "main"
requires
end
import "${PERCH_TEST_DIR}/lib.perch"
command m
    description "m"
    do
        run e
    end
end
`), 0644)
	p, err := Load(mainPath)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if _, ok := p.Commands["e"]; !ok {
		t.Errorf("e missing after env-var expansion; got %v", keysOf(p.Commands))
	}
}

func TestImportUnknownVar(t *testing.T) {
	dir := t.TempDir()
	os.WriteFile(dir+"/main.perch", []byte(`name "main"
import "${this_var_does_not_exist}/lib.perch"
command m
    description "m"
    do
        print "m"
    end
end
`), 0644)
	_, err := Load(dir + "/main.perch")
	if err == nil {
		t.Fatal("expected error for unknown placeholder, got nil")
	}
	if !contains(err.Error(), "this_var_does_not_exist") {
		t.Errorf("expected error to name the placeholder; got %v", err)
	}
}
