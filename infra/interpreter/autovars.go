package interpreter

// ProvidedVar is an auto-bound name available as ${name} in every command
// body without declaration. Seeded by seedGlobalsAndEnv before globals.
type ProvidedVar struct {
	Name        string
	Type        string
	Category    string
	Description string
}

// providedVars is the canonical catalog; keep in sync with seedGlobalsAndEnv.
var providedVars = []ProvidedVar{
	// OS / arch
	{Name: "os", Type: "string", Category: "os", Description: "Host OS: darwin, linux, or windows."},
	{Name: "arch", Type: "string", Category: "os", Description: "CPU architecture: amd64, arm64, …"},
	{Name: "is_windows", Type: "bool", Category: "os", Description: "True on Windows."},
	{Name: "is_macos", Type: "bool", Category: "os", Description: "True on macOS."},
	{Name: "is_linux", Type: "bool", Category: "os", Description: "True on Linux."},
	{Name: "is_unix", Type: "bool", Category: "os", Description: "True on non-Windows platforms."},
	{Name: "is_arm64", Type: "bool", Category: "os", Description: "True when GOARCH is arm64."},
	{Name: "is_amd64", Type: "bool", Category: "os", Description: "True when GOARCH is amd64."},
	{Name: "cpu_count", Type: "int", Category: "os", Description: "Number of logical CPUs."},
	{Name: "pid", Type: "int", Category: "os", Description: "Current process ID."},
	{Name: "now_unix", Type: "int", Category: "os", Description: "Current Unix timestamp (seconds)."},
	// Path conventions
	{Name: "path_sep", Type: "string", Category: "paths", Description: "Path separator: / or \\."},
	{Name: "path_list_sep", Type: "string", Category: "paths", Description: "PATH list separator: : or ;."},
	{Name: "exe_ext", Type: "string", Category: "paths", Description: "Executable extension (.exe on Windows, empty elsewhere)."},
	{Name: "null_device", Type: "string", Category: "paths", Description: "Null device path (/dev/null or NUL)."},
	{Name: "shell_name", Type: "string", Category: "paths", Description: "Default shell name (bash or cmd)."},
	// Standard directories
	{Name: "home", Type: "string", Category: "paths", Description: "User home directory (alias of home_dir)."},
	{Name: "home_dir", Type: "string", Category: "paths", Description: "User home directory."},
	{Name: "config_dir", Type: "string", Category: "paths", Description: "OS user config directory."},
	{Name: "cache_dir", Type: "string", Category: "paths", Description: "OS user cache directory."},
	{Name: "data_dir", Type: "string", Category: "paths", Description: "OS user data directory."},
	{Name: "temp_dir", Type: "string", Category: "paths", Description: "OS temp directory."},
	// Binary / script
	{Name: "exe_path", Type: "string", Category: "runtime", Description: "Absolute path to the running perch binary."},
	{Name: "exe_dir", Type: "string", Category: "runtime", Description: "Directory containing the running binary."},
	{Name: "exe_name", Type: "string", Category: "runtime", Description: "Base name of the running binary."},
	{Name: "script_path", Type: "string", Category: "runtime", Description: "Absolute path of the loaded .perch file (empty when embedded)."},
	{Name: "script_dir", Type: "string", Category: "runtime", Description: "Directory containing the loaded .perch file."},
	// Identity
	{Name: "user", Type: "string", Category: "identity", Description: "Current username."},
	{Name: "uid", Type: "string", Category: "identity", Description: "User ID (Unix); may be empty on some platforms."},
	{Name: "hostname", Type: "string", Category: "identity", Description: "Host name."},
}

// ProvidedVars returns a copy of every auto-bound variable name and metadata.
func ProvidedVars() []ProvidedVar {
	out := make([]ProvidedVar, len(providedVars))
	copy(out, providedVars)
	return out
}

// ProvidedVarNames returns the set of auto-bound names for static checks.
func ProvidedVarNames() map[string]bool {
	m := make(map[string]bool, len(providedVars))
	for _, v := range providedVars {
		m[v.Name] = true
	}
	return m
}
