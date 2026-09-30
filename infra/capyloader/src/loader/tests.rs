use std::collections::BTreeMap;
use std::fs;

use perch_domain::{Command, ErrorKind, OpError, Program};
use serde_json::Value;

use crate::{load, load_from_string};

fn arg_str<'a>(m: &'a serde_json::Map<String, Value>, k: &str) -> &'a str {
    m.get(k).and_then(|v| v.as_str()).unwrap_or("")
}

fn keys_of(m: &BTreeMap<String, Command>) -> Vec<&String> {
    m.keys().collect()
}

fn err_string(r: Result<Program, crate::Error>) -> String {
    match r {
        Ok(_) => panic!("expected an error, got Ok"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn load_minimal() {
    let src = "name \"app\"\nabout \"hi\"\nversion \"0.1\"\nrequires\nend\n";
    let p = load_from_string(src).unwrap();
    assert_eq!(p.name, "app");
    assert_eq!(p.description, "hi");
    assert_eq!(p.version, "0.1");
}

// Bare top-level `NAME = value` bindings replace the removed globals block.
#[test]
fn bare_top_level_bindings() {
    let src = "name \"x\"\nBUILD_DIR = \"./out\"\nflag = true\nrequires\nend\n";
    let p = load_from_string(src).unwrap();
    let got: BTreeMap<_, _> = p.globals.bindings.iter().map(|b| (b.name.clone(), b.ty.clone())).collect();
    assert_eq!(got["BUILD_DIR"], "string");
    assert_eq!(got["flag"], "bool");
}

// An interpolated exec bin (`exec ${tool} …`) can't be resolved statically, so
// the load-time gate must SKIP it (defer to the runtime guard) rather than flag
// bin_not_declared — consistent with how requires treats interpolated
// hosts/paths.
#[test]
fn interpolated_exec_bin_skips_static_gate() {
    let src = "name \"x\"\nTOOL = \"echo\"\nrequires\nend\ncommand t\n    do\n        exec ${TOOL} hi\n        exec ${HOME}/bin/thing run\n    end\nend\n";
    load_from_string(src).expect("interpolated exec bins must not be gated at load");
}

// Unique-name registry: a command shadowing a built-in op, two things sharing a
// name, and a command/bin name clash all error at load.
#[test]
fn name_registry() {
    let cases = [
        (
            "shadow op",
            "name \"x\"\nrequires\nend\ncommand print\n    do\n        fail \"x\"\n    end\nend\n",
            "collides",
        ),
        (
            "command vs bin",
            "name \"x\"\nrequires\n    bin \"tool\"\nend\ncommand tool\n    do\n        fail \"x\"\n    end\nend\n",
            "unique",
        ),
    ];
    for (name, src, want) in cases {
        let msg = match load_from_string(src) {
            Ok(_) => String::new(),
            Err(e) => e.to_string(),
        };
        assert!(msg.contains(want), "{name}: want error containing {want:?}, got {msg:?}");
    }
}

// `bin "PATH" as NAME` parses into a BinReq carrying both the path and the
// alias handle (+ the optional variant).
#[test]
fn bin_path_alias() {
    let src = "name \"x\"\nrequires\n    bin \"./bins/tool.exe\" as tool\n    bin \"${script_dir}/helper\" as helper optional\nend\n";
    let p = load_from_string(src).unwrap();
    let bins = &p.requirements.bins;
    assert_eq!(bins.len(), 2, "{bins:?}");
    assert!(bins[0].name == "./bins/tool.exe" && bins[0].alias == "tool", "{:?}", bins[0]);
    assert!(bins[1].alias == "helper" && bins[1].optional, "{:?}", bins[1]);
}

// The removed `globals` block must yield a clear migration error.
#[test]
fn globals_block_removed() {
    let src = "name \"x\"\nglobals\n    a = 1\nend\nrequires\nend\n";
    let msg = err_string(load_from_string(src));
    assert!(msg.contains("globals"), "{msg}");
}

// Bare-name requires forms: `write DIR` / `read SRC` resolve a binding.
#[test]
fn requires_by_name() {
    let src = "name \"x\"\nDIR = \"./out\"\nSRC = \"./cmd\"\nrequires\n    write DIR\n    read SRC\nend\n";
    let p = load_from_string(src).unwrap();
    assert_eq!(p.requirements.write_roots, vec!["${DIR}"]);
    assert_eq!(p.requirements.read_roots, vec!["${SRC}"]);
}

#[test]
fn globals() {
    let src = "name \"x\"\nrequires\nend\nflag = true\nn = 42\ns = \"hello\"\n";
    let p = load_from_string(src).unwrap();
    let got: BTreeMap<_, _> = p.globals.bindings.iter().map(|b| (b.name.clone(), b.ty.clone())).collect();
    for (k, v) in [("flag", "bool"), ("n", "int"), ("s", "string")] {
        assert_eq!(got[k], v, "global {k:?}");
    }
}

#[test]
fn command_with_args_and_ops() {
    let src = r#"name "x"
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
"#;
    let p = load_from_string(src).unwrap();
    let c = p.commands.get("build").expect("command build missing");
    assert_eq!(c.description, "compile");
    assert!(c.modifiers.private, "Private modifier not set");
    assert!(c.args.len() == 1 && c.args[0].name == "target" && c.args[0].ty == "string", "{:?}", c.args);
    assert!(c.args[0].has_default && c.args[0].default == Value::String("darwin".into()));
    assert_eq!(c.args[0].description, "Target OS");
    assert_eq!(c.ops.len(), 2);
    assert!(c.ops[0].kind == "print" && c.ops[1].kind == "shell");
}

#[test]
fn arg_block_fields() {
    let src = r#"name "x"
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
"#;
    let p = load_from_string(src).unwrap();
    let args = &p.commands["t"].args;
    assert_eq!(args.len(), 3);
    assert!(args[0].name == "count" && args[0].ty == "int", "{:?}", args[0]);
    assert!(args[0].has_default, "count: missing default flag");
    assert_eq!(args[1].index, Some(0));
    assert!(args[2].optional, "port: not optional");
}

#[test]
fn arg_missing_type_is_error() {
    let src = "name \"x\"\ncommand t\n    arg foo\n        default \"bar\"\n    end\n    do\n        print \"\"\n    end\nend\n";
    assert!(load_from_string(src).is_err(), "expected error for arg without type");
}

#[test]
fn nested_block_ops() {
    let src = r#"name "x"
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
"#;
    let p = load_from_string(src).unwrap();
    let c = &p.commands["setup"];
    assert_eq!(c.ops.len(), 2);
    for (i, op) in c.ops.iter().enumerate() {
        assert_eq!(op.kind, "if", "ops[{i}].kind");
        assert!(op.body.len() == 1 && op.body[0].kind == "shell", "ops[{i}].body: {:?}", op.body);
        assert!(
            op.args.get("op") == Some(&Value::String("eq".into()))
                && op.args.get("lhs") == Some(&Value::String("os".into())),
            "ops[{i}].args: {:?}",
            op.args
        );
    }
}

#[test]
fn let_capture() {
    let src = r#"name "x"
requires
end
command t
    do
        h = sha256_file "./bin"
        name = json_get "{}" "user.name"
    end
end
"#;
    let p = load_from_string(src).unwrap();
    let c = &p.commands["t"];
    assert_eq!(c.ops.len(), 2);
    assert!(c.ops[0].kind == "sha256_file" && c.ops[0].capture_into == "h", "{:?}", c.ops[0]);
    assert!(c.ops[1].kind == "json_get" && c.ops[1].capture_into == "name", "{:?}", c.ops[1]);
}

#[test]
fn catch() {
    let src = r#"name "x"
requires
end
catch unknown
    description "fallback"
    do
        print "no such: ${unknown}"
        exit 1
    end
end
"#;
    let p = load_from_string(src).unwrap();
    let c = p.catch.expect("catch missing");
    assert_eq!(c.bind, "unknown");
    assert_eq!(c.description, "fallback");
    assert_eq!(c.ops.len(), 2);
}

#[test]
fn parse_error() {
    let src = "command foo\n    blorp_unknown \"x\"\nend\n";
    assert!(load_from_string(src).is_err(), "expected parse error for unknown function");
}

#[test]
fn import_flat() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    fs::write(
        d.join("child.perch"),
        "name \"child\"\ncommand imported_cmd\n    description \"from child.perch\"\n    do\n        print \"ok\"\n    end\nend\n",
    )
    .unwrap();
    let main = d.join("main.perch");
    fs::write(
        &main,
        "name \"main\"\nrequires\nend\nimport \"./child.perch\"\ncommand local_cmd\n    description \"from main.perch\"\n    do\n        print \"local\"\n    end\nend\n",
    )
    .unwrap();
    let p = load(main.to_str().unwrap()).unwrap();
    assert!(p.commands.contains_key("imported_cmd"), "flat import: imported_cmd missing; got {:?}", keys_of(&p.commands));
    assert!(p.commands.contains_key("local_cmd"), "local_cmd missing; got {:?}", keys_of(&p.commands));
}

#[test]
fn import_aliased() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    fs::write(
        d.join("aws.perch"),
        "name \"aws\"\ncommand upload\n    description \"u\"\n    do\n        print \"u\"\n    end\nend\n",
    )
    .unwrap();
    let main = d.join("main.perch");
    fs::write(
        &main,
        "name \"main\"\nrequires\nend\nimport \"./aws.perch\" as aws\ncommand deploy\n    description \"d\"\n    do\n        aws.upload\n    end\nend\n",
    )
    .unwrap();
    let p = load(main.to_str().unwrap()).unwrap();
    assert!(p.commands.contains_key("aws.upload"), "aliased import: aws.upload missing; got {:?}", keys_of(&p.commands));
    assert!(!p.commands.contains_key("upload"), "aliased import: bare 'upload' should NOT be present");
}

#[test]
fn import_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    fs::write(d.join("a.perch"), "name \"a\"\nimport \"./b.perch\"\ncommand ca\n    description \"ca\"\n    do\n        print \"a\"\n    end\nend\n").unwrap();
    fs::write(d.join("b.perch"), "name \"b\"\nimport \"./a.perch\"\ncommand cb\n    description \"cb\"\n    do\n        print \"b\"\n    end\nend\n").unwrap();
    let msg = err_string(load(d.join("a.perch").to_str().unwrap()));
    assert!(msg.contains("cycle"), "expected cycle in error; got {msg}");
}

#[test]
fn import_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    fs::write(d.join("lib.perch"), "name \"lib\"\ncommand deploy\n    description \"from lib\"\n    do\n        print \"lib\"\n    end\nend\n").unwrap();
    fs::write(d.join("main.perch"), "name \"main\"\nimport \"./lib.perch\"\ncommand deploy\n    description \"from main\"\n    do\n        print \"main\"\n    end\nend\n").unwrap();
    let msg = err_string(load(d.join("main.perch").to_str().unwrap()));
    assert!(msg.contains("already declared"), "expected 'already declared' in error; got {msg}");
}

#[test]
fn import_file_dir() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    fs::create_dir_all(d.join("shared")).unwrap();
    fs::write(d.join("shared/lib.perch"), "name \"lib\"\ncommand from_lib\n    description \"from shared/lib.perch\"\n    do\n        print \"lib\"\n    end\nend\n").unwrap();
    fs::write(d.join("main.perch"), "name \"main\"\nrequires\nend\nimport \"${file_dir}/shared/lib.perch\"\ncommand main_cmd\n    description \"main\"\n    do\n        from_lib\n    end\nend\n").unwrap();
    let p = load(d.join("main.perch").to_str().unwrap()).unwrap();
    assert!(p.commands.contains_key("from_lib"), "from_lib missing after ${{file_dir}} expansion; got {:?}", keys_of(&p.commands));
}

#[test]
fn import_env_var() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    fs::write(d.join("lib.perch"), "name \"lib\"\ncommand e\n    description \"e\"\n    do\n        print \"e\"\n    end\nend\n").unwrap();
    std::env::set_var("PERCH_TEST_DIR_CAPYLOADER", d);
    let main = d.join("main.perch");
    fs::write(&main, "name \"main\"\nrequires\nend\nimport \"${PERCH_TEST_DIR_CAPYLOADER}/lib.perch\"\ncommand m\n    description \"m\"\n    do\n        e\n    end\nend\n").unwrap();
    let p = load(main.to_str().unwrap()).unwrap();
    assert!(p.commands.contains_key("e"), "e missing after env-var expansion; got {:?}", keys_of(&p.commands));
}

#[test]
fn import_unknown_var() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    fs::write(d.join("main.perch"), "name \"main\"\nimport \"${this_var_does_not_exist}/lib.perch\"\ncommand m\n    description \"m\"\n    do\n        print \"m\"\n    end\nend\n").unwrap();
    let msg = err_string(load(d.join("main.perch").to_str().unwrap()));
    assert!(msg.contains("this_var_does_not_exist"), "expected error to name the placeholder; got {msg}");
}

// The `let` keyword was removed: `=` is the universal assignment operator. A
// body capture is written `NAME = op args` (no `let`); top-level `NAME = value`
// stays a binding. Old `let` files get a clear migration error.
#[test]
fn let_keyword_removed() {
    // No-`let` capture inside a do block parses into a capture op.
    let src = "name \"x\"\nrequires\nend\ncommand c\n    do\n        u = upper \"hi\"\n        print \"${u}\"\n    end\nend\n";
    let p = load_from_string(src).expect("no-let capture failed to load");
    let ops = &p.commands["c"].ops;
    assert!(!ops.is_empty() && ops[0].kind == "upper" && ops[0].capture_into == "u", "{ops:?}");

    // Old `let NAME = …` is a clear migration error.
    let old = src.replacen("u = upper", "let u = upper", 1);
    let msg = match load_from_string(&old) {
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    };
    assert!(msg.contains("let` keyword was removed"), "expected a let-removed migration error, got {msg:?}");
}

// ── exec_parse_test.go ──────────────────────────────────────────────────────

// Verifies the `exec BIN "arg"…` op parses with the bin routed to args["bin"]
// and each quoted token to a positional argv slot (_0, _1, …) — the structured
// argv shape opExec consumes. Also covers the let-capture form
// `let x = exec BIN …`.
#[test]
fn exec_grammar() {
    let src = "name \"x\"\nrequires\n    bin \"git\"\n    bin \"docker\"\nend\ncommand t\n    do\n        exec git status\n        exec docker run -d --name web\n        head = exec git rev-parse HEAD\n    end\nend\n";
    let p = load_from_string(src).expect("parse");
    let ops = &p.commands["t"].ops;
    assert_eq!(ops.len(), 3, "{ops:?}");

    assert_eq!(ops[0].kind, "exec");
    assert_eq!(arg_str(&ops[0].args, "bin"), "git");
    assert_eq!(arg_str(&ops[0].args, "_0"), "status");

    assert_eq!(arg_str(&ops[1].args, "bin"), "docker");
    for (i, w) in ["run", "-d", "--name", "web"].iter().enumerate() {
        assert_eq!(arg_str(&ops[1].args, &format!("_{i}")), *w, "op1 _{i}");
    }

    assert_eq!(ops[2].kind, "exec");
    assert_eq!(ops[2].capture_into, "head");
    assert_eq!(arg_str(&ops[2].args, "bin"), "git");
    assert_eq!(arg_str(&ops[2].args, "_0"), "rev-parse");
    assert_eq!(arg_str(&ops[2].args, "_1"), "HEAD");
}

// ── requires_parse_test.go ──────────────────────────────────────────────────

// Verifies the loader hydrates a full `requires` block into Requirements: bins
// (plain / optional / hash / hash_file), env (plain / optional), host,
// read/write filesystem scopes, os, arch.
#[test]
fn requires_block_parsing() {
    let src = r#"name "app"

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
"#;
    let p = load_from_string(src).expect("parse");
    let r = &p.requirements;
    assert!(r.declared, "Requirements.declared should be true");

    assert_eq!(r.bins.len(), 4, "{:?}", r.bins);
    let by_name: BTreeMap<&str, usize> = r.bins.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
    assert!(r.bins[by_name["docker"]].optional, "docker should be optional");
    assert_eq!(r.bins[by_name["kubectl"]].hash, "sha256:abc123");
    assert_eq!(r.bins[by_name["internal-tool"]].hash_file, "bundle:checksums/tool.sha256");
    let git = &r.bins[by_name["git"]];
    assert!(git.hash.is_empty() && git.hash_file.is_empty(), "git should carry no hash metadata: {git:?}");

    assert_eq!(r.envs.len(), 2);
    let debug_optional = r.envs.iter().any(|e| e.name == "DEBUG" && e.optional);
    assert!(debug_optional, "DEBUG env should be optional");

    assert_eq!(r.hosts.len(), 2);

    assert!(r.read_roots.len() == 2 && r.read_roots[0] == "./src", "read roots: {:?}", r.read_roots);
    assert!(r.write_roots.len() == 1 && r.write_roots[0] == "./build", "write roots: {:?}", r.write_roots);

    assert!(r.os.len() == 2 && r.arch.len() == 1, "os={:?} arch={:?}", r.os, r.arch);
}

// A file without a `requires` block loads as if it had an empty
// `requires`/`end`: declared==true, no bins, pure ops allowed.
#[test]
fn no_requires_block_is_normalized() {
    let p = load_from_string("name \"x\"\ncommand t\n    do\n        print \"hi\"\n    end\nend\n").unwrap();
    assert!(p.requirements.declared, "missing requires must normalize to declared=true");
    assert!(p.requirements.bins.is_empty(), "{:?}", p.requirements.bins);
}

// A file with no requires block rejects undeclared exec at load (same as empty).
#[test]
fn no_requires_block_rejects_undeclared_exec() {
    let err = load_from_string("name \"x\"\ncommand t\n    do\n        exec echo hi\n    end\nend\n")
        .expect_err("want bin_not_declared at load");
    let oe = err.downcast_ref::<OpError>().expect("want an OpError");
    assert_eq!(oe.kind, ErrorKind::BinNotDeclared);
}

// An empty `requires`/`end` block is the legal "pure ops only" manifest: it
// loads cleanly and marks the program declared with no spawnable surface.
#[test]
fn empty_requires_block_is_declared() {
    let p = load_from_string("name \"x\"\nrequires\nend\ncommand t\n    do\n        print \"hi\"\n    end\nend\n").unwrap();
    assert!(p.requirements.declared, "an empty requires block must set declared=true");
    assert!(p.requirements.bins.is_empty());
}

// The removed version feature is gone: `bin "x" >= "1.0.0"` no longer parses as
// a requires entry.
#[test]
fn version_comparator_in_requires_rejected() {
    let src = "name \"x\"\nrequires\n    bin \"git\" >= \"2.0.0\"\nend\ncommand t\n    do\n        print \"hi\"\n    end\nend\n";
    assert!(load_from_string(src).is_err(), "expected a parse error for the removed version-comparator syntax");
}

// ── quotes_optional_test.go ─────────────────────────────────────────────────
//
// Quotes are OPTIONAL on every scalar value in the surface grammar. A bare
// token (`name redis`, `bin docker`, `default 6379`, `print hello`) and the
// quoted form (`name "redis"`, …) must parse to the SAME program. These tests
// pin that equivalence so a future grammar change can't silently make quotes
// mandatory again.

const QUOTED_PROGRAM: &str = r#"name    "myapp"
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
"#;

const BARE_PROGRAM: &str = r#"name    myapp
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
"#;

fn val_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn binding_map(p: &Program) -> BTreeMap<String, (String, String)> {
    p.globals.bindings.iter().map(|g| (g.name.clone(), (g.ty.clone(), val_str(&g.value)))).collect()
}

fn req_shape(p: &Program) -> String {
    let r = &p.requirements;
    let names = |v: Vec<&str>| format!("{v:?}");
    format!(
        "bins={} envs={} hosts={} read={:?} write={:?}",
        names(r.bins.iter().map(|b| b.name.as_str()).collect()),
        names(r.envs.iter().map(|b| b.name.as_str()).collect()),
        names(r.hosts.iter().map(|b| b.name.as_str()).collect()),
        r.read_roots,
        r.write_roots
    )
}

fn arg_shape(c: &Command) -> Vec<String> {
    c.args.iter().map(|a| format!("{}:{}={}", a.name, a.ty, val_str(&a.default))).collect()
}

fn op_shape(c: &Command) -> Vec<String> {
    c.ops.iter().map(|op| format!("{} {:?} cap={}", op.kind, op.args, op.capture_into)).collect()
}

#[test]
fn quotes_optional_bare_equals_quoted() {
    let q = load_from_string(QUOTED_PROGRAM).expect("quoted load");
    let b = load_from_string(BARE_PROGRAM).expect("bare load");

    assert!(q.name == b.name && q.name == "myapp", "name: quoted={:?} bare={:?}", q.name, b.name);
    assert!(q.version == b.version && q.version == "1.2.3", "version: quoted={:?} bare={:?}", q.version, b.version);
    assert_eq!(q.description, b.description);

    // Bare top-level bindings: same names, values, AND inferred types
    // (PORT→int, VERBOSE→bool, BUILD_DIR→string) regardless of quoting.
    let qg = binding_map(&q);
    let bg = binding_map(&b);
    assert_eq!(qg, bg, "bindings differ");
    assert_eq!(qg["PORT"], ("int".to_string(), "8080".to_string()));
    assert_eq!(qg["VERBOSE"], ("bool".to_string(), "true".to_string()));
    assert_eq!(qg["BUILD_DIR"], ("string".to_string(), "./out".to_string()));

    assert_eq!(req_shape(&q), req_shape(&b), "requirements differ");

    let qc = &q.commands["start"];
    let bc = &b.commands["start"];
    assert_eq!(arg_shape(qc), arg_shape(bc), "args differ");
    assert_eq!(op_shape(qc), op_shape(bc), "ops differ");
}

// Covers tokens quotes used to be required for: dotted versions, leading-digit
// identifiers, and pure numbers.
#[test]
fn quotes_optional_version_like_tokens() {
    for v in ["1.0.0", "v2.3.1", "1", "2024.01", "alpha-1"] {
        let src = format!("name x\nversion {v}\nrequires\nend\ncommand c\n  do\n    print hi\n  end\nend\n");
        let p = load_from_string(&src).unwrap_or_else(|e| panic!("bare version {v:?} failed to load: {e}"));
        assert_eq!(p.version, v, "bare version {v:?}");
    }
}

// ── unit tests for the pure helpers ─────────────────────────────────────────

#[test]
fn shell_split_classifies_bare_idents() {
    use super::shell_split_args_classified as split;
    let toks = split(r#"ps -q "a b" ${x} name"#);
    let got: Vec<(&str, bool)> = toks.iter().map(|t| (t.text.as_str(), t.bare)).collect();
    assert_eq!(got, vec![("ps", true), ("-q", false), ("a b", false), ("${x}", false), ("name", true)]);
}

#[test]
fn exec_chain_folds() {
    let src = "name \"x\"\nrequires\n    bin \"git\"\nend\ncommand t\n    do\n        exec git a && exec git b\n    end\nend\n";
    let p = load_from_string(src).unwrap();
    let op = &p.commands["t"].ops[0];
    assert_eq!(op.kind, "exec_chain");
    assert_eq!(op.body.len(), 2);
}

#[test]
fn op_kinds_sorted_and_nonempty() {
    let k = crate::op_kinds();
    assert!(!k.is_empty());
    let mut s = k.clone();
    s.sort();
    assert_eq!(k, s);
}

// ── R02a / R05 (PLAN-2026-0001) ──────────────────────────────────────────────

fn cmd_ops<'a>(p: &'a Program, name: &str) -> &'a Vec<perch_domain::Op> {
    &p.commands[name].ops
}

fn kinds(ops: &[perch_domain::Op]) -> Vec<&str> {
    ops.iter().map(|o| o.kind.as_str()).collect()
}

// T-05: a command-level `finally` lowers to a whole-body `try … finally` marker
// stream (one try op: body, `_catch` with no rescue arm, `_finally`, cleanup).
#[test]
fn t05_command_level_finally_lowers_to_try() {
    let src = "name \"x\"\nrequires\nend\ncommand t\n    do\n        print \"a\"\n        print \"b\"\n    finally\n        print \"c\"\n    end\nend\n";
    let p = load_from_string(src).unwrap();
    let ops = cmd_ops(&p, "t");
    assert_eq!(kinds(ops), ["try"]);
    assert_eq!(kinds(&ops[0].body), ["print", "print", "_catch", "_finally", "print"]);
    assert_eq!(arg_str(&ops[0].body[2].args, "bind"), "err");
    assert_eq!(arg_str(&ops[0].body[4].args, "msg"), "c");
}

// `do` without `finally`, and an empty `finally`, lower exactly as before.
#[test]
fn t05_no_finally_is_unchanged() {
    let src = "name \"x\"\nrequires\nend\ncommand t\n    do\n        print \"a\"\n    end\nend\ncommand u\n    do\n        print \"a\"\n    finally\n    end\nend\n";
    let p = load_from_string(src).unwrap();
    assert_eq!(kinds(cmd_ops(&p, "t")), ["print"]);
    assert_eq!(kinds(cmd_ops(&p, "u")), ["print"]);
}

// A nested `try … finally` inside a `do … finally` keeps its own sections.
#[test]
fn t05_nested_try_finally_inside_command_finally() {
    let src = "name \"x\"\nrequires\nend\ncommand t\n    do\n        try\n            print \"a\"\n        finally\n            print \"inner\"\n        end\n    finally\n        print \"outer\"\n    end\nend\n";
    let p = load_from_string(src).unwrap();
    let outer = &cmd_ops(&p, "t")[0];
    assert_eq!(kinds(&outer.body), ["try", "_catch", "_finally", "print"]);
    assert_eq!(kinds(&outer.body[0].body), ["print", "_catch", "_finally", "print"]);
}

const PREFIX_REQ: &str = "name \"x\"\nrequires\n    bin \"tool\"\nend\n";

fn prefix_of(op: &perch_domain::Op) -> Vec<(String, String)> {
    op.args
        .get("env_prefix")
        .and_then(|v| v.as_object())
        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string())).collect())
        .unwrap_or_default()
}

// T-25: leading NAME=VALUE tokens become `env_prefix` (order kept; bare, quoted,
// `$NAME`, `${NAME}`); name=value args AFTER the binary are untouched.
#[test]
fn t25_env_prefix_parses() {
    let src = format!(
        "{PREFIX_REQ}command t\n    do\n        K=bare Q=\"a b\" D=$HOME B=${{X}} tool run name=value\n    end\nend\n"
    );
    let p = load_from_string(&src).unwrap();
    let op = &cmd_ops(&p, "t")[0];
    assert_eq!(op.kind, "exec");
    assert_eq!(arg_str(&op.args, "bin"), "tool");
    assert_eq!(
        prefix_of(op),
        [("K", "bare"), ("Q", "a b"), ("D", "${HOME}"), ("B", "${X}")]
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .to_vec()
    );
    assert_eq!(arg_str(&op.args, "_0"), "run");
    assert_eq!(arg_str(&op.args, "_1"), "name=value");
    assert!(!op.args.contains_key("_2"));
}

#[test]
fn t25_env_prefix_on_exec_capture_and_chain() {
    let src = format!(
        "{PREFIX_REQ}command t\n    do\n        exec K=v tool a\n        out = J=2 tool b\n        K=1 tool a && L=2 tool b\n        plain = tool c k=v\n    end\nend\n"
    );
    let p = load_from_string(&src).unwrap();
    let ops = cmd_ops(&p, "t");
    assert_eq!(prefix_of(&ops[0]), [("K".to_string(), "v".to_string())]);
    assert_eq!(arg_str(&ops[0].args, "bin"), "tool");
    assert_eq!(ops[1].capture_into, "out");
    assert_eq!(prefix_of(&ops[1]), [("J".to_string(), "2".to_string())]);
    assert_eq!(ops[2].kind, "exec_chain");
    assert_eq!(prefix_of(&ops[2].body[0]), [("K".to_string(), "1".to_string())]);
    assert_eq!(prefix_of(&ops[2].body[1]), [("L".to_string(), "2".to_string())]);
    // A capture with name=value AFTER the binary has no prefix.
    assert!(ops[3].args.get("env_prefix").is_none());
    assert_eq!(arg_str(&ops[3].args, "_1"), "k=v");
}

// D2 (T-31, loader half): a prefix on a built-in op / command is rejected.
#[test]
fn t31_env_prefix_on_builtin_op_rejected() {
    let src = format!("{PREFIX_REQ}command t\n    do\n        K=1 print hi\n    end\nend\n");
    let e = err_string(load_from_string(&src));
    assert!(e.contains("only valid on a declared-bin call or `exec`") && e.contains("built-in op"), "{e}");
    let src = format!("{PREFIX_REQ}command other\n    do\n        print hi\n    end\nend\ncommand t\n    do\n        K=1 other\n    end\nend\n");
    let e = err_string(load_from_string(&src));
    assert!(e.contains("is a command"), "{e}");
}

// T-32 (loader half): malformed `NAME=` and a prefix with no command.
#[test]
fn t32_malformed_env_assignment_rejected() {
    for bad in ["1K=2 tool a", "K-V=2 tool a", "=x tool a", "exec K=v tool && 9X=1 tool"] {
        let src = format!("{PREFIX_REQ}command t\n    do\n        {bad}\n    end\nend\n");
        let e = err_string(load_from_string(&src));
        assert!(e.contains("malformed env assignment"), "{bad}: {e}");
    }
    let src = format!("{PREFIX_REQ}command t\n    do\n        exec K=v\n    end\nend\n");
    let e = err_string(load_from_string(&src));
    assert!(e.contains("no command to apply to"), "{e}");
}

// A statement that only LOOKS assignment-shaped inside a string or a comment is
// never rewritten.
#[test]
fn env_prefix_prepass_ignores_strings_and_comments() {
    let src = format!(
        "{PREFIX_REQ}command t\n    do\n        # K=v tool a\n        print \"K=v tool a\"\n        write_file \"f\" `\n  K=v tool a\n`\n    end\nend\n"
    );
    let p = load_from_string(&src).unwrap();
    assert_eq!(kinds(cmd_ops(&p, "t")), ["print", "write_file"]);
}
