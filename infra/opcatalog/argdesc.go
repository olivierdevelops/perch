package opcatalog

var commonArgDesc = map[string]string{
	"path":    "Filesystem path.",
	"src":     "Source path.",
	"dst":     "Destination path.",
	"link":    "Symlink path to create.",
	"msg":     "Message string.",
	"cmd":     "Shell command string (first token is the binary name).",
	"bin":     "Binary name to execute (must be declared in `requires`).",
	"argv":    "Remaining argv tokens (space-separated in source; split at load time).",
	"url":     "HTTP(S) URL.",
	"name":    "Name identifier.",
	"target":  "Command name to invoke.",
	"code":    "Exit code.",
	"host":    "Hostname.",
	"content": "File contents to write.",
	"pattern": "Glob or regex pattern.",
	"key":     "Lookup key.",
	"value":   "Value.",
	"secs":    "Duration in seconds.",
	"text":    "Input text.",
	"pat":     "Pattern string.",
	"n":       "Count or index.",
	"fmt":     "Format string.",
	"sep":     "Separator string.",
	"old":     "Substring or pattern to replace.",
	"new":     "Replacement string.",
	"mode":    "File mode (octal string, e.g. \"0755\").",
	"hash":    "Expected hash (algo:hex).",
}

func argDescription(kind, name, typ string) string {
	if d, ok := perOpArgDesc[kind+"."+name]; ok {
		return d
	}
	if d, ok := commonArgDesc[name]; ok {
		return d
	}
	switch typ {
	case "tail":
		return "Remaining tokens on the line (parsed at load time)."
	case "word":
		return "Bare word token."
	case "ident":
		return "Identifier binding or command name."
	case "string":
		return name + " (string)."
	case "int":
		return name + " (integer)."
	case "any":
		return name + " (any value)."
	default:
		return name
	}
}

// perOpArgDesc holds op-specific arg help where the generic heuristic is wrong.
var perOpArgDesc = map[string]string{
	"exec.bin":        "Binary to run directly (no shell); must be declared in `requires bin \"…\"`.",
	"shell.cmd":       "Full shell command; first token must be a declared `bin`.",
	"download.url":    "URL to fetch (host must be declared in `requires`).",
	"download.dst":    "Local path to write (must fall inside a declared `write` root).",
	"get_env._0":      "Environment variable name (must be declared in `requires env \"…\"`).",
	"set_env._0":      "Environment variable name to set.",
	"http_get._0":     "URL to GET (hostname must be declared in `requires host \"…\"`).",
	"replace._1":      "Replacement spec as \"OLD,NEW\".",
	"version_compat._0": "Required version constraint string.",
}
