package opcatalog

import (
	"fmt"
	"strings"
)

// opExamples holds hand-written samples for ops where heuristics are misleading.
var opExamples = map[string]string{
	"if": `if count > 0
    print "has items"
end`,
	"if_call": `if exists "./bin/app"
    print "built"
end`,
	"for_each": `for_each lines line
    print "${line}"
end`,
	"try": `try
    exec risky_cmd
rescue
    print "failed: ${err.message}"
end`,
	"match": `match os
    "darwin"
        print "mac"
    "linux"
        print "linux"
end`,
	"os": `os "linux"
    print "on linux"
end`,
	"arch": `arch "arm64"
    print "apple silicon"
end`,
	"timeout": `timeout 30
    exec long_running
end`,
	"retry": `retry 3
    http_get "https://example.com/health"
end`,
	"parallel": `parallel max=2
    exec task_a
    exec task_b
end`,
	"with_env": `with_env DEBUG "1"
    exec mytool
end`,
	"with_cwd": `with_cwd "${script_dir}"
    exec git status
end`,
	"sandbox": `sandbox no_network
    read_file "./config.json"
end`,
	"pipe": `let out = pipe
    exec grep ERROR
    exec wc -l
end`,
	"exec_chain": `exec_chain
    exec git add .
    && exec git commit -m "wip"
    || print "nothing to commit"
end`,
	"exec":           `exec git status`,
	"shell":          `shell "git status"`,
	"shell_output":   `let out = shell_output "git rev-parse HEAD"`,
	"shell_detached": `shell_detached "myserver start"`,
	"shell_in":       `shell_in "cat" "input data"`,
	"try_shell":      `try_shell "maybe_fails"`,
	"run":            `run deploy`,
	"list_commands":  `list_commands`,
	"write_file":     `write_file "./out.txt" "hello"`,
	"read_file":      `let body = read_file "./config.json"`,
	"http_get":       `let body = http_get "https://example.com/api"`,
	"http_post":      `let resp = http_post "https://example.com/hook" "{\"ok\":true}"`,
	"download":       `download "https://example.com/pkg.tgz" "./pkg.tgz"`,
	"replace":        `let out = replace "hello world" "world,universe"`,
	"json_get":       `let name = json_get doc "user.name"`,
	"get_env":        `let path = get_env "PATH"`,
	"set_env":        `set_env "MY_FLAG" "1"`,
	"cache":          `let hit = cache "build" "v1" "compute_key"`,
	"bundle_dir":     `let assets = bundle_dir`,
	"bundle_extract": `bundle_extract "./plugin.wasm"`,
	"wasm_run": `wasm_run
    wasm_arg count 42
    wasm_mount_read "./schema.json"
end`,
	"assert_eq":           `assert_eq "${got}" "expected"`,
	"assert_contains":     `assert_contains "${log}" "OK"`,
	"assert_exists":       `assert_exists "./bin/app"`,
	"assert_match":        `assert_match "${tag}" "^v[0-9]+"`,
	"assert_version_ge":   `assert_version_ge "1.2.0" "1.0.0"`,
	"version_compat":      `let ok = version_compat "^1.2" "1.2.3"`,
	"wait_for_port":       `wait_for_port "localhost" "8080" "30"`,
	"wait_for_url":        `wait_for_url "https://example.com/health" "60"`,
	"ensure_line_in_file": `ensure_line_in_file "./.gitignore" "node_modules/"`,
	"replace_in_file":     `replace_in_file "./config.ini" "debug=false" "debug=true"`,
	"csv_parse":           `let rows = csv_parse "${csv_text}"`,
	"csv_stringify":       `let csv = csv_stringify rows`,
}

func exampleForOp(kind string, args []Arg, signature string) string {
	if ex, ok := opExamples[kind]; ok {
		return ex
	}
	if returnsValue(signature) {
		return fmt.Sprintf("let result = %s", callExpr(kind, args, signature))
	}
	return callExpr(kind, args, signature)
}

func exampleForVar(name, typ string) string {
	switch typ {
	case "bool":
		return fmt.Sprintf("if %s\n    print \"yes\"\nend", name)
	default:
		return fmt.Sprintf("print \"${%s}\"", name)
	}
}

func returnsValue(signature string) bool {
	return strings.Contains(signature, "→")
}

func callExpr(kind string, args []Arg, signature string) string {
	if strings.Contains(signature, "()") || len(args) == 0 {
		return kind
	}
	parts := make([]string, 0, len(args))
	for _, a := range args {
		if a.Type == "tail" {
			continue
		}
		parts = append(parts, argSample(kind, a))
	}
	if len(parts) == 0 {
		return kind
	}
	return kind + " " + strings.Join(parts, " ")
}

func argSample(kind string, a Arg) string {
	if v, ok := perArgSamples[kind+"."+a.Name]; ok {
		return v
	}
	if v, ok := perArgSamples[a.Name]; ok {
		return v
	}
	switch a.Type {
	case "word":
		return sampleWord(kind, a.Name)
	case "ident":
		return sampleIdent(a.Name)
	case "int":
		return sampleInt(a.Name)
	case "tail":
		return ""
	default:
		return sampleString(kind, a.Name)
	}
}

var perArgSamples = map[string]string{
	"bin":           "git",
	"argv":          "status",
	"msg":           `"hello"`,
	"cmd":           `"echo hi"`,
	"path":          `"./data"`,
	"src":           `"./src"`,
	"dst":           `"./dst"`,
	"content":       `"line one"`,
	"url":           `"https://example.com"`,
	"host":          `"localhost"`,
	"target":        "deploy",
	"code":          "1",
	"pattern":       `"*.json"`,
	"mode":          `"0755"`,
	"hash":          `"sha256:abc123"`,
	"key":           `"cache-key"`,
	"value":         `"42"`,
	"secs":          "5",
	"text":          `"input text"`,
	"pat":           `"ERROR"`,
	"n":             "10",
	"fmt":           `"%s v%d"`,
	"sep":           `","`,
	"old":           `"old"`,
	"new":           `"new"`,
	"link":          `"./link"`,
	"name":          "line",
	"line":          `"setting=true"`,
	"pkg":           `"curl"`,
	"proc":          `"nginx"`,
	"port":          `"8080"`,
	"timeout":       `"30"`,
	"layout":        `"rfc3339"`,
	"constraint":    `"^1.0"`,
	"version":       `"1.2.3"`,
	"min":           `"1.0.0"`,
	"entry":         `"plugin.wasm"`,
	"module":        `"validator"`,
	"func":          `"validate"`,
	"scenario":      `"default"`,
	"fixture":       `"./fixture.json"`,
	"max":           "3",
	"env_name":      `"DEBUG"`,
	"env_value":     `"1"`,
	"dir":           `"${script_dir}"`,
	"expr":          `"linux"`,
	"list":          "items",
	"ident":         "item",
	"haystack":      `"hello world"`,
	"needle":        `"world"`,
	"left":          `"1.2.3"`,
	"right":         `"1.0.0"`,
	"a":             `"a"`,
	"b":             `"b"`,
	"got":           `"actual"`,
	"want":          `"expected"`,
	"file":          `"./file.txt"`,
	"archive":       `"./dist.tar.gz"`,
	"zip":           `"./dist.zip"`,
	"data":          `"stdin data"`,
	"stdin":         `"input"`,
	"stage":         `"grep foo"`,
	"clause":        `exec echo ok`,
	"wasm_path":     `"./plugin.wasm"`,
	"mount":         `"./data"`,
	"allow":         `"api.example.com"`,
	"label":         `"prod"`,
	"kind":          `"ErrNotFound"`,
	"field":         `"name"`,
	"json":          `"{\"ok\":true}"`,
	"csv":           `"a,b\n1,2"`,
	"rows":          "rows",
	"doc":           "doc",
	"ip":            `"127.0.0.1"`,
	"service":       `"http"`,
	"signal":        `"SIGTERM"`,
	"handler":       "cleanup",
	"root":          `"${script_dir}"`,
	"glob":          `"**/*.json"`,
	"line_no":       "1",
	"count":         "3",
	"delay":         "1",
	"attempts":      "3",
	"seconds":       "5",
	"ms":            "500",
	"tag":           `"v1.2.3"`,
	"regex":         `"^[a-z]+$"`,
	"repl":          `"X"`,
	"subject":       `"abc123"`,
	"timestamp":     "1700000000",
	"format":        `"rfc3339"`,
	"algo":          `"sha256"`,
	"expected":      `"deadbeef"`,
	"actual":        `"deadbeef"`,
	"bin_name":      `"go"`,
	"flag":          `"--version"`,
	"manager":       `"apt"`,
	"package":       `"curl"`,
	"command":       `"deploy"`,
	"arg":           `"--verbose"`,
	"cwd":           `"${script_dir}"`,
	"pid":           `"1234"`,
	"uid":           `"1000"`,
	"user":          `"alice"`,
	"hostname":      `"myhost"`,
	"scheme":        `"https"`,
	"method":        `"GET"`,
	"body":          `"{}"`,
	"headers":       `"Content-Type: application/json"`,
	"token":         `"secret"`,
	"namespace":     `"default"`,
	"resource":      `"pod/app"`,
	"policy":        `"strict"`,
	"manifest":      `"./k8s.yaml"`,
	"diff":          `"./changes.patch"`,
	"summary":       `"risk: low"`,
	"risk":          `"low"`,
	"schema":        `"./schema.json"`,
	"input":         `"./input.json"`,
	"output":        `"./output.json"`,
	"plugin":        `"discount"`,
	"cart":          `"./cart.json"`,
	"tax_rate":      `"0.08"`,
	"amount":        `"100"`,
	"currency":      `"USD"`,
	"locale":        `"en_US"`,
	"timezone":      `"UTC"`,
	"date":          `"2024-01-01"`,
	"time":          `"12:00:00"`,
	"duration":      `"30s"`,
	"interval":      `"1s"`,
	"limit":         "100",
	"offset":        "0",
	"page":          "1",
	"size":          "20",
	"filter":        `"active"`,
	"query":         `"status=ok"`,
	"endpoint":      `"/health"`,
	"base":          `"."`,
	"rel":           `"./sub"`,
	"ext":           `".txt"`,
	"elements":      `"a" "b"`,
	"parts":         `"a" "b"`,
	"paths":         `"a" "b"`,
	"values":        `"x" "y"`,
	"items":         "items",
	"item":          "item",
	"who":           `"world"`,
	"what":          `"build"`,
	"where":         `"${temp_dir}"`,
	"when":          `"now"`,
	"why":           `"deploy"`,
	"how":           `"exec"`,
	"status":        `"ok"`,
	"state":         `"ready"`,
	"level":         `"info"`,
	"message":       `"done"`,
	"error":         `"failed"`,
	"reason":        `"timeout"`,
	"detail":        `"connection refused"`,
	"source":        `"./src"`,
	"dest":          `"./dest"`,
	"from":          `"./from"`,
	"to":            `"./to"`,
}

func sampleWord(kind, name string) string {
	if name == "bin" {
		return "git"
	}
	return "arg"
}

func sampleIdent(name string) string {
	switch name {
	case "target", "command":
		return "deploy"
	case "name", "ident", "item", "line":
		return name
	default:
		return "name"
	}
}

func sampleInt(name string) string {
	switch name {
	case "code":
		return "1"
	case "n", "count", "max", "attempts", "secs", "seconds", "delay":
		return "3"
	default:
		return "1"
	}
}

func sampleString(kind, name string) string {
	if strings.HasSuffix(name, "_path") || name == "path" {
		return `"./path"`
	}
	if strings.Contains(name, "url") || name == "url" {
		return `"https://example.com"`
	}
	if strings.Contains(name, "host") {
		return `"localhost"`
	}
	if strings.Contains(name, "file") {
		return `"./file.txt"`
	}
	if strings.Contains(name, "dir") {
		return `"${script_dir}"`
	}
	return `"value"`
}
