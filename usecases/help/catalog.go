package help

// catalog is the hand-curated source of truth for the CLI surface.
// Adding a new flag means appending an entry here AND wiring it in
// io/cli/cli.go — keeping both in this file makes "drift" obvious in
// PR review.
var catalog = []Topic{

	// ─── Execution ────────────────────────────────────────────────────
	{
		Name: "perch <cmd>", Kind: "subcommand", Group: "Execution",
		Synopsis: "run a named command from commands.perch in the cwd",
		Usage:    "perch <cmd> [args...]",
		Examples: []string{"perch deploy", "perch test -race"},
	},
	{
		Name: "-f", Kind: "flag", Group: "Execution",
		Synopsis: "load a specific .perch file (or `-` for stdin)",
		Usage:    "perch -f <file>|- <cmd> [args...]",
		Description: `Override the default commands.perch lookup. The special path "-" reads the
.perch source from stdin — used for pipelines like ` + "`curl URL | perch -f - run`" + `.
When stdin is the source, perch treats the program as untrusted by default
(see --trust-stdin and the --allow-* flags).`,
		Examples: []string{"perch -f deploy.perch up", "curl URL | perch -f - --allow-shell run"},
		SeeAlso:  []string{"--trust-stdin", "--allow-shell"},
		DocURL:   "https://luowensheng.github.io/perch/getting-started/",
	},
	{
		Name: "--help, -h", Kind: "flag", Group: "Execution",
		Synopsis: "list commands in the loaded .perch file",
		Usage:    "perch [--help|-h]   |   perch <cmd> --help",
	},
	{
		Name: "--version", Kind: "flag", Group: "Execution",
		Synopsis: "print perch version", Usage: "perch --version",
	},
	{
		Name: "--server", Kind: "subcommand", Group: "Execution",
		Synopsis:    "serve a web UI for the loaded .perch file",
		Usage:       "perch --server [--port N]",
		Description: `Spawns a small HTTP server on 127.0.0.1:10032 (default) that renders one button per declared command with typed argument forms. /api/exec streams stdout/stderr as NDJSON. Use for handing a .perch surface to non-engineers.`,
		Examples:    []string{"perch --server", "perch --server --port 8080"},
		DocURL:      "https://luowensheng.github.io/perch/llm-control-plane/",
	},
	{
		Name: "--shell", Kind: "subcommand", Group: "Execution",
		Synopsis:    "interactive REPL for the loaded .perch file",
		Usage:       "perch --shell",
		Description: "Type a command name + args; perch runs it with persistent bindings across lines.",
	},

	// ─── Authoring ────────────────────────────────────────────────────
	{
		Name: "--init", Kind: "subcommand", Group: "Authoring",
		Synopsis:    "scaffold a starter commands.perch with shebang + main",
		Usage:       "perch --init",
		Description: `Writes commands.perch in the cwd, sets +x, includes #!/usr/bin/env perch at the top so the file is immediately runnable as a script (./commands.perch). Refuses if a file already exists.`,
		Examples:    []string{"perch --init", "./commands.perch hello"},
		SeeAlso:     []string{"shebang"},
	},
	{
		Name: "--check, --validate", Kind: "subcommand", Group: "Authoring",
		Synopsis:    "static validation of a .perch file",
		Usage:       "perch --check [-f FILE]",
		Description: `Catches typed-arg mismatches, duplicate args, colliding positional indexes, missing run targets, unknown op kinds, unresolved ${name} placeholders. Wire into pre-commit; runs in <100ms on typical files.`,
		Examples:    []string{"perch --check", "perch --check -f deploy.perch"},
		DocURL:      "https://luowensheng.github.io/perch/language/",
	},
	{
		Name: "--scan", Kind: "subcommand", Group: "Authoring",
		Synopsis:    "static security audit (capabilities + risk findings + recommended invocation)",
		Usage:       "perch --scan [-f FILE]",
		Description: `Walks the program (no execution), reports the capabilities it needs (shell/network/writes/subprocess), the env vars it touches, risk findings (sudo, catch passthrough, unvalidated interpolation), and the tightest --no-*/--allow-bin/--env invocation that should still let it run.`,
		Examples:    []string{"perch --scan -f deploy.perch"},
		DocURL:      "https://luowensheng.github.io/perch/sandbox/",
	},
	{
		Name: "--import", Kind: "subcommand", Group: "Authoring",
		Synopsis:    "translate a .sh into a best-effort .perch scaffold",
		Usage:       "perch --import <script.sh> [-o <out.perch>]",
		Description: "Recognises functions, variable assignments, echo, & (background). Preserves bash semantics by routing unrecognised lines through `shell` ops. The output passes --check immediately and runs identically to the original; you progressively promote shell ops to native ops.",
		Examples:    []string{"perch --import deploy.sh"},
		DocURL:      "https://luowensheng.github.io/perch/migrating-from-shell/",
	},

	// ─── Preview / debugging ──────────────────────────────────────────
	{
		Name: "--dry-run", Kind: "flag", Group: "Authoring",
		Synopsis:    "print every op (with interpolated args) and skip execution",
		Usage:       "perch --dry-run <cmd>",
		Description: "Walks the program, prints each op as it would fire (kind + interpolated args + capture target), but never calls the handler. Captures get empty values so subsequent interpolation still works.",
	},
	{
		Name: "--ask", Kind: "flag", Group: "Authoring",
		Synopsis:    "interactive step-through — y/n/a/q per op",
		Usage:       "perch --ask <cmd>",
		Description: "For each op: `y` runs and prompts next, `n` skips, `a` runs this op then all remaining without further prompts, `q` quits. Stacks with --no-shell etc.",
	},

	// ─── Build / distribution ─────────────────────────────────────────
	{
		Name: "--build", Kind: "subcommand", Group: "Build",
		Synopsis:    "bundle a .perch into a self-extracting binary",
		Usage:       "perch --build [-o NAME] [--include PATH]",
		Description: `Produces a portable binary with the parsed .perch embedded inside, plus an optional file tree (--include). Recipients run the binary directly — no Go, no perch install, no source.`,
		Examples:    []string{"perch --build -o deploy", "perch --build --include ./src -o stt_bin"},
		DocURL:      "https://luowensheng.github.io/perch/embedding/",
	},

	// ─── Security: restrictions ───────────────────────────────────────
	{
		Name: "--no-shell", Kind: "flag", Group: "Security",
		Synopsis:    "disable every shell-spawning op",
		Description: "Blocks shell, shell_output, shell_detached, shell_in, try_shell. With --no-shell + --no-subprocess on, perch can't spawn any subprocess.",
		DocURL:      "https://luowensheng.github.io/perch/sandbox/",
		SeeAlso:     []string{"--no-subprocess", "--allow-bin", "--no-shell-metachars"},
	},
	{
		Name: "--no-subprocess", Kind: "flag", Group: "Security",
		Synopsis:    "disable pkg_install, kill_by_name, bin_version, etc.",
		Description: "Blocks ops that fork-exec without going through shell. Closes the subprocess escape hatch for the non-`shell` paths.",
	},
	{
		Name: "--no-network", Kind: "flag", Group: "Security",
		Synopsis:    "disable every network op (http_*, download, dns_lookup, port_*, wait_for_*, IP/MAC discovery)",
	},
	{
		Name: "--no-write", Kind: "flag", Group: "Security",
		Synopsis:    "disable every filesystem-mutation op (write_file, cp, mv, rm, mkdir, chmod, archives, symlinks, bundle_extract, …)",
	},
	{
		Name: "--allow-bin", Kind: "flag", Group: "Security",
		Synopsis: "when shell is allowed, restrict which binaries it can spawn",
		Usage:    "perch --allow-bin NAME[,NAME...] <cmd>",
		Description: `First non-env-assignment token of every shell command must be in the list. Basename-matched, so /usr/local/bin/git matches "git". Multiple --allow-bin flags compose additively.`,
		Examples: []string{"perch --allow-bin git,docker run", "perch --allow-bin=kubectl deploy"},
	},
	{
		Name: "--no-shell-metachars", Kind: "flag", Group: "Security",
		Synopsis:    "reject `|`, `>`, `<`, `&`, `;`, `` ` ``, `$(` in shell args",
		Description: "Stops shell-injection-style escapes inside an otherwise-allowed call. Pair with --allow-bin for `only X calls, no pipes/redirects/subshells`.",
	},
	{
		Name: "--env", Kind: "flag", Group: "Security",
		Synopsis:    "restrict which host env vars resolve via ${NAME}",
		Usage:       "perch --env A,B,C <cmd>   |   perch --env <cmd>  (bare = no env vars visible)",
		Description: `Only listed env vars resolve via interpolation fallthrough. Auto-bound names (${home}, ${cache_dir}, ${is_macos}, …) are not env vars and are unaffected. Bonus: subprocesses inherit only the allowlist too — closes the env-exfiltration escape hatch for shell ops.`,
		Examples:    []string{"perch --env HOME,PATH,API_KEY deploy", "perch --env -f script.perch run"},
	},
	{
		Name: "--max-runtime", Kind: "flag", Group: "Security",
		Synopsis: "wall-clock budget for the whole invocation",
		Usage:    "perch --max-runtime SECS <cmd>",
		Examples: []string{"perch --max-runtime 60 deploy"},
	},
	{
		Name: "--audit", Kind: "flag", Group: "Security",
		Synopsis:    "structured NDJSON trace of every op",
		Usage:       "perch --audit FILE.ndjson <cmd>   (path `-` = stdout)",
		Description: "One line per op call: timestamp, command, kind, interpolated args, duration, ok/error. Plus session-start / session-end. Blocked ops record the denial too — same artifact for security review.",
	},
	{
		Name: "--restrictions", Kind: "flag", Group: "Security",
		Synopsis: "list every --no-* flag with the ops it blocks",
		Usage:    "perch --restrictions",
	},
	{
		Name: "--max-redirects", Kind: "flag", Group: "Security",
		Synopsis:    "cap the redirect-follow chain for http_*/download (default 5)",
		Usage:       "perch --max-redirects N <cmd>",
		Description: "0 is equivalent to --no-redirects. The cap is enforced before any redirect packet is sent — perch never follows past the cap.",
		Examples:    []string{"perch --max-redirects 0 -f script.perch run", "perch --max-redirects 2 deploy"},
	},
	{
		Name: "--no-redirects", Kind: "flag", Group: "Security",
		Synopsis: "refuse to follow any redirect for http_*/download",
		Usage:    "perch --no-redirects <cmd>",
	},
	{
		Name: "--allow-host", Kind: "flag", Group: "Security",
		Synopsis: "restrict HTTP requests + redirects to specific hosts",
		Usage:    "perch --allow-host HOST[,HOST...] <cmd>   (repeatable)",
		Description: `When set, every URL hit by http_get / http_post / download / http_status — INITIAL request AND every redirect destination — must match an entry. Patterns:
   - exact:           api.github.com
   - single-label *:  *.s3.amazonaws.com   (matches api.x.com, NOT a.b.x.com)
   - host:port:       localhost:8080       (port must match exactly)
   - IP literal:      10.0.0.1
Composes AND-wise with the SSRF guard — a host in the allowlist still has to pass the private-IP check unless --allow-private-ips is also set. Multiple --allow-host flags accumulate.`,
		Examples: []string{
			"perch --allow-host api.github.com,registry.docker.io deploy",
			`perch --allow-host "*.example.com" run`,
			"perch --allow-host localhost:8080 --allow-private-ips dev",
		},
		SeeAlso: []string{"--allow-private-ips", "--no-redirects", "--max-redirects"},
	},
	{
		Name: "--allow-private-ips", Kind: "flag", Group: "Security",
		Synopsis:    "permit HTTP requests / redirects to private IPs",
		Description: "By default perch refuses to dial any host that resolves to a loopback (127.0.0.0/8, ::1), link-local (169.254.0.0/16 — AWS metadata service!), private (RFC 1918 / IPv6 ULA), or unspecified address. Pass this flag when the script needs to talk to a localhost service or your internal network.",
		SeeAlso:     []string{"--no-network", "--allow-scheme-downgrade"},
	},
	{
		Name: "--allow-scheme-downgrade", Kind: "flag", Group: "Security",
		Synopsis:    "permit https → http redirect chains",
		Description: "By default a 30x from https://X to http://Y is refused. Pass this flag for legacy endpoints where the downgrade is intentional.",
	},

	// ─── Security: stdin-only allow flags ─────────────────────────────
	{
		Name: "--allow-shell", Kind: "flag", Group: "Security",
		Synopsis:    "(stdin only) re-enable shell ops",
		Description: "When -f - reads from stdin, perch defaults to the strictest posture. --allow-shell opts in to shell. No-op for file input.",
		SeeAlso:     []string{"--trust-stdin"},
	},
	{
		Name: "--allow-subprocess", Kind: "flag", Group: "Security",
		Synopsis: "(stdin only) re-enable subprocess ops",
	},
	{
		Name: "--allow-network", Kind: "flag", Group: "Security",
		Synopsis: "(stdin only) re-enable network ops",
	},
	{
		Name: "--allow-write", Kind: "flag", Group: "Security",
		Synopsis: "(stdin only) re-enable filesystem-mutation ops",
	},
	{
		Name: "--trust-stdin", Kind: "flag", Group: "Security",
		Synopsis:    "skip the deny-by-default applied to stdin input",
		Description: "Use when you piped a .perch file you wrote yourself and want the file-input posture (everything allowed unless explicitly restricted).",
	},

	// ─── Agents / editor integration ──────────────────────────────────
	{
		Name: "perch-mcp", Kind: "subcommand", Group: "Agents",
		Synopsis:    "MCP server — exposes commands as tools for AI agents",
		Usage:       "perch-mcp -f <file>",
		Description: "Model Context Protocol server (JSON-RPC over stdio). Tools: perch_list, perch_run. Configure Claude Desktop / Cursor / Zed to launch it.",
		DocURL:      "https://luowensheng.github.io/perch/mcp/",
		SeeAlso:     []string{"--allow-bin", "--no-shell", "--audit"},
	},
	{
		Name: "perch-lsp", Kind: "subcommand", Group: "Agents",
		Synopsis: "Language Server Protocol server for .perch authoring",
		DocURL:   "https://luowensheng.github.io/perch/lsp/",
	},
	{
		Name: "--install-lsp", Kind: "subcommand", Group: "Agents",
		Synopsis: "install the perch-lsp binary via `go install`",
	},
	{
		Name: "--install-vscode", Kind: "subcommand", Group: "Agents",
		Synopsis: "install the bundled VS Code extension",
	},
	{
		Name: "--completions", Kind: "subcommand", Group: "Agents",
		Synopsis: "emit a shell completion script",
		Usage:    "perch --completions bash|zsh|fish",
	},

	// ─── Concepts ─────────────────────────────────────────────────────
	{
		Name: "shebang", Kind: "concept", Group: "Concepts",
		Synopsis:    "running a .perch file directly via #!/usr/bin/env perch",
		Description: "perch --init writes the shebang line at the top of commands.perch and sets the file +x. Then ./commands.perch invokes the `main` command (or lists commands if no `main`); ./commands.perch hello runs hello explicitly. The shebang is a # comment to capy's parser — no grammar change.",
		Examples:    []string{"perch --init && chmod +x commands.perch && ./commands.perch"},
	},
	{
		Name: "interpolation", Kind: "concept", Group: "Concepts",
		Synopsis:    "${name} substitution inside string args",
		Description: "Resolution order: command args → globals → per-command env → host env (host scope respects --env). Auto-bound names (home, cache_dir, exe_path, is_macos, …) are always available. The three string delimiters \"...\", '...', `...` are all raw — no backslash escapes — and all do interpolation. Pick the one that doesn't appear in your content.",
		DocURL:      "https://luowensheng.github.io/perch/language/#interpolation",
	},
	{
		Name: "auto-bindings", Kind: "concept", Group: "Concepts",
		Synopsis:    "~30 variables every command starts with already bound",
		Description: "os, arch, home, cache_dir, config_dir, data_dir, temp_dir, exe_path, exe_dir, script_path, script_dir, path_sep, exe_ext, null_device, cpu_count, user, hostname, is_windows, is_macos, is_linux, is_unix, is_arm64, is_amd64, pid, now_unix, … — the building blocks of cross-platform install/build/uninstall scripts.",
	},
	{
		Name: "sandbox", Kind: "concept", Group: "Concepts",
		Synopsis: "the full capability-restriction model",
		DocURL:   "https://luowensheng.github.io/perch/sandbox/",
	},
}
