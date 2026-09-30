/// An auto-bound name available as `${name}` in every command body without
/// declaration. Seeded by `seed_globals_and_env` before globals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvidedVar {
    pub name: &'static str,
    pub ty: &'static str,
    pub category: &'static str,
    pub description: &'static str,
}

const fn pv(
    name: &'static str,
    ty: &'static str,
    category: &'static str,
    description: &'static str,
) -> ProvidedVar {
    ProvidedVar { name, ty, category, description }
}

/// The canonical catalog; keep in sync with `seed_globals_and_env`.
pub(crate) static PROVIDED_VARS: &[ProvidedVar] = &[
    // OS / arch
    pv("os", "string", "os", "Host OS: darwin, linux, or windows."),
    pv("arch", "string", "os", "CPU architecture: amd64, arm64, …"),
    pv("is_windows", "bool", "os", "True on Windows."),
    pv("is_macos", "bool", "os", "True on macOS."),
    pv("is_linux", "bool", "os", "True on Linux."),
    pv("is_unix", "bool", "os", "True on non-Windows platforms."),
    pv("is_arm64", "bool", "os", "True when GOARCH is arm64."),
    pv("is_amd64", "bool", "os", "True when GOARCH is amd64."),
    pv("cpu_count", "int", "os", "Number of logical CPUs."),
    pv("pid", "int", "os", "Current process ID."),
    pv("now_unix", "int", "os", "Current Unix timestamp (seconds)."),
    // Path conventions
    pv("path_sep", "string", "paths", "Path separator: / or \\."),
    pv("path_list_sep", "string", "paths", "PATH list separator: : or ;."),
    pv("exe_ext", "string", "paths", "Executable extension (.exe on Windows, empty elsewhere)."),
    pv("null_device", "string", "paths", "Null device path (/dev/null or NUL)."),
    pv("shell_name", "string", "paths", "Default shell name (bash or cmd)."),
    // Standard directories
    pv("home", "string", "paths", "User home directory (alias of home_dir)."),
    pv("home_dir", "string", "paths", "User home directory."),
    pv("config_dir", "string", "paths", "OS user config directory."),
    pv("cache_dir", "string", "paths", "OS user cache directory."),
    pv("data_dir", "string", "paths", "OS user data directory."),
    pv("temp_dir", "string", "paths", "OS temp directory."),
    // Binary / script
    pv("exe_path", "string", "runtime", "Absolute path to the running perch binary."),
    pv("exe_dir", "string", "runtime", "Directory containing the running binary."),
    pv("exe_name", "string", "runtime", "Base name of the running binary."),
    pv("script_path", "string", "runtime", "Absolute path of the loaded .perch file (empty when embedded)."),
    pv("script_dir", "string", "runtime", "Directory containing the loaded .perch file."),
    // Identity
    pv("user", "string", "identity", "Current username."),
    pv("uid", "string", "identity", "User ID (Unix); may be empty on some platforms."),
    pv("hostname", "string", "identity", "Host name."),
];

/// Every auto-bound variable name and metadata.
pub fn provided_vars() -> Vec<ProvidedVar> {
    PROVIDED_VARS.to_vec()
}

/// The set of auto-bound names for static checks.
pub fn provided_var_names() -> std::collections::HashSet<&'static str> {
    PROVIDED_VARS.iter().map(|v| v.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provided_var_names_matches_catalog() {
        let names = provided_var_names();
        assert_eq!(names.len(), PROVIDED_VARS.len());
        for v in PROVIDED_VARS {
            assert!(names.contains(v.name), "missing {:?}", v.name);
        }
    }
}
