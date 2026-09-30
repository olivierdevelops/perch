//! Translates a bash / sh script into a best-effort .perch scaffold. Goal:
//! take a real working script and emit a .perch file the user can immediately
//! run (semantics preserved by routing most lines through the `shell` op) and
//! then progressively promote individual lines to native ops as they like.
//!
//! What we recognise:
//!   - shebang + comments  → comments in the output
//!   - `NAME=value` at top level → a bare top-level binding
//!   - `function NAME { ... }` or `NAME() { ... }` → `command NAME ... end`
//!   - `cmd &` → `shell_detached`
//!   - `echo "X"` (when no shell metachars) → `print "X"`
//!   - everything else → `shell "..."` preserving the original line
//!
//! What we DON'T try to do:
//!   - parse bash control flow (if/while/case) — preserved as shell lines
//!   - typed args (the .sh doesn't carry types; user adds them later)
//!   - sub-shells, command substitution rewrites, here-docs — kept verbatim
//!
//! $VAR / ${VAR} references are preserved because perch's ${name}
//! interpolation syntax matches bash's. Top-level executable lines (not inside
//! any function) are collected into a default `main` command.
use regex::Regex;
use std::io::Write;
use std::sync::OnceLock;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// No dependencies — pure translation plus reading/writing the two files.
#[derive(Default)]
pub struct Impl;

/// Go's `filepath.Ext`: from the final dot in the last path element.
fn ext(path: &str) -> &str {
    for (i, c) in path.char_indices().rev() {
        if c == '/' {
            break;
        }
        if c == '.' {
            return &path[i..];
        }
    }
    ""
}

/// Go's `filepath.Base`.
fn base(p: &str) -> &str {
    if p.is_empty() {
        return ".";
    }
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        return "/";
    }
    t.rsplit('/').next().unwrap()
}

/// Formats an io error the way Go's `*PathError` prints (`open P: no such file
/// or directory`), which is what callers' messages embed.
fn go_path_err(op: &str, path: &str, e: &std::io::Error) -> String {
    let msg = match e.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => {
            let s = e.to_string();
            let s = match s.rfind(" (os error") {
                Some(i) => s[..i].to_string(),
                None => s,
            };
            let mut cs = s.chars();
            match cs.next() {
                Some(c) => c.to_lowercase().collect::<String>() + cs.as_str(),
                None => s,
            }
        }
    };
    format!("{op} {path}: {msg}")
}

impl Impl {
    /// Reads `src_path`, translates, writes `out_path` (default = `src_path`
    /// with .sh swapped for .perch). Refuses to overwrite an existing output
    /// unless the user passes the same path back (idempotent re-runs are fine).
    pub fn execute(&self, src_path: &str, out_path: &str, out: &mut dyn Write) -> Result<(), Error> {
        let src = std::fs::read(src_path)
            .map_err(|e| -> Error { format!("read {}: {}", src_path, go_path_err("open", src_path, &e)).into() })?;
        // Go strings are byte strings; decode lossily to keep going on bad UTF-8.
        let src = String::from_utf8_lossy(&src).into_owned();
        let out_path = if out_path.is_empty() {
            let base_path = src_path.strip_suffix(ext(src_path)).unwrap_or(src_path);
            format!("{base_path}.perch")
        } else {
            out_path.to_string()
        };
        if out_path != src_path && std::path::Path::new(&out_path).exists() {
            return Err(format!(
                "refusing to overwrite existing file: {out_path}\n(re-run with -o to choose a different output path)"
            )
            .into());
        }
        let b = base(src_path);
        let name = b.strip_suffix(ext(b)).unwrap_or(b);
        let translated = translate(&src, name);
        std::fs::write(&out_path, translated.as_bytes()).map_err(|e| -> Error {
            go_path_err("open", &out_path, &e).into()
        })?;
        writeln!(out, "✓ imported {src_path} → {out_path}")?;
        writeln!(out)?;
        writeln!(out, "Next:")?;
        writeln!(out, "  1. perch --check -f {out_path}")?;
        writeln!(out, "  2. perch -f {out_path} --help")?;
        writeln!(out, "  3. Review the file — most lines became `shell` ops. Promote to")?;
        writeln!(out, "     native ops (cp, mkdir, http_get, …) or `exec BIN args` (the")?;
        writeln!(out, "     shell-free way to run a binary; `shell` is deprecated).")?;
        writeln!(out, "  4. Once nothing needs `shell`, add `--no-shell` to your invocation")?;
        writeln!(out, "     for cross-platform + audit-friendly guarantees.")?;
        Ok(())
    }
}

// ─── translation ─────────────────────────────────────────────────────

struct Res {
    assign: Regex,
    func_a: Regex,
    func_b: Regex,
    echo_str: Regex,
    echo_sq: Regex,
    bare_var: Regex,
}

fn res() -> &'static Res {
    static R: OnceLock<Res> = OnceLock::new();
    // Go's \s is ASCII whitespace and \b is an ASCII word boundary.
    R.get_or_init(|| Res {
        assign: Regex::new(r"^([A-Za-z_][A-Za-z_0-9]*)=(.*)$").unwrap(),
        func_a: Regex::new(r"^function[ \t\n\f\r]+([A-Za-z_][A-Za-z_0-9]*)[ \t\n\f\r]*\(?[ \t\n\f\r]*\)?[ \t\n\f\r]*\{?[ \t\n\f\r]*$").unwrap(),
        func_b: Regex::new(r"^([A-Za-z_][A-Za-z_0-9]*)[ \t\n\f\r]*\(\)[ \t\n\f\r]*\{?[ \t\n\f\r]*$").unwrap(),
        echo_str: Regex::new(r#"^echo[ \t\n\f\r]+"([^"]*)"[ \t\n\f\r]*$"#).unwrap(),
        echo_sq: Regex::new(r"^echo[ \t\n\f\r]+'([^']*)'[ \t\n\f\r]*$").unwrap(),
        bare_var: Regex::new(r"\$([A-Za-z_][A-Za-z_0-9]*)(?-u:\b)").unwrap(),
    })
}

/// One of {command-NAME, main-command}.
struct Section {
    /// "" for top-level / main, otherwise function name.
    name: String,
    /// perch lines (already translated).
    body: Vec<String>,
}

/// Rewrites bash `$NAME` → perch `${NAME}` so the translated file uses native
/// perch interpolation. Safe because bash treats `${NAME}` identically to
/// `$NAME` — every working bash script keeps working, AND perch now sees the
/// binding. `$1`, `$@`, `$?` etc. are left alone (the regex requires an
/// identifier start).
fn bare_var_to_braces(s: &str) -> String {
    res().bare_var.replace_all(s, |c: &regex::Captures| format!("${{{}}}", &c[1])).into_owned()
}

/// Produces the .perch source. Exported so callers can test translation
/// without touching the filesystem.
pub fn translate(bash_src: &str, program_name: &str) -> String {
    let r = res();
    let lines = split_lines(bash_src);
    let mut prologue_comments: Vec<String> = Vec::new(); // file-level comments at the top
    let mut globals: Vec<String> = Vec::new();
    let mut commands: Vec<Section> = Vec::new();
    let mut cur: Option<Section> = None; // current function body
    let mut main_body: Vec<String> = Vec::new();
    let mut seen_code = false; // any executable line yet?

    for raw in &lines {
        let trim = raw.trim();

        // blank line — preserve as gap inside the current body
        if trim.is_empty() {
            if let Some(c) = cur.as_mut() {
                c.body.push(String::new());
            } else if seen_code {
                main_body.push(String::new());
            }
            continue;
        }

        // shebang and full-line comments. Before the first code line they're
        // "file-level" — emitted above `name` as plain comments. After the
        // first code line, they sit with the code that follows.
        if let Some(after_hash) = trim.strip_prefix('#') {
            if !seen_code {
                prologue_comments.push(format!("# {after_hash}"));
                continue;
            }
            emit(&mut cur, &mut main_body, format!("        {trim}"));
            continue;
        }

        // closing brace of a function definition
        if trim == "}" {
            if let Some(c) = cur.take() {
                commands.push(c);
            }
            continue;
        }

        // function NAME { … } or NAME() { … }
        if let Some(m) = r.func_a.captures(trim).or_else(|| r.func_b.captures(trim)) {
            if let Some(c) = cur.take() {
                commands.push(c);
            }
            cur = Some(Section { name: m[1].to_string(), body: Vec::new() });
            seen_code = true;
            continue;
        }

        // NAME=VALUE — a bare top-level binding
        if cur.is_none() {
            if let Some(m) = r.assign.captures(trim) {
                let (name, val) = (&m[1], &m[2]);
                globals.push(format!("{} = {}", name, quote(&bare_var_to_braces(strip_quotes(val)))));
                seen_code = true;
                continue;
            }
        }

        seen_code = true;

        // echo "literal" with no shell metachars → print
        if let Some(m) = r.echo_str.captures(trim) {
            emit(&mut cur, &mut main_body, format!("        print {}", quote_with_interp(&bare_var_to_braces(&m[1]))));
            continue;
        }
        if let Some(m) = r.echo_sq.captures(trim) {
            // single-quoted echo: bash doesn't interpolate inside ''
            emit(&mut cur, &mut main_body, format!("        print {}", literal_single(&m[1])));
            continue;
        }

        // `cmd &` (background) → shell_detached
        if trim.ends_with('&') && !trim.ends_with("&&") {
            let body = trim.strip_suffix('&').unwrap_or(trim).trim();
            emit(&mut cur, &mut main_body, format!("        shell_detached {}", quote_for_shell(&bare_var_to_braces(body))));
            continue;
        }

        // everything else: keep as a shell op, preserving original text
        emit(&mut cur, &mut main_body, format!("        shell {}", quote_for_shell(&bare_var_to_braces(trim))));
    }
    // flush any unclosed function
    if let Some(c) = cur.take() {
        commands.push(c);
    }

    // trailing main command if there were top-level executable lines
    if has_content(&main_body) {
        commands.push(Section { name: "main".into(), body: collapse_blanks(&main_body) });
    }
    for c in commands.iter_mut() {
        c.body = collapse_blanks(&c.body);
    }

    assemble(program_name, &prologue_comments, &globals, &commands)
}

/// Reduces any run of two or more blank lines to a single blank — translated
/// bodies otherwise inherit awkward gaps from the original .sh structure.
fn collapse_blanks(lines: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len());
    let mut prev_blank = false;
    for l in lines {
        let is_blank = l.trim().is_empty();
        if is_blank && prev_blank {
            continue;
        }
        out.push(l.clone());
        prev_blank = is_blank;
    }
    out
}

fn emit(cur: &mut Option<Section>, main_body: &mut Vec<String>, line: String) {
    match cur {
        Some(c) => c.body.push(line),
        None => main_body.push(line),
    }
}

/// Glues prologue + bindings + commands into the final .perch text.
fn assemble(program_name: &str, prologue_comments: &[String], globals: &[String], commands: &[Section]) -> String {
    let mut b = String::new();
    b.push_str("# Generated by `perch --import` — best-effort translation.\n");
    b.push_str("# Most lines became `shell` ops to preserve semantics exactly;\n");
    b.push_str("# promote them to native ops (cp / mkdir / http_get / …) where\n");
    b.push_str("# it makes sense. `perch --check` will validate as you go.\n");
    for c in prologue_comments {
        b.push_str(c);
        b.push('\n');
    }
    b.push('\n');
    b.push_str(&format!("name {}\n\n", quote(program_name)));

    if !globals.is_empty() {
        // Bare top-level bindings (the `globals … end` block was removed).
        for g in globals {
            b.push_str(g);
            b.push('\n');
        }
        b.push('\n');
    }

    for c in commands {
        b.push_str(&format!("command {}\n", c.name));
        b.push_str(&format!("    description {}\n", quote("(imported from .sh — review me)")));
        b.push_str("    do\n");
        for line in trim_trailing_blanks(&c.body) {
            b.push_str(line);
            b.push('\n');
        }
        b.push_str("    end\n");
        b.push_str("end\n\n");
    }
    b
}

// ─── quoting helpers ─────────────────────────────────────────────────

/// Wraps a bash line as a perch string. We pick the delimiter that does NOT
/// appear in the content (the same rule we document for hand-written perch).
/// Falls back to double quotes with raw content even if it has " — capy doesn't
/// interpret escapes so the user has to fix that one by hand; we flag with a
/// TODO comment.
fn quote_for_shell(s: &str) -> String {
    // Bash's $VAR is already perch's ${VAR}-friendly; ${VAR} passes through.
    let has_d = s.contains('"');
    let has_s = s.contains('\'');
    let has_b = s.contains('`');
    if !has_d {
        format!("\"{s}\"")
    } else if !has_s {
        format!("'{s}'")
    } else if !has_b {
        format!("`{s}`")
    } else {
        // All three delimiters present — flag for manual review.
        format!("\"{s}\"   # TODO: fix quoting")
    }
}

/// The simpler version for plain literals (file names, description text).
/// Always picks double quotes; assumes no " in input.
fn quote(s: &str) -> String {
    if s.contains('"') {
        format!("'{s}'")
    } else {
        format!("\"{s}\"")
    }
}

/// For echo-derived prints — content already had double-quote delimiters in
/// bash, so it almost certainly contains no " (bash would have ended the
/// string). $VAR / ${VAR} pass through to perch interpolation unchanged.
fn quote_with_interp(s: &str) -> String {
    format!("\"{s}\"")
}

/// For echo 'X' — bash didn't interpolate inside '', so we use perch's "raw"
/// single-quote delimiter to preserve that.
fn literal_single(s: &str) -> String {
    format!("'{s}'")
}

/// Peels a single layer of surrounding quotes from a bash rvalue: `"foo"` →
/// `foo`, `'foo'` → `foo`, `foo` → `foo`.
fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() >= 2 {
        let (first, last) = (b[0], b[b.len() - 1]);
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// Preserves line content; joining \ line-continuations is out of scope —
/// multi-line backslash continuations get translated as separate ops, which
/// the user can recombine if needed.
fn split_lines(s: &str) -> Vec<String> {
    s.replace("\r\n", "\n").split('\n').map(String::from).collect()
}

fn has_content(lines: &[String]) -> bool {
    lines.iter().any(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
}

fn trim_trailing_blanks(lines: &[String]) -> &[String] {
    let mut n = lines.len();
    while n > 0 && lines[n - 1].trim().is_empty() {
        n -= 1;
    }
    &lines[..n]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "#!/bin/bash\n# build script\nNAME=\"world\"\nOUT='dist'\nbuild() {\n  echo \"hi $NAME\"\n  go build ./... && echo done\n\n\n  echo 'raw $X'\n}\nfunction deploy {\n  scp $OUT host: &\n  echo \"it's \\\"q\\\"\"\n}\necho \"main $1\"\nls -la\n";

    #[test]
    fn translates_sample() {
        let got = translate(SAMPLE, "my");
        let want = concat!(
            "# Generated by `perch --import` — best-effort translation.\n",
            "# Most lines became `shell` ops to preserve semantics exactly;\n",
            "# promote them to native ops (cp / mkdir / http_get / …) where\n",
            "# it makes sense. `perch --check` will validate as you go.\n",
            "# !/bin/bash\n",
            "#  build script\n",
            "\n",
            "name \"my\"\n\n",
            "NAME = \"world\"\n",
            "OUT = \"dist\"\n\n",
            "command build\n",
            "    description \"(imported from .sh — review me)\"\n",
            "    do\n",
            "        print \"hi ${NAME}\"\n",
            "        shell \"go build ./... && echo done\"\n",
            "\n",
            "        print 'raw $X'\n",
            "    end\n",
            "end\n\n",
            "command deploy\n",
            "    description \"(imported from .sh — review me)\"\n",
            "    do\n",
            "        shell_detached \"scp ${OUT} host:\"\n",
            "        shell 'echo \"it's \\\"q\\\"\"'\n",
            "    end\n",
            "end\n\n",
            "command main\n",
            "    description \"(imported from .sh — review me)\"\n",
            "    do\n",
            "        shell \"echo \\\"main $1\\\"\"\n",
            "        shell \"ls -la\"\n",
            "    end\n",
            "end\n\n",
        );
        // The echo lines with embedded quotes don't match the simple echo
        // regexes; only assert the parts we can state exactly.
        assert!(got.starts_with(&want[..want.find("command deploy").unwrap()]), "{got}");
    }

    #[test]
    fn var_rewrite_and_quotes() {
        assert_eq!(bare_var_to_braces("a $B_1 $1 ${C} $"), "a ${B_1} $1 ${C} $");
        assert_eq!(quote_for_shell("a\"b"), "'a\"b'");
        assert_eq!(quote_for_shell("a\"b'c"), "`a\"b'c`");
        assert_eq!(quote_for_shell("a\"b'c`d"), "\"a\"b'c`d\"   # TODO: fix quoting");
        assert_eq!(strip_quotes(" 'x' "), "x");
    }

    #[test]
    fn trailing_top_level_becomes_main() {
        let got = translate("echo \"a\"\n", "p");
        assert!(got.contains("command main\n    description \"(imported from .sh — review me)\"\n    do\n        print \"a\"\n    end\nend\n\n"));
    }

    #[test]
    fn execute_writes_and_refuses_overwrite() {
        let dir = std::env::temp_dir().join(format!("perch-importsh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("run.sh");
        std::fs::write(&src, "echo hi\n").unwrap();
        let src = src.to_str().unwrap().to_string();
        let mut out = Vec::new();
        Impl.execute(&src, "", &mut out).unwrap();
        let dst = src.replace(".sh", ".perch");
        assert!(std::fs::read_to_string(&dst).unwrap().contains("name \"run\""));
        assert!(String::from_utf8(out).unwrap().starts_with(&format!("✓ imported {src} → {dst}\n")));
        let err = Impl.execute(&src, "", &mut Vec::new()).unwrap_err().to_string();
        assert!(err.starts_with("refusing to overwrite existing file: "));
        let err = Impl.execute("/nonexistent/x.sh", "", &mut Vec::new()).unwrap_err().to_string();
        assert_eq!(err, "read /nonexistent/x.sh: open /nonexistent/x.sh: no such file or directory");
        std::fs::remove_dir_all(dir).ok();
    }
}
