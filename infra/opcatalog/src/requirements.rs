use serde::Serialize;

fn is_false(b: &bool) -> bool {
    !*b
}

/// What must be declared in a `requires` block to use an op. Pure and ambient
/// ops need no declaration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Requirements {
    #[serde(skip_serializing_if = "is_false")]
    pub pure: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub ambient: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub bin: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bin_note: String,
    #[serde(skip_serializing_if = "is_false")]
    pub host: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub net: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub read: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub write: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub env: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub declares: Vec<String>,
}

const SHELL_BINS: &[&str] = &["shell", "shell_output", "shell_detached", "shell_in", "try_shell"];
const SUBPROCESS_BINS: &[&str] = &[
    "exec", "exec_chain", "pipe", "bin_version", "pkg_install", "pkg_uninstall", "pkg_installed",
    "os_version", "process_running", "kill_by_name",
];
const HOST_OPS: &[&str] = &[
    "http_get", "http_post", "http_put", "http_delete", "http_status", "download", "dns_lookup",
    "port_check", "wait_for_port", "wait_for_url", "public_ip",
];
const NET_OPS: &[&str] = &["local_ip", "interfaces", "mac_address", "port_free", "find_free_port"];
const ENV_OPS: &[&str] = &["get_env", "set_env", "unset_env", "env_has", "env_default"];
const READ_OPS: &[&str] = &[
    "read_file", "exists", "is_dir", "is_file", "file_size", "list_dir", "read_link", "sha256_file",
    "sha1_file", "md5_file", "glob", "verify_sha256",
];
const WRITE_OPS: &[&str] = &[
    "mkdir", "rm", "touch", "chmod", "write_file", "append_file", "append_line", "ensure_dir",
    "make_executable", "ensure_line_in_file", "replace_in_file", "symlink", "bundle_extract",
    "mktemp_dir", "mktemp_file",
];
const READ_WRITE_OPS: &[&str] = &[
    "cp", "mv", "copy_dir", "backup_file", "tar_create", "tar_extract", "zip_create", "zip_extract",
    "gzip", "ungzip",
];
const AMBIENT_OPS: &[&str] = &[
    "get_os", "get_arch", "hostname", "user", "pid", "cpu_count", "cwd", "home_dir", "temp_dir",
    "cache_dir", "config_dir", "data_dir", "app_data_dir", "exe_path", "exe_dir", "script_path",
    "script_dir", "path_sep", "path_list_sep", "exe_ext", "null_device", "which", "has_bin",
    "detect_pkg_mgr", "is_admin", "is_ci", "is_tty",
];

fn has(set: &[&str], k: &str) -> bool {
    set.contains(&k)
}

pub(crate) fn requirements_for(kind: &str) -> Requirements {
    if has(AMBIENT_OPS, kind) {
        return Requirements { ambient: true, ..Default::default() };
    }
    let mut r = Requirements::default();
    if has(SHELL_BINS, kind) {
        r.bin = true;
        r.bin_note = "First token of the shell command must match a declared `bin \"…\"`.".into();
        r.declares.push("bin".into());
    }
    if has(SUBPROCESS_BINS, kind) {
        r.bin = true;
        r.bin_note = if kind == "exec" || kind == "exec_chain" || kind == "pipe" {
            "Named binary (and each pipe stage) must be declared in `bin \"…\"`.".into()
        } else {
            "Spawned binary must be declared in `bin \"…\"`.".into()
        };
        append_unique(&mut r.declares, &["bin"]);
    }
    if has(HOST_OPS, kind) {
        r.host = true;
        append_unique(&mut r.declares, &["host"]);
    }
    if has(NET_OPS, kind) {
        r.net = true;
        append_unique(&mut r.declares, &["host"]);
    }
    if has(ENV_OPS, kind) {
        r.env = true;
        append_unique(&mut r.declares, &["env"]);
    }
    if has(READ_OPS, kind) || has(READ_WRITE_OPS, kind) {
        r.read = true;
        append_unique(&mut r.declares, &["read"]);
    }
    if has(WRITE_OPS, kind) || has(READ_WRITE_OPS, kind) {
        r.write = true;
        append_unique(&mut r.declares, &["write"]);
    }
    if kind == "download" {
        append_unique(&mut r.declares, &["host", "write"]);
    }
    if r.declares.is_empty() && !r.bin && !r.host && !r.net && !r.read && !r.write && !r.env {
        return Requirements { pure: true, ..Default::default() };
    }
    r
}

fn append_unique(xs: &mut Vec<String>, vals: &[&str]) {
    for v in vals {
        if !xs.iter().any(|x| x == v) {
            xs.push((*v).to_string());
        }
    }
}

pub(crate) fn category_for(kind: &str) -> &'static str {
    let k = kind;
    let is = |names: &[&str]| names.contains(&k);
    if has(SHELL_BINS, k)
        || has(SUBPROCESS_BINS, k)
        || is(&["print", "println", "eprintln", "fail", "exit", "sleep", "run", "list_commands"])
    {
        "process"
    } else if has(READ_OPS, k) || has(WRITE_OPS, k) || has(READ_WRITE_OPS, k) {
        "filesystem"
    } else if has(HOST_OPS, k) || has(NET_OPS, k) {
        "network"
    } else if has(ENV_OPS, k) {
        "environment"
    } else if is(&["if", "if_call", "for_each", "try", "match", "os", "arch"]) {
        "control_flow"
    } else if is(&["timeout", "retry", "parallel", "with_env", "with_cwd", "sandbox"]) {
        "context"
    } else if is(&["wasm_run", "wasm_arg", "wasm_mount_read", "wasm_mount_write", "wasm_env", "wasm_allow_host"]) {
        "wasm"
    } else if is(&[
        "assert_eq", "assert_neq", "assert_contains", "assert_not_contains", "assert_exists",
        "assert_not_exists", "assert_match", "assert_version", "assert_version_ge",
    ]) {
        "assertions"
    } else if is(&["cache", "bundle_hash", "bundle_dir"]) {
        "bundle"
    } else {
        if k.len() > 5 && k.starts_with("http_") {
            return "network";
        }
        if k.len() > 4 && (k.starts_with("path") || is(&["expand_path", "is_abs", "to_slash", "from_slash"])) {
            return "paths";
        }
        if (k.len() >= 4 && k.starts_with("json"))
            || k == "csv_parse"
            || (k.len() >= 6 && k.starts_with("base64"))
            || (k.len() >= 3 && (k.starts_with("hex") || k.starts_with("url")))
        {
            return "encoding";
        }
        if is(&["grep", "reject", "cut", "head", "tail", "sort_lines", "uniq_lines", "count_lines"]) {
            return "text_lines";
        }
        if is(&[
            "trim", "lower", "upper", "contains", "split", "join", "format", "replace", "repeat", "length",
            "capitalize", "has_prefix", "has_suffix",
        ]) {
            return "strings";
        }
        if is(&["md5", "sha1", "sha256", "crc32", "md5_file", "sha1_file", "sha256_file", "verify_sha256"]) {
            return "hashing";
        }
        if is(&["regex_match", "regex_replace", "regex_find_all"]) {
            return "regex";
        }
        if is(&["now", "unix_to_iso"]) {
            return "time";
        }
        if k.len() > 7 && k.starts_with("version") {
            return "version";
        }
        if is(&[
            "which", "has_bin", "bin_version", "pkg_install", "pkg_installed", "pkg_uninstall",
            "detect_pkg_mgr", "path_contains", "shell_rc_path", "add_to_path", "link_into_path",
        ]) {
            return "install";
        }
        "other"
    }
}
