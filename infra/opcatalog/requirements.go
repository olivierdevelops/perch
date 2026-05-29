package opcatalog

// Requirements describes what must be declared in a `requires` block to use an op.
// Pure and ambient ops need no declaration.
type Requirements struct {
	Pure     bool     `json:"pure,omitempty"`
	Ambient  bool     `json:"ambient,omitempty"`
	Bin      bool     `json:"bin,omitempty"`
	BinNote  string   `json:"bin_note,omitempty"`
	Host     bool     `json:"host,omitempty"`
	Net      bool     `json:"net,omitempty"`
	Read     bool     `json:"read,omitempty"`
	Write    bool     `json:"write,omitempty"`
	Env      bool     `json:"env,omitempty"`
	Declares []string `json:"declares,omitempty"`
}

var (
	shellBins = strSet{
		"shell": true, "shell_output": true, "shell_detached": true,
		"shell_in": true, "try_shell": true,
	}
	subprocessBins = strSet{
		"exec": true, "exec_chain": true, "pipe": true,
		"bin_version": true, "pkg_install": true, "pkg_uninstall": true,
		"pkg_installed": true, "os_version": true, "process_running": true,
		"kill_by_name": true,
	}
	hostOps = strSet{
		"http_get": true, "http_post": true, "http_put": true, "http_delete": true,
		"http_status": true, "download": true, "dns_lookup": true, "port_check": true,
		"wait_for_port": true, "wait_for_url": true, "public_ip": true,
	}
	netOps = strSet{
		"local_ip": true, "interfaces": true, "mac_address": true,
		"port_free": true, "find_free_port": true,
	}
	envOps = strSet{
		"get_env": true, "set_env": true, "unset_env": true,
		"env_has": true, "env_default": true,
	}
	readOps = strSet{
		"read_file": true, "exists": true, "is_dir": true, "is_file": true,
		"file_size": true, "list_dir": true, "read_link": true,
		"sha256_file": true, "sha1_file": true, "md5_file": true,
		"glob": true, "verify_sha256": true,
	}
	writeOps = strSet{
		"mkdir": true, "rm": true, "touch": true, "chmod": true,
		"write_file": true, "append_file": true, "append_line": true,
		"ensure_dir": true, "make_executable": true, "ensure_line_in_file": true,
		"replace_in_file": true, "symlink": true, "bundle_extract": true,
		"mktemp_dir": true, "mktemp_file": true, "export_ops_catalog": true,
	}
	readWriteOps = strSet{
		"cp": true, "mv": true, "copy_dir": true, "backup_file": true,
		"tar_create": true, "tar_extract": true, "zip_create": true,
		"zip_extract": true, "gzip": true, "ungzip": true,
	}
	ambientOps = strSet{
		"get_os": true, "get_arch": true, "hostname": true, "user": true,
		"pid": true, "cpu_count": true, "cwd": true, "home_dir": true,
		"temp_dir": true, "cache_dir": true, "config_dir": true, "data_dir": true,
		"app_data_dir": true, "exe_path": true, "exe_dir": true,
		"script_path": true, "script_dir": true, "path_sep": true,
		"path_list_sep": true, "exe_ext": true, "null_device": true,
		"which": true, "has_bin": true, "detect_pkg_mgr": true,
		"is_admin": true, "is_ci": true, "is_tty": true,
	}
)

type strSet map[string]bool

func requirementsFor(kind string) Requirements {
	if ambientOps[kind] {
		return Requirements{Ambient: true}
	}
	var r Requirements
	if shellBins[kind] {
		r.Bin = true
		r.BinNote = "First token of the shell command must match a declared `bin \"…\"`."
		r.Declares = append(r.Declares, "bin")
	}
	if subprocessBins[kind] {
		r.Bin = true
		if kind == "exec" || kind == "exec_chain" || kind == "pipe" {
			r.BinNote = "Named binary (and each pipe stage) must be declared in `bin \"…\"`."
		} else {
			r.BinNote = "Spawned binary must be declared in `bin \"…\"`."
		}
		r.Declares = appendUnique(r.Declares, "bin")
	}
	if hostOps[kind] {
		r.Host = true
		r.Declares = appendUnique(r.Declares, "host")
	}
	if netOps[kind] {
		r.Net = true
		r.Declares = appendUnique(r.Declares, "host")
	}
	if envOps[kind] {
		r.Env = true
		r.Declares = appendUnique(r.Declares, "env")
	}
	if readOps[kind] || readWriteOps[kind] {
		r.Read = true
		r.Declares = appendUnique(r.Declares, "read")
	}
	if writeOps[kind] || readWriteOps[kind] {
		r.Write = true
		r.Declares = appendUnique(r.Declares, "write")
	}
	if kind == "download" {
		// host + write dst — already covered; ensure both in Declares
		r.Declares = appendUnique(r.Declares, "host", "write")
	}
	if len(r.Declares) == 0 && !r.Bin && !r.Host && !r.Net && !r.Read && !r.Write && !r.Env {
		return Requirements{Pure: true}
	}
	return r
}

func appendUnique(xs []string, vals ...string) []string {
	seen := map[string]bool{}
	for _, x := range xs {
		seen[x] = true
	}
	for _, v := range vals {
		if !seen[v] {
			xs = append(xs, v)
			seen[v] = true
		}
	}
	return xs
}

func categoryFor(kind string) string {
	switch {
	case shellBins[kind] || subprocessBins[kind] || kind == "print" || kind == "println" || kind == "eprintln" || kind == "fail" || kind == "exit" || kind == "sleep" || kind == "run" || kind == "list_commands" || kind == "export_ops_catalog":
		return "process"
	case readOps[kind] || writeOps[kind] || readWriteOps[kind]:
		return "filesystem"
	case hostOps[kind] || netOps[kind]:
		return "network"
	case envOps[kind]:
		return "environment"
	case kind == "if" || kind == "if_call" || kind == "for_each" || kind == "try" || kind == "match" || kind == "os" || kind == "arch":
		return "control_flow"
	case kind == "timeout" || kind == "retry" || kind == "parallel" || kind == "with_env" || kind == "with_cwd" || kind == "sandbox":
		return "context"
	case kind == "wasm_run" || kind == "wasm_arg" || kind == "wasm_mount_read" || kind == "wasm_mount_write" || kind == "wasm_env" || kind == "wasm_allow_host":
		return "wasm"
	case kind == "assert_eq" || kind == "assert_neq" || kind == "assert_contains" || kind == "assert_not_contains" || kind == "assert_exists" || kind == "assert_not_exists" || kind == "assert_match" || kind == "assert_version" || kind == "assert_version_ge":
		return "assertions"
	case kind == "cache" || kind == "bundle_hash" || kind == "bundle_dir":
		return "bundle"
	default:
		if len(kind) > 5 && kind[:5] == "http_" {
			return "network"
		}
		if len(kind) > 4 && (kind[:4] == "path" || kind == "expand_path" || kind == "is_abs" || kind == "to_slash" || kind == "from_slash") {
			return "paths"
		}
		if len(kind) >= 4 && kind[:4] == "json" || kind == "csv_parse" || (len(kind) >= 6 && kind[:6] == "base64") || (len(kind) >= 3 && (kind[:3] == "hex" || kind[:3] == "url")) {
			return "encoding"
		}
		if kind == "grep" || kind == "reject" || kind == "cut" || kind == "head" || kind == "tail" || kind == "sort_lines" || kind == "uniq_lines" || kind == "count_lines" {
			return "text_lines"
		}
		if kind == "trim" || kind == "lower" || kind == "upper" || kind == "contains" || kind == "split" || kind == "join" || kind == "format" || kind == "replace" || kind == "repeat" || kind == "length" || kind == "capitalize" || kind == "has_prefix" || kind == "has_suffix" {
			return "strings"
		}
		if kind == "md5" || kind == "sha1" || kind == "sha256" || kind == "crc32" || kind == "md5_file" || kind == "sha1_file" || kind == "sha256_file" || kind == "verify_sha256" {
			return "hashing"
		}
		if kind == "regex_match" || kind == "regex_replace" || kind == "regex_find_all" {
			return "regex"
		}
		if kind == "now" || kind == "unix_to_iso" {
			return "time"
		}
		if len(kind) > 7 && kind[:7] == "version" {
			return "version"
		}
		if kind == "which" || kind == "has_bin" || kind == "bin_version" || kind == "pkg_install" || kind == "pkg_installed" || kind == "pkg_uninstall" || kind == "detect_pkg_mgr" || kind == "path_contains" || kind == "shell_rc_path" || kind == "add_to_path" || kind == "link_into_path" {
			return "install"
		}
		return "other"
	}
}
