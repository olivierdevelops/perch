package capyloader

import (
	"fmt"
	"reflect"
	"testing"

	"github.com/olivierdevelops/perch/domain"
)

// Quotes are OPTIONAL on every scalar value in the surface grammar. A bare
// token (`name redis`, `bin docker`, `default 6379`, `print hello`) and the
// quoted form (`name "redis"`, …) must parse to the SAME program. These tests
// pin that equivalence so a future grammar change can't silently make quotes
// mandatory again.

const quotedProgram = `name    "myapp"
about   "Manage things"
version "1.2.3"

BUILD_DIR = "./out"
PORT      = 8080
VERBOSE   = true

requires
    bin  "docker"
    env  "HOME"
    host "api.github.com"
    read  "./config"
    write "./out"
end

command start
    description "Start the thing"
    arg port
        type int
        default 8080
    end
    arg label
        type string
        default "web"
    end
    arg debug
        type bool
        default false
    end
    do
        mkdir "./out"
        cp "./a" "./b"
    end
end
`

const bareProgram = `name    myapp
about   "Manage things"
version 1.2.3

BUILD_DIR = ./out
PORT      = 8080
VERBOSE   = true

requires
    bin  docker
    env  HOME
    host api.github.com
    read  ./config
    write ./out
end

command start
    description "Start the thing"
    arg port
        type int
        default 8080
    end
    arg label
        type string
        default web
    end
    arg debug
        type bool
        default false
    end
    do
        mkdir ./out
        cp ./a ./b
    end
end
`

func TestQuotesOptional_BareEqualsQuoted(t *testing.T) {
	q, err := LoadFromString(quotedProgram)
	if err != nil {
		t.Fatalf("quoted load: %v", err)
	}
	b, err := LoadFromString(bareProgram)
	if err != nil {
		t.Fatalf("bare load: %v", err)
	}

	// Top-level scalars.
	if q.Name != b.Name || q.Name != "myapp" {
		t.Errorf("name: quoted=%q bare=%q", q.Name, b.Name)
	}
	if q.Version != b.Version || q.Version != "1.2.3" {
		t.Errorf("version: quoted=%q bare=%q", q.Version, b.Version)
	}
	if q.Description != b.Description {
		t.Errorf("about: quoted=%q bare=%q", q.Description, b.Description)
	}

	// Bare top-level bindings: same names, values, AND inferred types
	// (PORT→int, VERBOSE→bool, BUILD_DIR→string) regardless of quoting.
	qg := bindingMap(q)
	bg := bindingMap(b)
	if !reflect.DeepEqual(qg, bg) {
		t.Errorf("bindings differ:\n quoted=%+v\n bare  =%+v", qg, bg)
	}
	if got := qg["PORT"]; got.typ != "int" || got.val != "8080" {
		t.Errorf("PORT binding: %+v (want int/8080)", got)
	}
	if got := qg["VERBOSE"]; got.typ != "bool" || got.val != "true" {
		t.Errorf("VERBOSE binding: %+v (want bool/true)", got)
	}
	if got := qg["BUILD_DIR"]; got.typ != "string" || got.val != "./out" {
		t.Errorf("BUILD_DIR binding: %+v (want string/./out)", got)
	}

	// Requirements: same declared bins/env/hosts/read/write.
	if !reflect.DeepEqual(reqShape(q), reqShape(b)) {
		t.Errorf("requirements differ:\n quoted=%+v\n bare  =%+v", reqShape(q), reqShape(b))
	}

	// Command args: names, types, defaults all match.
	qc := q.Commands["start"]
	bc := b.Commands["start"]
	if qc == nil || bc == nil {
		t.Fatalf("missing start command: quoted=%v bare=%v", qc, bc)
	}
	if !reflect.DeepEqual(argShape(qc), argShape(bc)) {
		t.Errorf("args differ:\n quoted=%+v\n bare  =%+v", argShape(qc), argShape(bc))
	}

	// Ops: same kinds and the same string args.
	if !reflect.DeepEqual(opShape(qc), opShape(bc)) {
		t.Errorf("ops differ:\n quoted=%+v\n bare  =%+v", opShape(qc), opShape(bc))
	}
}

type binding struct{ typ, val string }

func bindingMap(p *domain.Program) map[string]binding {
	m := map[string]binding{}
	for _, g := range p.Globals.Bindings {
		m[g.Name] = binding{typ: g.Type, val: fmt.Sprintf("%v", g.Value)}
	}
	return m
}

func reqShape(p *domain.Program) string {
	r := p.Requirements
	bins := make([]string, len(r.Bins))
	for i, b := range r.Bins {
		bins[i] = b.Name
	}
	envs := make([]string, len(r.Envs))
	for i, e := range r.Envs {
		envs[i] = e.Name
	}
	hosts := make([]string, len(r.Hosts))
	for i, h := range r.Hosts {
		hosts[i] = h.Name
	}
	return fmt.Sprintf("bins=%v envs=%v hosts=%v read=%v write=%v", bins, envs, hosts, r.ReadRoots, r.WriteRoots)
}

func argShape(c *domain.Command) []string {
	out := make([]string, len(c.Args))
	for i, a := range c.Args {
		out[i] = fmt.Sprintf("%s:%s=%v", a.Name, a.Type, a.Default)
	}
	return out
}

func opShape(c *domain.Command) []string {
	out := make([]string, len(c.Ops))
	for i, op := range c.Ops {
		out[i] = fmt.Sprintf("%s %v cap=%s", op.Kind, op.Args, op.CaptureInto)
	}
	return out
}

// TestQuotesOptional_VersionLikeTokens covers tokens quotes used to be required
// for: dotted versions, leading-digit identifiers, and pure numbers.
func TestQuotesOptional_VersionLikeTokens(t *testing.T) {
	for _, v := range []string{"1.0.0", "v2.3.1", "1", "2024.01", "alpha-1"} {
		src := "name x\nversion " + v + "\nrequires\nend\ncommand c\n  do\n    print hi\n  end\nend\n"
		p, err := LoadFromString(src)
		if err != nil {
			t.Errorf("bare version %q failed to load: %v", v, err)
			continue
		}
		if p.Version != v {
			t.Errorf("bare version %q: got %q", v, p.Version)
		}
	}
}
