use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use capy_core::capy::Library;
use perch_domain::{
    ArgSpec, BinReq, BundleAlias, Command, EnvReq, GlobalBinding, Hook, HostReq, Op, Program,
    Template,
};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::enforce::{enforce_zero_ambient, op_set};
use crate::error::{wrap, CapyParseError, Error};
use crate::registry::check_name_registry;

/// The embedded `lib.capy` grammar source.
static LIBRARY_SOURCE: &str = include_str!("../lib.capy");

/// Returns the embedded `lib.capy` grammar source.
pub fn library_source() -> &'static str {
    LIBRARY_SOURCE
}

/// The canonical list of built-in op kinds (one per line), generated from
/// `ops.BuiltinKinds()`. The loader needs this to disambiguate a bare
/// capture's leading name — `let s = sha256 "x"` (op) vs `let r = docker ps`
/// (declared bin) — at load time, before any interpreter handler map is
/// available. Most value-returning ops (file_size, get_env, sha256_file, …)
/// have no dedicated grammar keyword: they are reachable only via the generic
/// capture form, so they wouldn't appear in the grammar-derived op vocabulary.
/// A drift test in infra/ops keeps this file in sync with the handler registry.
static OP_KINDS_SOURCE: &str = include_str!("../opkinds.txt");

pub(crate) fn op_kinds_source() -> &'static str {
    OP_KINDS_SOURCE
}

/// Returns the sorted canonical built-in op-kind names the loader knows about
/// (from the embedded opkinds.txt). Exposed so a drift test in infra/ops can
/// assert it stays in sync with the handler registry.
pub fn op_kinds() -> Vec<String> {
    op_set().iter().cloned().collect()
}

/// Parses a .perch source file and returns a Program, recursively resolving
/// any `import "PATH"` directives.
///
/// Special case: path == "-" reads the source from stdin. Imports from a
/// stdin-loaded program are resolved against cwd (no $0 to derive a directory
/// from). This is what makes piping work:
///
/// ```text
/// curl -fsSL https://.../commands.perch | perch -f - <command>
/// ```
///
/// Imports form a graph: cycles are detected and reported (not followed). Each
/// file's transitive imports are merged into the root program's command set —
/// flat by default, namespaced when written as `import "X" as ALIAS` (commands
/// become callable as `ALIAS.name`).
pub fn load(path: &str) -> Result<Program, Error> {
    let prog = load_recursive(path, &mut HashSet::new())?;
    check_name_registry(&prog)?;
    let mut prog = prog;
    enforce_zero_ambient(&mut prog)?;
    Ok(prog)
}

/// Like [`load`] but reads from an in-memory string. Imports declared in the
/// source ARE resolved (against cwd), so a string passed via the REPL or the
/// embedded MCP server can still pull in sibling files.
pub fn load_from_string(script_src: &str) -> Result<Program, Error> {
    let (prog, imports) = parse_once(script_src)?;
    let mut merged = resolve_imports(prog, imports, "", &mut HashSet::new())?;
    check_name_registry(&merged)?;
    enforce_zero_ambient(&mut merged)?;
    Ok(merged)
}

/// The file-backed entry point. `visited` tracks every absolute path on the
/// current import stack so cycles surface as a helpful error rather than
/// infinite recursion.
fn load_recursive(path: &str, visited: &mut HashSet<String>) -> Result<Program, Error> {
    let (src, abs_path);
    if path == "-" {
        let mut data = Vec::new();
        std::io::stdin()
            .read_to_end(&mut data)
            .map_err(|e| wrap("read stdin", e))?;
        src = String::from_utf8_lossy(&data).into_owned();
        abs_path = "-".to_string();
    } else {
        abs_path = go_abs(path);
        if visited.contains(&abs_path) {
            return Err(format!(
                "import cycle detected: {abs_path} already on the import stack"
            )
            .into());
        }
        src = read_file(path)?;
    }

    let (mut prog, imports) = parse_once(&src)?;
    prog.script_path = abs_path.clone();

    // Mark this file as visited for the duration of its subtree.
    visited.insert(abs_path.clone());

    // Directory that import paths are resolved against. For stdin, imports
    // resolve against cwd.
    let import_base = if abs_path != "-" {
        go_dir(&abs_path)
    } else {
        String::new()
    };
    let res = resolve_imports(prog, imports, &import_base, visited);
    visited.remove(&abs_path);
    res
}

/// Reads a file, phrasing failures like Go's `read PATH: open PATH: no such
/// file or directory`.
fn read_file(path: &str) -> Result<String, Error> {
    let fail = |e: std::io::Error| -> Error {
        format!("read {path}: open {path}: {}", go_io_message(&e)).into()
    };
    if Path::new(path).is_dir() {
        return Err(format!("read {path}: read {path}: is a directory").into());
    }
    let data = std::fs::read(path).map_err(fail)?;
    Ok(String::from_utf8_lossy(&data).into_owned())
}

/// Renders an io error the way Go's syscall errors read (lowercase, no
/// `(os error N)` suffix).
fn go_io_message(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "no such file or directory".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => {
            let s = e.to_string();
            let s = match s.find(" (os error") {
                Some(i) => s[..i].to_string(),
                None => s,
            };
            let mut cs = s.chars();
            match cs.next() {
                Some(c) => c.to_lowercase().collect::<String>() + cs.as_str(),
                None => s,
            }
        }
    }
}

/// Runs one file through capy and returns the program plus the list of import
/// directives encountered. Pure (no IO).
fn parse_once(script_src: &str) -> Result<(Program, Vec<ImportDirective>), Error> {
    let lib = shared_library().map_err(|e| wrap("compile perch library", e))?;
    let script_src = mark_env_prefixed_statements(script_src);
    let stream = lib.run(&script_src).map_err(|e| {
        let pe = CapyParseError {
            msg: if e.plain { e.to_string() } else { e.msg.clone() },
            hint: e.hint.clone(),
            line: e.line,
            col: e.col,
        };
        wrap("parse script", pe)
    })?;
    parse_event_stream(&stream).map_err(Into::into)
}

/// The capy engine is the native `capy-core` crate. The library source is a
/// compile-time constant, so it is compiled once and shared: a loader run
/// parses one file per import, and `run` is safe for concurrent use.
fn shared_library() -> Result<&'static Library, Error> {
    static ENGINE: OnceLock<Result<Library, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| Library::new(LIBRARY_SOURCE).map_err(|e| e.msg))
        .as_ref()
        .map_err(|e| e.clone().into())
}

/// Loads each import target and merges its commands + globals into `prog`.
/// Errors carry the importing file's directive so debugging an "import not
/// found" error doesn't require git-bisecting the import graph.
///
/// The raw path string supports `${name}` substitution before filesystem
/// resolution — see [`expand_import_path`]. This is what makes
///
/// ```text
/// import "${file_dir}/shared/aws.perch" as aws
/// ```
///
/// portable across machines and machine-independent of the cwd at invocation
/// time.
fn resolve_imports(
    mut prog: Program,
    imports: Vec<ImportDirective>,
    base: &str,
    visited: &mut HashSet<String>,
) -> Result<Program, Error> {
    for imp in &imports {
        let ctx = format!("import {}", go_quote(&imp.path));
        let expanded = expand_import_path(&imp.path, base).map_err(|e| wrap(ctx.clone(), e))?;
        let mut target = expanded;
        if !Path::new(&target).is_absolute() && !base.is_empty() {
            target = go_join(base, &target);
        }
        let sub = load_recursive(&target, visited).map_err(|e| wrap(ctx.clone(), e))?;
        merge_program(&mut prog, sub, &imp.alias).map_err(|e| wrap(ctx.clone(), e))?;
    }
    // Resolve bare-name dispatch BEFORE template expansion: a bare `deploy` /
    // `ensure_dir "x"` was folded by the grammar into an implicit `exec` op;
    // now that the full command + template sets are merged, rewrite those
    // whose bin is actually a command (→ `run`) or a template (→
    // `_template_call`, which the expansion pass below then inlines). Bins are
    // left as exec. This is what lets `run`/`call` be dropped from the surface.
    resolve_bare_dispatch(&mut prog)?;

    // Re-run template expansion now that imported templates are merged into
    // prog.templates. parse_event_stream did a first pass on the parent
    // file's own templates; that pass leaves `_template_call` markers for any
    // template the parent didn't define locally. Now that imported templates
    // are visible, expand those remaining calls.
    if !prog.templates.is_empty() {
        expand_all_templates(&mut prog)?;
    }
    Ok(prog)
}

/// Rewrites implicit-exec ops (`deploy`, `ensure_dir "x"`) whose leading name
/// is a command or template into the corresponding `run` / `_template_call`
/// op. The unique-name registry guarantees the name resolves to at most one of
/// {command, template, bin}, so the mapping is unambiguous. Names that are
/// neither command nor template stay as exec (a real bin, or an
/// undeclared-bin error raised later by `enforce_zero_ambient`).
fn resolve_bare_dispatch(prog: &mut Program) -> Result<(), String> {
    let cmds: BTreeSet<String> = prog.commands.keys().cloned().collect();
    let tmpls: BTreeSet<String> = prog.templates.keys().cloned().collect();
    let req = prog.requirements.clone();
    let op_kinds = op_set();

    struct Ctx<'a> {
        cmds: &'a BTreeSet<String>,
        tmpls: &'a BTreeSet<String>,
        req: &'a perch_domain::Requirements,
        op_kinds: &'a BTreeSet<String>,
    }

    fn walk(ops: &mut [Op], cx: &Ctx) -> Result<(), String> {
        for op in ops.iter_mut() {
            // R05 (decision D2): an inline `NAME=VALUE` prefix belongs to a
            // declared-bin call or `exec` only — never a built-in op, a command
            // or a template call.
            if op.kind == "exec" && op.args.contains_key("env_prefix") {
                let bin = op.args.get("bin").and_then(|v| v.as_str()).unwrap_or("");
                let what = if cx.op_kinds.contains(bin) && !cx.req.bin_allowed(bin) {
                    Some("a built-in op")
                } else if cx.cmds.contains(bin) {
                    Some("a command")
                } else if cx.tmpls.contains(bin) {
                    Some("a template")
                } else {
                    None
                };
                if let Some(what) = what {
                    return Err(format!(
                        "env prefix (NAME=value before the call) is only valid on a declared-bin call or `exec`; `{bin}` is {what} \
                         — use `with_env` or the command's `env` modifier instead"
                    ));
                }
            }
            if op.kind == "exec" && truthy_arg(op.args.get("implicit")) {
                let bin = op
                    .args
                    .get("bin")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if cx.op_kinds.contains(&bin) && !cx.req.bin_allowed(&bin) {
                    // Bare leading name is a built-in OP, not a declared bin:
                    // rewrite the captured exec back into a native op-capture.
                    // split_exec_argv already populated _0.._N (+ `_N_var` for
                    // bare-ident args) from the tail, so the op handler sees the
                    // same positional args it would from an explicit `op a "b"`
                    // call — and a bare-ident arg still resolves against bindings.
                    // A name that is BOTH an op and a declared bin stays exec —
                    // the explicit `bin "…"` declaration signals the subprocess.
                    let mut args = Map::new();
                    copy_argv_slots(&op.args, &mut args);
                    *op = Op {
                        kind: bin,
                        args,
                        capture_into: std::mem::take(&mut op.capture_into),
                        line: op.line,
                        ..Default::default()
                    };
                } else if cx.cmds.contains(&bin) {
                    demote_vars(&mut op.args);
                    let mut args = Map::new();
                    args.insert("target".into(), Value::String(bin));
                    copy_argv_slots(&op.args, &mut args);
                    *op = Op {
                        kind: "run".into(),
                        args,
                        capture_into: std::mem::take(&mut op.capture_into),
                        line: op.line,
                        ..Default::default()
                    };
                } else if cx.tmpls.contains(&bin) {
                    demote_vars(&mut op.args);
                    let mut args = Map::new();
                    args.insert("name".into(), Value::String(bin));
                    copy_argv_slots(&op.args, &mut args);
                    *op = Op {
                        kind: "_template_call".into(),
                        args,
                        line: op.line,
                        ..Default::default()
                    };
                } else {
                    // Stays a subprocess exec (a declared bin): argv tokens are
                    // literal, so a bare-ident token is its own text, never a
                    // binding lookup (`docker ps` → literal `ps`).
                    demote_vars(&mut op.args);
                }
            } else if op.kind != "exec"
                && op.body.is_empty()
                && (!op.capture_into.is_empty() || truthy_arg(op.args.get("implicit_ident")))
                && (op.args.get("_0_var").is_some_and(|v| !v.is_null())
                    || truthy_arg(op.args.get("implicit_ident")))
            {
                // A single-bare-ident bare-name op: either a capture
                // (`let x = NAME arg`, let_1arg_ident) or a flat statement
                // (`ensure_dir BUILD_DIR`, implicit_ident). The arg was emitted as
                // `_0_var` — a var-ref, the right default when NAME is a built-in
                // op (`upper who`, `ensure_dir BUILD_DIR`). The marker is internal
                // only; drop it now.
                op.args.remove("implicit_ident");
                let name = op.kind.clone();
                // Genuine built-in op: leave the var-ref `_0_var` in place for
                // InterpolateArgs to resolve. Nothing else to do.
                if !cx.op_kinds.contains(&name) {
                    // NAME is a bin/command/template, not an op: the bare ident is a
                    // LITERAL positional token, not a var-ref, so demote `_0_var` →
                    // `_0` before folding.
                    if let Some(Value::String(vn)) = op.args.get("_0_var").cloned() {
                        op.args.remove("_0_var");
                        op.args.insert("_0".into(), Value::String(vn));
                    }
                    let mut args = Map::new();
                    if cx.cmds.contains(&name) {
                        args.insert("target".into(), Value::String(name));
                        copy_argv_slots(&op.args, &mut args);
                        *op = Op {
                            kind: "run".into(),
                            args,
                            capture_into: std::mem::take(&mut op.capture_into),
                            line: op.line,
                            ..Default::default()
                        };
                    } else if cx.tmpls.contains(&name) {
                        args.insert("name".into(), Value::String(name));
                        copy_argv_slots(&op.args, &mut args);
                        *op = Op {
                            kind: "_template_call".into(),
                            args,
                            line: op.line,
                            ..Default::default()
                        };
                    } else {
                        args.insert("bin".into(), Value::String(name));
                        args.insert("implicit".into(), Value::Bool(true));
                        copy_argv_slots(&op.args, &mut args);
                        *op = Op {
                            kind: "exec".into(),
                            args,
                            capture_into: std::mem::take(&mut op.capture_into),
                            line: op.line,
                            ..Default::default()
                        };
                    }
                }
            }
            if !op.body.is_empty() {
                walk(&mut op.body, cx)?;
            }
        }
        Ok(())
    }

    let cx = Ctx { cmds: &cmds, tmpls: &tmpls, req: &req, op_kinds };
    for (name, c) in prog.commands.iter_mut() {
        walk(&mut c.ops, &cx).map_err(|e| format!("command {name}: {e}"))?;
    }
    if let Some(catch) = prog.catch.as_mut() {
        walk(&mut catch.ops, &cx).map_err(|e| format!("catch: {e}"))?;
    }
    for (name, t) in prog.templates.iter_mut() {
        walk(&mut t.ops, &cx).map_err(|e| format!("template {name}: {e}"))?;
    }
    Ok(())
}

/// Copies the positional argv slots from an exec op's args into a destination
/// map. Both literal `_N` and var-ref `_N_var` slots are preserved, so
/// converting an implicit-exec into a native op-capture keeps a bare-ident
/// argument resolving against the bindings (`join SRC DST`).
fn copy_argv_slots(src: &Map<String, Value>, dst: &mut Map<String, Value>) {
    let mut n = 0;
    loop {
        let lit = format!("_{n}");
        let var_k = format!("{lit}_var");
        let lv = src.get(&lit);
        let vv = src.get(&var_k);
        if lv.is_none() && vv.is_none() {
            break;
        }
        if let Some(lv) = lv {
            dst.insert(lit, lv.clone());
        }
        if let Some(vv) = vv {
            dst.insert(var_k, vv.clone());
        }
        n += 1;
    }
}

/// Rewrites every `_N_var` slot to a literal `_N` slot in place — the
/// bare-identifier token becomes its own literal text. Used when an
/// implicit-exec resolves to a subprocess bin / command / template, where a
/// positional arg is a literal token (`docker ps`, not the binding `ps`),
/// never a var-ref.
fn demote_vars(args: &mut Map<String, Value>) {
    let mut n = 0;
    loop {
        let lit = format!("_{n}");
        let var_k = format!("{lit}_var");
        let has_lit = args.contains_key(&lit);
        let has_var = args.contains_key(&var_k);
        if !has_lit && !has_var {
            break;
        }
        if let Some(vv) = args.remove(&var_k) {
            args.insert(lit, vv);
        }
        n += 1;
    }
}

fn truthy_arg(v: Option<&Value>) -> bool {
    matches!(v, Some(Value::Bool(true)))
}

/// Re-runs the template expansion pass across every command body, catch body,
/// and template body in `prog`. Idempotent — commands that already had every
/// call resolved by `parse_event_stream` are unchanged. Used after imports
/// merge to resolve `call` markers that referred to imported templates.
fn expand_all_templates(prog: &mut Program) -> Result<(), Error> {
    let names: Vec<String> = prog.commands.keys().cloned().collect();
    for name in names {
        let expanded = expand_template_ops(&prog.commands[&name].ops, &prog.templates, &HashSet::new())
            .map_err(|e| format!("expanding template in command {}: {e}", go_quote(&name)))?;
        prog.commands.get_mut(&name).unwrap().ops = expanded;
    }
    if let Some(catch) = &prog.catch {
        let expanded = expand_template_ops(&catch.ops, &prog.templates, &HashSet::new())
            .map_err(|e| format!("expanding template in catch: {e}"))?;
        prog.catch.as_mut().unwrap().ops = expanded;
    }
    let tnames: Vec<String> = prog.templates.keys().cloned().collect();
    for name in tnames {
        let expanding = HashSet::from([name.clone()]);
        let expanded = expand_template_ops(&prog.templates[&name].ops, &prog.templates, &expanding)
            .map_err(|e| format!("expanding template in template {}: {e}", go_quote(&name)))?;
        prog.templates.get_mut(&name).unwrap().ops = expanded;
    }
    Ok(())
}

/// Substitutes `${name}` placeholders in an import path before filesystem
/// resolution. Recognised names:
///
/// ```text
/// ${file_dir} / ${script_dir} — directory of the importing file
/// ${home} / ${HOME}           — user's home directory
/// ${cache_dir}                — OS user cache dir
/// ${config_dir}               — OS user config dir
/// ${temp_dir}                 — OS temp dir
/// ${exe_dir}                  — directory of the running perch binary
/// ${user} / ${USER}           — current username
/// ${ANY_OTHER}                — falls through to the process environment
/// ```
///
/// Unknown names fail with a clear error rather than expanding to empty
/// (which would silently produce a wrong path like `/shared/aws.perch`
/// instead of the intended `${HOME}/shared/aws.perch`).
///
/// This is the same set perch auto-binds at runtime — kept aligned so users
/// don't have to learn a separate vocabulary for import-time vs command-time
/// interpolation.
fn expand_import_path(raw: &str, file_dir: &str) -> Result<String, String> {
    expand_template(raw, |name| {
        match name {
            "file_dir" | "script_dir" => {
                if file_dir.is_empty() {
                    if let Ok(cwd) = std::env::current_dir() {
                        return Some(cwd.to_string_lossy().into_owned());
                    }
                }
                return Some(file_dir.to_string());
            }
            "home" | "HOME" => {
                if let Some(h) = user_home_dir() {
                    return Some(h);
                }
            }
            "cache_dir" => {
                if let Some(d) = user_cache_dir() {
                    return Some(d);
                }
            }
            "config_dir" => {
                if let Some(d) = user_config_dir() {
                    return Some(d);
                }
            }
            "temp_dir" => return Some(std::env::temp_dir().to_string_lossy().into_owned()),
            "exe_dir" => {
                if let Ok(mut exe) = std::env::current_exe() {
                    if let Ok(resolved) = std::fs::canonicalize(&exe) {
                        exe = resolved;
                    }
                    return Some(go_dir(&exe.to_string_lossy()));
                }
            }
            "user" | "USER" => {
                if let Some(u) = current_username() {
                    return Some(u);
                }
            }
            _ => {}
        }
        // Fall through to host env. ${ANYTHING_ELSE} reads the live env.
        std::env::var(name).ok()
    })
}

fn env_nonempty(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

fn user_home_dir() -> Option<String> {
    env_nonempty(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
}

fn user_cache_dir() -> Option<String> {
    if cfg!(windows) {
        return env_nonempty("LocalAppData");
    }
    if cfg!(target_os = "macos") {
        return user_home_dir().map(|h| format!("{h}/Library/Caches"));
    }
    env_nonempty("XDG_CACHE_HOME").or_else(|| user_home_dir().map(|h| format!("{h}/.cache")))
}

fn user_config_dir() -> Option<String> {
    if cfg!(windows) {
        return env_nonempty("AppData");
    }
    if cfg!(target_os = "macos") {
        return user_home_dir().map(|h| format!("{h}/Library/Application Support"));
    }
    env_nonempty("XDG_CONFIG_HOME").or_else(|| user_home_dir().map(|h| format!("{h}/.config")))
}

fn current_username() -> Option<String> {
    ["USER", "LOGNAME", "USERNAME"].iter().find_map(|k| env_nonempty(k))
}

/// A tiny `${name}` scanner. It mirrors the interpreter's interpolation rules
/// at the surface (same syntax) but lives in this crate to avoid a
/// loader→interpreter dependency, and uses a resolver callback so the caller
/// decides what each name means.
fn expand_template(s: &str, resolve: impl Fn(&str) -> Option<String>) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if i + 1 < b.len() && b[i] == b'$' && b[i + 1] == b'{' {
            let Some(end) = s[i + 2..].find('}') else {
                return Err(format!("unterminated ${{ in {}", go_quote(s)));
            };
            let name = s[i + 2..i + 2 + end].trim();
            match resolve(name) {
                Some(v) => out.push_str(&v),
                None => return Err(format!("unknown placeholder ${{{name}}} in import path")),
            }
            i += 2 + end + 1;
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    Ok(out)
}

/// Folds `from`'s commands into `into`. Flat imports merge commands by their
/// bare names; aliased imports prefix each with `ALIAS.`. Conflicts surface as
/// errors so duplicate-name bugs don't silently win-or-lose. Globals merge
/// silently with parent-wins precedence (the importer can override a default
/// from a shared file).
fn merge_program(into: &mut Program, from: Program, alias: &str) -> Result<(), String> {
    for (name, cmd) in from.commands {
        // Skip imported private commands on flat import — they're the imported
        // file's internal helpers and shouldn't pollute the caller's namespace.
        // Aliased import keeps them callable as `ALIAS.privateName` for
        // advanced composition.
        if cmd.modifiers.private && alias.is_empty() {
            continue;
        }
        let target_name = if alias.is_empty() { name } else { format!("{alias}.{name}") };
        if into.commands.contains_key(&target_name) {
            return Err(format!(
                "command {} already declared (conflict from import)",
                go_quote(&target_name)
            ));
        }
        into.commands.insert(target_name, cmd);
    }
    // Globals: parent wins. If the importer declared NAME, we keep its value;
    // otherwise we adopt the imported value. This gives the caller a "default
    // with override" semantic without surfacing a conflict.
    let existing: HashSet<String> = into.globals.bindings.iter().map(|g| g.name.clone()).collect();
    for g in from.globals.bindings {
        if !existing.contains(&g.name) {
            into.globals.bindings.push(g);
        }
    }
    // Templates from imports are merged into the parent's template map flat (no
    // alias prefix — templates are inlined at parse time, so there's no
    // namespace at runtime to worry about). Importer-defined templates win on
    // conflict because the importer is the "more specific" definition; an
    // imported template can't shadow one the caller already wrote.
    for (name, tpl) in from.templates {
        into.templates.entry(name).or_insert(tpl);
    }
    // Requirements union. Under zero-ambient-authority the spawnable-bin set is
    // the manifest, so an imported file's declared bins/hosts/env/fs must count
    // toward the merged program's allowlist — otherwise a command imported from
    // a file that legitimately declared `bin "docker"` would fail
    // bin_not_declared at load. We union (dedup) rather than parent-wins:
    // capabilities are additive across the import graph. Declared is OR'd so a
    // root that imports a manifest-bearing partial counts as declared.
    let fr = from.requirements;
    if fr.declared {
        into.requirements.declared = true;
    }
    let ir = &mut into.requirements;
    union_by(&mut ir.bins, fr.bins, |x: &BinReq| x.name.clone());
    union_by(&mut ir.envs, fr.envs, |x: &EnvReq| x.name.clone());
    union_by(&mut ir.hosts, fr.hosts, |x: &HostReq| x.name.clone());
    union_by(&mut ir.read_roots, fr.read_roots, |x: &String| x.clone());
    union_by(&mut ir.write_roots, fr.write_roots, |x: &String| x.clone());
    union_by(&mut ir.os, fr.os, |x: &String| x.clone());
    union_by(&mut ir.arch, fr.arch, |x: &String| x.clone());

    // Catch handlers don't propagate via import — only the root file's catch
    // (if any) is active. Imported catches would race with the importer's, and
    // there's no clear right answer for what wins.
    Ok(())
}

/// Appends the items of `b` whose key isn't already in `a` (dedup by key).
fn union_by<T>(a: &mut Vec<T>, b: Vec<T>, key: impl Fn(&T) -> String) {
    let mut seen: HashSet<String> = a.iter().map(&key).collect();
    for x in b {
        if seen.insert(key(&x)) {
            a.push(x);
        }
    }
}

/// One line of NDJSON emitted by lib.capy.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Event {
    event: String,
    name: String,
    kind: String,
    value: Value,
    args: Option<Map<String, Value>>,
    capture_into: String,
    /// `import` event.
    path: String,
    /// `import` event.
    alias: String,
    /// `requires_bin` events.
    optional: bool,
    /// `hook` events.
    timing: String,
    target: String,
    handler: String,
}

/// One `import "PATH" [as ALIAS]` statement from a .perch file.
/// Loader-internal; not exposed on `Program`.
#[derive(Debug, Clone)]
pub(crate) struct ImportDirective {
    /// Raw path as written.
    pub path: String,
    /// Empty for flat, set for namespaced.
    pub alias: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ParserState {
    Top,
    Command,
    CommandArg,
    CommandDo,
    Catch,
    CatchArg,
    CatchDo,
    Template,
    TemplateArg,
    TemplateDo,
    Bundle,
    Requires,
    Hooks,
}

/// Which op list a `do` block is currently filling.
enum Root {
    Cmd(String),
    Catch,
    Tpl(String),
}

/// The destination of the next op: the root list (a command's / catch's /
/// template's `ops`) plus a path of indices into nested `body` lists. Push on
/// `_enter`, pop on `_leave`.
struct OpStack {
    root: Root,
    path: Vec<usize>,
}

impl OpStack {
    fn dest<'a>(&self, prog: &'a mut Program) -> &'a mut Vec<Op> {
        let mut v: &mut Vec<Op> = match &self.root {
            Root::Cmd(n) => &mut prog.commands.get_mut(n).unwrap().ops,
            Root::Catch => &mut prog.catch.as_mut().unwrap().ops,
            Root::Tpl(n) => &mut prog.templates.get_mut(n).unwrap().ops,
        };
        for &i in &self.path {
            v = &mut v[i].body;
        }
        v
    }
}

fn parse_event_stream(stream: &str) -> Result<(Program, Vec<ImportDirective>), String> {
    let mut prog = Program::default();
    let mut imports: Vec<ImportDirective> = Vec::new();

    let mut state = ParserState::Top;
    let mut cur_cmd: Option<String> = None;
    let mut cur_catch = false;
    let mut cur_tpl: Option<String> = None;
    let mut cur_arg: Option<ArgSpec> = None;
    let mut op_stack: Option<OpStack> = None;
    // Index into the `do` body's ops where the command-level `finally` section
    // starts (set by the `do_finally` divider event).
    let mut do_finally_at: Option<usize> = None;

    let mut line_num = 0;
    for raw in stream.split('\n') {
        let line = raw.trim();
        line_num += 1;
        if line.is_empty() {
            continue;
        }
        let ev: Event = serde_json::from_str(line)
            .map_err(|e| format!("line {line_num}: malformed event {}: {e}", go_quote(line)))?;

        match ev.event.as_str() {
            "name" => prog.name = as_string(&ev.value),
            "about" => prog.description = as_string(&ev.value),
            "version" => prog.version = as_string(&ev.value),
            "import" => {
                // Imports are collected at parse time and resolved by the
                // caller (load / resolve_imports). This keeps the parser
                // pure — no IO. Only valid at the top level.
                if state != ParserState::Top {
                    return Err(format!("line {line_num}: import must be at top level"));
                }
                imports.push(ImportDirective { path: ev.path, alias: ev.alias });
            }

            "globals_removed" => {
                return Err(format!(
                    "line {line_num}: the `globals ... end` block was removed — \
                     declare shared bindings bare at top level instead, e.g. `BUILD_DIR = \"{}/.out\"`",
                    "${script_dir}"
                ));
            }
            "let_removed" => {
                return Err(format!(
                    "line {line_num}: the `let` keyword was removed — write `{} = …` \
                     instead (`=` is the assignment operator: a binding at file scope, a capture inside `do`)",
                    ev.name
                ));
            }
            "bundle_begin" => {
                if state != ParserState::Top {
                    return Err(format!("line {line_num}: bundle block must be at top level"));
                }
                state = ParserState::Bundle;
            }
            "bundle_end" => state = ParserState::Top,
            "hooks_begin" => {
                if state != ParserState::Top {
                    return Err(format!("line {line_num}: hooks block must be at top level"));
                }
                state = ParserState::Hooks;
            }
            "hooks_end" => state = ParserState::Top,
            "hook" => {
                if state != ParserState::Hooks {
                    return Err(format!(
                        "line {line_num}: hook line outside a `hooks ... end` block"
                    ));
                }
                prog.hooks.push(Hook { timing: ev.timing, target: ev.target, handler: ev.handler });
            }
            "bundle_include" => {
                if state != ParserState::Bundle {
                    return Err(format!("line {line_num}: 'include' event outside bundle block"));
                }
                let path = as_string(&ev.value);
                prog.bundle.includes.push(path.clone());
                if !ev.alias.is_empty() {
                    // Default entry: basename of the source path. This is what
                    // `tarballPaths` writes for a file include. For directory
                    // includes the alias points at the dir root; users wanting
                    // to address a specific file inside should alias the file
                    // path directly, e.g. `include "./modules/policy.wasm" as policy_wasm`.
                    let entry = go_base(&path);
                    prog.bundle.aliases.push(BundleAlias { name: ev.alias, entry });
                }
            }
            "requires_begin" => {
                if state != ParserState::Top {
                    return Err(format!("line {line_num}: requires block must be at top level"));
                }
                state = ParserState::Requires;
                prog.requirements.declared = true;
            }
            "requires_end" => state = ParserState::Top,
            "requires_bin" => {
                if state != ParserState::Requires {
                    return Err(format!("line {line_num}: 'bin' outside requires block"));
                }
                prog.requirements.bins.push(BinReq {
                    name: ev.name,
                    alias: ev.alias,
                    optional: ev.optional,
                    ..Default::default()
                });
            }
            "requires_bin_field" => {
                // Mutates the most-recently-appended BinReq (hash / hash_file).
                let n = prog.requirements.bins.len();
                if n == 0 || state != ParserState::Requires {
                    return Err(format!(
                        "line {line_num}: '{}' outside a `bin ... end` block",
                        ev.kind
                    ));
                }
                match ev.kind.as_str() {
                    "hash" => prog.requirements.bins[n - 1].hash = as_string(&ev.value),
                    "hash_file" => prog.requirements.bins[n - 1].hash_file = as_string(&ev.value),
                    _ => {
                        return Err(format!(
                            "line {line_num}: unknown bin field {}",
                            go_quote(&ev.kind)
                        ))
                    }
                }
            }
            "requires_env" => {
                if state != ParserState::Requires {
                    return Err(format!("line {line_num}: 'env' outside requires block"));
                }
                prog.requirements.envs.push(EnvReq { name: ev.name, optional: ev.optional });
            }
            "requires_host" => {
                if state != ParserState::Requires {
                    return Err(format!("line {line_num}: 'host' outside requires block"));
                }
                prog.requirements.hosts.push(HostReq { name: ev.name, optional: ev.optional });
            }
            "requires_read" => {
                if state != ParserState::Requires {
                    return Err(format!("line {line_num}: 'read' outside requires block"));
                }
                prog.requirements.read_roots.push(as_string(&ev.value));
            }
            "requires_write" => {
                if state != ParserState::Requires {
                    return Err(format!("line {line_num}: 'write' outside requires block"));
                }
                prog.requirements.write_roots.push(as_string(&ev.value));
            }
            "requires_read_var" => {
                // Bare-name form `read SRC` → wrap the binding name back into a
                // "${SRC}" root so it interpolates like the string form.
                if state != ParserState::Requires {
                    return Err(format!("line {line_num}: 'read' outside requires block"));
                }
                prog.requirements.read_roots.push(format!("${{{}}}", ev.name));
            }
            "requires_write_var" => {
                if state != ParserState::Requires {
                    return Err(format!("line {line_num}: 'write' outside requires block"));
                }
                prog.requirements.write_roots.push(format!("${{{}}}", ev.name));
            }
            "requires_os" => {
                if state != ParserState::Requires {
                    return Err(format!("line {line_num}: 'os' outside requires block"));
                }
                prog.requirements.os.push(ev.name);
            }
            "requires_arch" => {
                if state != ParserState::Requires {
                    return Err(format!("line {line_num}: 'arch' outside requires block"));
                }
                prog.requirements.arch.push(ev.name);
            }

            "global" => {
                // `NAME = VALUE`. The `=` operator is universal (no `let` keyword):
                //   • at FILE scope it declares a shared binding;
                //   • inside a `do` block (single-token RHS) it's a CAPTURE — the
                //     value resolves as a bare op / bin / command via the normal
                //     capture pipeline (resolve_bare_dispatch), with its output
                //     bound to NAME. Multi-token captures (`x = docker ps -q`)
                //     come in as `op` events from the assign_* grammar forms instead.
                if let Some(stack) = &op_stack {
                    let bin = ev.value.as_str().unwrap_or("").to_string();
                    let mut args = Map::new();
                    args.insert("bin".into(), Value::String(bin));
                    args.insert("implicit".into(), Value::Bool(true));
                    let op = Op {
                        kind: "exec".into(),
                        args,
                        capture_into: ev.name,
                        line: line_num,
                        ..Default::default()
                    };
                    stack.dest(&mut prog).push(op);
                    continue;
                }
                if state != ParserState::Top {
                    return Err(format!(
                        "line {line_num}: assignment `{} = ...` not allowed here",
                        ev.name
                    ));
                }
                prog.globals.bindings.push(GlobalBinding {
                    name: ev.name,
                    ty: infer_literal_type(&ev.value).to_string(),
                    value: ev.value,
                });
            }

            "command_begin" => {
                if state != ParserState::Top {
                    return Err(format!("line {line_num}: nested command not allowed"));
                }
                prog.commands.insert(
                    ev.name.clone(),
                    Command { name: ev.name.clone(), ..Default::default() },
                );
                cur_cmd = Some(ev.name);
                state = ParserState::Command;
                op_stack = None;
            }

            "command_end" => {
                if state != ParserState::Command && state != ParserState::CommandDo {
                    return Err(format!("line {line_num}: command_end while not in command"));
                }
                cur_cmd = None;
                state = ParserState::Top;
                op_stack = None;
            }

            "catch_begin" => {
                if state != ParserState::Top {
                    return Err(format!("line {line_num}: nested catch not allowed"));
                }
                prog.catch = Some(perch_domain::Catch { bind: ev.name, ..Default::default() });
                cur_catch = true;
                state = ParserState::Catch;
                op_stack = None;
            }

            "catch_end" => {
                cur_catch = false;
                state = ParserState::Top;
                op_stack = None;
            }

            "template_begin" => {
                if state != ParserState::Top {
                    return Err(format!("line {line_num}: template must be at top level"));
                }
                if prog.templates.contains_key(&ev.name) {
                    return Err(format!(
                        "line {line_num}: template {} redeclared",
                        go_quote(&ev.name)
                    ));
                }
                prog.templates
                    .insert(ev.name.clone(), Template { name: ev.name.clone(), ..Default::default() });
                cur_tpl = Some(ev.name);
                state = ParserState::Template;
                op_stack = None;
            }

            "template_end" => {
                if state != ParserState::Template && state != ParserState::TemplateDo {
                    return Err(format!("line {line_num}: template_end while not in template"));
                }
                cur_tpl = None;
                state = ParserState::Top;
                op_stack = None;
            }

            "do_begin" => {
                let root = match state {
                    ParserState::Command => {
                        state = ParserState::CommandDo;
                        Root::Cmd(cur_cmd.clone().unwrap())
                    }
                    ParserState::Catch => {
                        state = ParserState::CatchDo;
                        Root::Catch
                    }
                    ParserState::Template => {
                        state = ParserState::TemplateDo;
                        Root::Tpl(cur_tpl.clone().unwrap())
                    }
                    _ => {
                        return Err(format!(
                            "line {line_num}: 'do' outside command/catch/template"
                        ))
                    }
                };
                op_stack = Some(OpStack { root, path: Vec::new() });
            }

            "do_finally" => {
                // Section divider between the `do` body and its `finally`
                // section. Only ever emitted by the `do` function, so the op
                // stack is live; nested `try … finally` dividers are separate
                // marker ops, never this event.
                let Some(stack) = op_stack.as_ref() else {
                    return Err(format!("line {line_num}: `finally` outside a do block"));
                };
                do_finally_at = Some(stack.dest(&mut prog).len());
            }

            "do_end" => {
                // R02a: a non-empty command-level `finally` wraps the whole
                // body in the same marker stream as `try … finally … end`
                // (one `try` op: body, `_catch` with no rescue arm, `_finally`,
                // cleanup), so opTry semantics stay in one place.
                if let (Some(at), Some(stack)) = (do_finally_at.take(), op_stack.as_ref()) {
                    let ops = stack.dest(&mut prog);
                    if ops.len() > at {
                        let fin = ops.split_off(at);
                        let line = ops.first().or(fin.first()).map(|o| o.line).unwrap_or(0);
                        let mut body = std::mem::take(ops);
                        let mut catch_args = Map::new();
                        catch_args.insert("bind".into(), Value::String("err".into()));
                        body.push(Op { kind: "_catch".into(), args: catch_args, ..Default::default() });
                        body.push(Op { kind: "_finally".into(), ..Default::default() });
                        body.extend(fin);
                        ops.push(Op { kind: "try".into(), body, line, ..Default::default() });
                    }
                }
                match state {
                    ParserState::CommandDo => state = ParserState::Command,
                    ParserState::CatchDo => state = ParserState::Catch,
                    ParserState::TemplateDo => state = ParserState::Template,
                    _ => {
                        return Err(format!(
                            "line {line_num}: 'do_end' without matching do_begin"
                        ))
                    }
                }
                op_stack = None;
            }

            "arg_begin" => {
                state = match state {
                    ParserState::Command => ParserState::CommandArg,
                    ParserState::Catch => ParserState::CatchArg,
                    ParserState::Template => ParserState::TemplateArg,
                    _ => {
                        return Err(format!(
                            "line {line_num}: 'arg' outside command/catch/template config region"
                        ))
                    }
                };
                cur_arg = Some(ArgSpec { name: ev.name, ..Default::default() });
            }

            "arg_end" => {
                let Some(arg) = cur_arg.take() else {
                    return Err(format!("line {line_num}: 'arg_end' without matching 'arg'"));
                };
                if arg.ty.is_empty() {
                    return Err(format!(
                        "line {line_num}: arg {} has no `type` field",
                        go_quote(&arg.name)
                    ));
                }
                match state {
                    ParserState::CommandArg => {
                        prog.commands.get_mut(cur_cmd.as_ref().unwrap()).unwrap().args.push(arg);
                        state = ParserState::Command;
                    }
                    // Catch doesn't currently track its own arg list; ignore.
                    ParserState::CatchArg => state = ParserState::Catch,
                    ParserState::TemplateArg => {
                        prog.templates.get_mut(cur_tpl.as_ref().unwrap()).unwrap().args.push(arg);
                        state = ParserState::Template;
                    }
                    _ => {}
                }
            }

            "arg_field" => {
                let Some(arg) = cur_arg.as_mut() else {
                    return Err(format!(
                        "line {line_num}: '{}' field outside an `arg` block",
                        ev.kind
                    ));
                };
                match ev.kind.as_str() {
                    "type" => arg.ty = as_string(&ev.value),
                    "default" => {
                        arg.default = ev.value;
                        arg.has_default = true;
                    }
                    "optional" => arg.optional = true,
                    "rest" => arg.rest = true,
                    "index" => arg.index = Some(as_floatish(&ev.value) as i64),
                    _ => {
                        return Err(format!(
                            "line {line_num}: unknown arg field {}",
                            go_quote(&ev.kind)
                        ))
                    }
                }
            }

            "config" => {
                // `description` inside an arg block sets the arg's description.
                if let Some(arg) = cur_arg.as_mut() {
                    if ev.kind == "description" {
                        arg.description = as_string(&ev.value);
                        continue;
                    }
                }
                // Templates support `description` as their only config statement.
                // Anything else (private, env, on_signal …) is meaningless on a
                // parse-time stamp and is rejected here so it surfaces early.
                if let Some(tn) = &cur_tpl {
                    if ev.kind != "description" {
                        return Err(format!(
                            "line {line_num}: config {} not allowed inside a template",
                            go_quote(&ev.kind)
                        ));
                    }
                    prog.templates.get_mut(tn).unwrap().description = as_string(&ev.value);
                    continue;
                }
                let cmd = cur_cmd.as_ref().and_then(|n| prog.commands.get_mut(n));
                let catch = if cur_catch { prog.catch.as_mut() } else { None };
                apply_config(cmd, catch, &ev).map_err(|e| format!("line {line_num}: {e}"))?;
            }

            "op" => {
                let Some(stack) = op_stack.as_mut() else {
                    return Err(format!(
                        "line {line_num}: op '{}' outside a do block",
                        ev.kind
                    ));
                };
                // `_template_call` is a placeholder the expansion pass replaces
                // with the template's body, after positional args are bound.
                // Templates can be defined later in the file than they're
                // called, so we collect markers here and resolve after the full
                // stream is parsed.
                if ev.kind == "_template_call" {
                    let op = Op {
                        kind: "_template_call".into(),
                        args: ev.args.unwrap_or_default(),
                        line: line_num,
                        ..Default::default()
                    };
                    stack.dest(&mut prog).push(op);
                } else if ev.kind == "_enter" {
                    // Push a new nested op whose body becomes the active target.
                    // capture_into lets a block op's value be bound (e.g.
                    // `let out = pipe ... end` captures the last stage's stdout).
                    let new_op = Op {
                        kind: ev.name,
                        args: ev.args.unwrap_or_default(),
                        capture_into: ev.capture_into,
                        ..Default::default()
                    };
                    let dest = stack.dest(&mut prog);
                    dest.push(new_op);
                    let idx = dest.len() - 1;
                    stack.path.push(idx);
                } else if ev.kind == "_leave" {
                    if stack.path.is_empty() {
                        return Err(format!("line {line_num}: _leave without matching _enter"));
                    }
                    stack.path.pop();
                } else {
                    let op = Op {
                        kind: ev.kind,
                        args: ev.args.unwrap_or_default(),
                        capture_into: ev.capture_into,
                        ..Default::default()
                    };
                    stack.dest(&mut prog).push(op);
                }
            }

            other => return Err(format!("line {line_num}: unknown event {}", go_quote(other))),
        }
    }

    // Expand `_template_call` markers inline. Templates are pure parse-time
    // stamps: every call is replaced with the template's body, with positional
    // args bound as ${argname} substitutions in string args. Done AFTER the
    // stream is parsed so templates can be defined later in the file than
    // they're called. Recursion is rejected (visited set); declaration-emitting
    // templates were already prevented at parse time (templates can't contain
    // command_begin or import).
    if !prog.templates.is_empty() {
        let names: Vec<String> = prog.commands.keys().cloned().collect();
        for name in names {
            let expanded =
                expand_template_ops(&prog.commands[&name].ops, &prog.templates, &HashSet::new())
                    .map_err(|e| format!("expanding template in command {}: {e}", go_quote(&name)))?;
            prog.commands.get_mut(&name).unwrap().ops = expanded;
        }
        if let Some(catch) = &prog.catch {
            let expanded = expand_template_ops(&catch.ops, &prog.templates, &HashSet::new())
                .map_err(|e| format!("expanding template in catch: {e}"))?;
            prog.catch.as_mut().unwrap().ops = expanded;
        }
        // Templates may also reference each other. Expand their own bodies so a
        // later-spliced call already has nested calls resolved.
        let tnames: Vec<String> = prog.templates.keys().cloned().collect();
        for name in tnames {
            let expanding = HashSet::from([name.clone()]);
            let expanded =
                expand_template_ops(&prog.templates[&name].ops, &prog.templates, &expanding)
                    .map_err(|e| {
                        format!("expanding template in template {}: {e}", go_quote(&name))
                    })?;
            prog.templates.get_mut(&name).unwrap().ops = expanded;
        }
    }

    // Split `exec`/pipe-stage argv: the grammar captures all argv tokens as a
    // single `tail` string (`argv_raw`); we shell-split it into positional
    // slots _0.._N HERE, at load time, on the LITERAL source (before any
    // ${…} interpolation). That preserves the §3.3 keystone — a `${msg}`
    // token stays one slot even if its runtime value contains spaces — while
    // letting the grammar be one `exec BIN tail` function instead of an
    // arity-capped overload ladder. (capy >= ac128fb: quote-preserving tail.)
    split_exec_argv_all(&mut prog)?;

    Ok((prog, imports))
}

/// Walks every command/catch/template body (recursing into block bodies) and
/// expands any `exec` op's argv_raw into _0.._N slots.
fn split_exec_argv_all(prog: &mut Program) -> Result<(), String> {
    for (name, cmd) in prog.commands.iter_mut() {
        split_exec_argv(&mut cmd.ops).map_err(|e| format!("command {name}: {e}"))?;
    }
    if let Some(catch) = prog.catch.as_mut() {
        split_exec_argv(&mut catch.ops).map_err(|e| format!("catch: {e}"))?;
    }
    for (name, tpl) in prog.templates.iter_mut() {
        split_exec_argv(&mut tpl.ops).map_err(|e| format!("template {name}: {e}"))?;
    }
    Ok(())
}

fn split_exec_argv(ops: &mut [Op]) -> Result<(), String> {
    for op in ops.iter_mut() {
        if op.kind == "exec" && !op.args.contains_key("argv_raw") {
            // `exec K=v` with nothing after the assignments: nothing to run.
            let bin = op.args.get("bin").and_then(|v| v.as_str()).unwrap_or("").to_string();
            peel_env_prefix(&[bin])?;
        }
        if op.kind == "exec" {
            if let Some(Value::String(raw)) = op.args.get("argv_raw").cloned() {
                op.args.remove("argv_raw");
                let classified = shell_split_args_classified(&raw);
                let tokens: Vec<String> = classified.iter().map(|t| t.text.clone()).collect();
                let bin = op.args.get("bin").and_then(|v| v.as_str()).unwrap_or("").to_string();
                // Full token stream for this exec line: bin + argv.
                let mut full = vec![bin];
                full.extend(tokens);
                let (clauses, ops_between) = split_chain(&full);
                if ops_between.is_empty() {
                    // R05: peel leading `NAME=VALUE` tokens off the (single) clause
                    // into the `env_prefix` arg; the first non-assignment token is
                    // the real binary. Tokens AFTER it are never touched.
                    let (prefix, k) = peel_env_prefix(&full)?;
                    if k > 0 {
                        op.args.insert("bin".into(), Value::String(full[k].clone()));
                        op.args.insert("env_prefix".into(), prefix);
                        // `full[i]` is `classified[i - 1]`; argv is `full[k + 1..]`.
                        let implicit = truthy_arg(op.args.get("implicit"));
                        for (n, t) in classified[k..].iter().enumerate() {
                            let key = if implicit && t.bare { format!("_{n}_var") } else { format!("_{n}") };
                            op.args.insert(key, Value::String(t.text.clone()));
                        }
                        continue;
                    }
                }
                if !ops_between.is_empty() {
                    // `exec a && exec b ; exec c` — fold into an exec_chain
                    // block op whose body holds one child exec per clause.
                    // The operators (literal source tokens, never from ${…})
                    // drive short-circuit evaluation at runtime (§3.3).
                    let mut args = Map::new();
                    args.insert(
                        "ops".into(),
                        Value::Array(ops_between.into_iter().map(Value::String).collect()),
                    );
                    *op = Op {
                        kind: "exec_chain".into(),
                        args,
                        capture_into: std::mem::take(&mut op.capture_into),
                        line: op.line,
                        ..Default::default()
                    };
                    for clause in &clauses {
                        op.body.push(make_exec_op(clause)?);
                    }
                } else {
                    // Only IMPLICIT execs (a bare leading name, possibly an op) get
                    // per-token classification: a bare-identifier token becomes a
                    // `_N_var` var-ref (resolves to a binding of that name, else the
                    // literal token), a quoted / `${…}` / flag token a literal `_N`.
                    // resolve_bare_dispatch then keeps `_N_var` for a built-in op and
                    // demotes it to literal for a subprocess bin. An EXPLICIT `exec`
                    // is always a subprocess: its argv is literal, so no var-refs.
                    let implicit = truthy_arg(op.args.get("implicit"));
                    for (n, t) in classified.iter().enumerate() {
                        let key = if implicit && t.bare { format!("_{n}_var") } else { format!("_{n}") };
                        op.args.insert(key, Value::String(t.text.clone()));
                    }
                }
            }
        }
        if !op.body.is_empty() {
            split_exec_argv(&mut op.body)?;
        }
    }
    Ok(())
}

/// Partitions a flat exec token stream on top-level `&&` / `||` / `;` operator
/// tokens. Returns the per-clause token slices and the operators between them.
/// A clause after an operator begins with the user's repeated `exec` keyword,
/// which is stripped. Returns no operators when the line is a single command
/// (the common case).
fn split_chain(tokens: &[String]) -> (Vec<Vec<String>>, Vec<String>) {
    let mut clauses: Vec<Vec<String>> = Vec::new();
    let mut ops: Vec<String> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    let flush = |clauses: &mut Vec<Vec<String>>, cur: &mut Vec<String>| {
        // Strip a leading "exec" keyword on chained clauses (the 1st clause's
        // bin came from the grammar, so it has no leading "exec").
        if !clauses.is_empty() && !cur.is_empty() && cur[0] == "exec" {
            cur.remove(0);
        }
        clauses.push(std::mem::take(cur));
    };
    for t in tokens {
        if t == "&&" || t == "||" || t == ";" {
            flush(&mut clauses, &mut cur);
            ops.push(t.clone());
            continue;
        }
        cur.push(t.clone());
    }
    flush(&mut clauses, &mut cur);
    (clauses, ops)
}

/// Builds a single exec Op from a clause's tokens (bin + argv).
fn make_exec_op(clause: &[String]) -> Result<Op, String> {
    let mut args = Map::new();
    if !clause.is_empty() {
        // R05: each chained clause may carry its own `NAME=VALUE` prefix.
        let (prefix, k) = peel_env_prefix(clause)?;
        if k > 0 {
            args.insert("env_prefix".into(), prefix);
        }
        args.insert("bin".into(), Value::String(clause[k].clone()));
        for (n, tok) in clause[k + 1..].iter().enumerate() {
            args.insert(format!("_{n}"), Value::String(tok.clone()));
        }
    }
    Ok(Op { kind: "exec".into(), args, ..Default::default() })
}

// ── inline env prefix (R05) ──────────────────────────────────────────────────

/// `NAME=VALUE` with `NAME` = `[A-Za-z_][A-Za-z0-9_]*`. Returns (name, value).
fn parse_env_assignment(tok: &str) -> Option<(&str, &str)> {
    let (name, value) = tok.split_once('=')?;
    if is_bare_ident(name.as_bytes()) {
        Some((name, value))
    } else {
        None
    }
}

/// A token shaped like an assignment whose NAME is not a valid identifier
/// (`1FOO=x`, `FOO-BAR=x`, `=x`). Rejected rather than silently read as a
/// binary name; a path or flag (`./x=y`, `--k=v`) is not flagged.
fn is_malformed_env_assignment(tok: &str) -> bool {
    let Some((name, _)) = tok.split_once('=') else { return false };
    if is_bare_ident(name.as_bytes()) {
        return false;
    }
    name.is_empty()
        || (name.as_bytes()[0].is_ascii_alphanumeric() || name.as_bytes()[0] == b'_')
            && name.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
}

/// Rewrites `$NAME` to `${NAME}` (so the op-arg resolver's one syntax applies);
/// `${NAME}` and everything else pass through untouched.
fn normalize_dollar_refs(v: &str) -> String {
    let b = v.as_bytes();
    let mut out = String::with_capacity(v.len() + 2);
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'$' && i + 1 < b.len() && (b[i + 1] == b'_' || b[i + 1].is_ascii_alphabetic()) {
            let mut j = i + 1;
            while j < b.len() && (b[j] == b'_' || b[j].is_ascii_alphanumeric()) {
                j += 1;
            }
            out.push_str("${");
            out.push_str(&v[i + 1..j]);
            out.push('}');
            i = j;
            continue;
        }
        let ch = v[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Splits the leading `NAME=VALUE` tokens off an exec token run (bin first).
/// Returns the `env_prefix` object (name to string, source order) and the number
/// of tokens consumed. Errors on a malformed `NAME=` token or when nothing but
/// assignments is left (no command to apply them to).
fn peel_env_prefix(tokens: &[String]) -> Result<(Value, usize), String> {
    let mut prefix = Map::new();
    let mut k = 0;
    while k < tokens.len() {
        let tok = &tokens[k];
        if let Some((name, value)) = parse_env_assignment(tok) {
            prefix.insert(name.to_string(), Value::String(normalize_dollar_refs(value)));
            k += 1;
        } else if is_malformed_env_assignment(tok) {
            return Err(format!(
                "malformed env assignment {}: the name before `=` must match [A-Za-z_][A-Za-z0-9_]*",
                go_quote(tok)
            ));
        } else {
            break;
        }
    }
    if k > 0 && k == tokens.len() {
        return Err(format!(
            "env prefix {} has no command to apply to (write `NAME=value BIN verb …`)",
            go_quote(&tokens[k - 1])
        ));
    }
    Ok((Value::Object(prefix), k))
}

/// Source pre-pass for R05. capy reads `K=v tool args` at statement start as a
/// capture (`K = v tool args`) — the same tokens as the spaced form — so the
/// adjacency that marks a prefix would be lost. This pass finds statements
/// whose FIRST word is a no-space `NAME=VALUE` (value may be quoted) followed
/// by more tokens, and inserts the `exec` keyword, which routes them to the
/// explicit exec grammar with the assignment as `bin`; [`peel_env_prefix`] then
/// turns the leading assignment tokens into `env_prefix`. It mirrors capy's
/// line model: backtick strings may span lines, `#` starts a comment, a line
/// opened inside an unclosed bracket is a continuation, and only indented
/// statements (command bodies) are candidates. The capture form
/// (`out = K=v tool`) and `exec K=v tool` already parse correctly.
fn mark_env_prefixed_statements(src: &str) -> std::borrow::Cow<'_, str> {
    if !src.contains('=') {
        return std::borrow::Cow::Borrowed(src);
    }
    let mut out = String::with_capacity(src.len() + 16);
    let mut in_backtick = false;
    let mut depth: i64 = 0;
    let mut changed = false;
    for (n, line) in src.split('\n').enumerate() {
        if n > 0 {
            out.push('\n');
        }
        let continuation = in_backtick || depth > 0;
        let trimmed = line.trim_start_matches([' ', '\t']);
        let indent = line.len() - trimmed.len();
        if !continuation && indent > 0 && starts_env_prefixed_call(trimmed) {
            out.push_str(&line[..indent]);
            out.push_str("exec ");
            out.push_str(trimmed);
            changed = true;
        } else {
            out.push_str(line);
        }
        scan_line_state(line, &mut in_backtick, &mut depth);
    }
    if changed {
        std::borrow::Cow::Owned(out)
    } else {
        std::borrow::Cow::Borrowed(src)
    }
}

/// Updates the multi-line state (open backtick string, bracket depth) after one
/// source line, ignoring quoted text and `#` comments.
fn scan_line_state(line: &str, in_backtick: &mut bool, depth: &mut i64) {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if *in_backtick {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'`' {
                *in_backtick = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'`' => *in_backtick = true,
            b'#' => return,
            b'"' | b'\'' => {
                i += 1;
                while i < b.len() && b[i] != c {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'(' | b'{' | b'[' => *depth += 1,
            b')' | b'}' | b']' => *depth -= 1,
            _ => {}
        }
        i += 1;
    }
}

/// Whether `stmt` (an indented statement, indent stripped) begins with an
/// assignment-shaped word (`NAME=VALUE`, or a malformed `1X=…` / `=…`) and has
/// at least one more token after it.
fn starts_env_prefixed_call(stmt: &str) -> bool {
    let b = stmt.as_bytes();
    // First word, honoring quotes: up to the first unquoted space/tab.
    let mut i = 0;
    let mut eq: Option<usize> = None;
    while i < b.len() && b[i] != b' ' && b[i] != b'\t' {
        match b[i] {
            b'"' | b'\'' => {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'=' if eq.is_none() => eq = Some(i),
            b'#' => return false,
            _ => {}
        }
        i += 1;
    }
    let Some(eq) = eq else { return false };
    let word = &stmt[..i.min(stmt.len())];
    // `a==b` / `a=>b` are not assignments.
    if matches!(b.get(eq + 1), Some(b'=') | Some(b'>')) {
        return false;
    }
    if !(parse_env_assignment(word).is_some() || is_malformed_env_assignment(word)) {
        return false;
    }
    // Something other than a comment must follow the word.
    let rest = stmt[i.min(stmt.len())..].trim_start();
    !rest.is_empty() && !rest.starts_with('#')
}

/// One split argv token plus a classification: `bare` is true when the token in
/// the LITERAL source was an unquoted bare identifier (no quotes, no `${…}`,
/// matches `[A-Za-z_][A-Za-z0-9_]*`). A bare token is treated like a CLI
/// variable reference — it resolves to a binding of that name if one exists,
/// otherwise it stays the literal token text (see InterpolateArgs). Quoted
/// tokens (`"foo"`), `${x}` placeholders, flags (`-q`), and `k=v` pairs are NOT
/// bare and pass through as literal/interpolated positional slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArgToken {
    pub text: String,
    pub bare: bool,
}

/// Splits a tail-captured argv string into tokens, honoring double quotes (a
/// quoted run is one token, with the surrounding quotes stripped) and backslash
/// escapes. Runs on the LITERAL source, so `${x}` placeholders pass through as
/// single tokens and are interpolated later.
#[allow(dead_code)]
pub(crate) fn shell_split_args(s: &str) -> Vec<String> {
    shell_split_args_classified(s).into_iter().map(|t| t.text).collect()
}

/// [`shell_split_args`] that also records, per token, whether it was an
/// unquoted bare identifier (see [`ArgToken`]).
pub(crate) fn shell_split_args_classified(s: &str) -> Vec<ArgToken> {
    fn flush(
        out: &mut Vec<ArgToken>,
        cur: &mut Vec<u8>,
        started: &mut bool,
        saw_quote_or_dollar: &mut bool,
    ) {
        out.push(ArgToken {
            text: String::from_utf8_lossy(cur).into_owned(),
            bare: !*saw_quote_or_dollar && is_bare_ident(cur),
        });
        cur.clear();
        *started = false;
        *saw_quote_or_dollar = false;
    }
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut in_quote = false;
    let mut started = false;
    let mut saw_quote_or_dollar = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'\\' && i + 1 < b.len() {
            cur.push(b[i + 1]);
            i += 1;
            started = true;
            saw_quote_or_dollar = true;
        } else if c == b'"' {
            in_quote = !in_quote;
            started = true;
            saw_quote_or_dollar = true;
        } else if (c == b' ' || c == b'\t') && !in_quote {
            if started {
                flush(&mut out, &mut cur, &mut started, &mut saw_quote_or_dollar);
            }
        } else {
            if c == b'$' {
                saw_quote_or_dollar = true;
            }
            cur.push(c);
            started = true;
        }
        i += 1;
    }
    if started {
        flush(&mut out, &mut cur, &mut started, &mut saw_quote_or_dollar);
    }
    out
}

/// Whether `b` is a non-empty `[A-Za-z_][A-Za-z0-9_]*`.
fn is_bare_ident(b: &[u8]) -> bool {
    if b.is_empty() {
        return false;
    }
    b.iter().enumerate().all(|(i, &c)| {
        c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())
    })
}

/// Walks `ops`, replacing every `_template_call` op with the named template's
/// body. Positional args (`_0`, `_1`, …) are bound to the template's declared
/// arg names and substituted into string args values via the same ${NAME}
/// convention the runtime uses for bindings. `expanding` tracks the names
/// currently mid-expansion — re-entering one is recursion and rejected with a
/// clear error.
///
/// Unknown templates are LEFT IN PLACE as `_template_call` markers (no error).
/// The post-import pass ([`expand_all_templates`]) re-runs this with the full
/// template map after imports merge, then the final --check pass rejects any
/// remaining unresolved markers as "unknown template".
fn expand_template_ops(
    ops: &[Op],
    templates: &BTreeMap<String, Template>,
    expanding: &HashSet<String>,
) -> Result<Vec<Op>, String> {
    let mut out: Vec<Op> = Vec::with_capacity(ops.len());
    for op in ops {
        let mut op = op.clone();
        // Block ops carry a body. Recurse into it whether or not this op is
        // itself a template call so nested calls inside a `retry` / `parallel`
        // / `if` block also resolve.
        if !op.body.is_empty() && op.kind != "_template_call" {
            op.body = expand_template_ops(&op.body, templates, expanding)?;
        }
        if op.kind != "_template_call" {
            out.push(op);
            continue;
        }
        // Resolve the call. If the template isn't visible yet (an imported one
        // not merged in), pass through unchanged so the post-import expansion
        // pass can resolve it.
        let name = op.args.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let Some(tpl) = templates.get(&name) else {
            out.push(op);
            continue;
        };
        if expanding.contains(&name) {
            return Err(format!(
                "line {}: template {} calls itself (recursion is not allowed)",
                op.line,
                go_quote(&name)
            ));
        }
        // Bind positional args to declared template args.
        let mut bindings: BTreeMap<String, String> = BTreeMap::new();
        for (idx, spec) in tpl.args.iter().enumerate() {
            let key = format!("_{idx}");
            let val = match op.args.get(&key) {
                Some(v) => v.clone(),
                None if spec.has_default => spec.default.clone(),
                None if spec.optional => Value::String(String::new()),
                None => {
                    return Err(format!(
                        "line {}: template {} missing positional arg #{} ({})",
                        op.line,
                        go_quote(&name),
                        idx,
                        spec.name
                    ))
                }
            };
            bindings.insert(spec.name.clone(), stringify_arg(&val));
        }
        // Expand the template body, then substitute bindings into every string
        // args entry. The body might contain its own template calls — recurse
        // first.
        let mut next_expanding = expanding.clone();
        next_expanding.insert(name.clone());
        let body = expand_template_ops(&tpl.ops, templates, &next_expanding)?;
        out.extend(substitute_ops(&body, &bindings, &name));
    }
    Ok(out)
}

/// Deep-clones a slice of ops, replacing ${NAME} occurrences in every
/// string-valued args entry with the bound value, and tagging each emitted op
/// with `expanded_from` for diagnostics.
fn substitute_ops(ops: &[Op], bindings: &BTreeMap<String, String>, template_name: &str) -> Vec<Op> {
    ops.iter()
        .map(|op| {
            let mut o = op.clone();
            o.expanded_from = template_name.to_string();
            if !op.args.is_empty() {
                let mut new_args = Map::new();
                for (k, v) in &op.args {
                    match v {
                        Value::String(s) => {
                            new_args.insert(k.clone(), Value::String(substitute_string(s, bindings)));
                        }
                        // `env_prefix` (R05): template args substitute into the values.
                        Value::Object(m) => {
                            let mut nm = Map::new();
                            for (nk, nv) in m {
                                let nv = match nv {
                                    Value::String(s) => Value::String(substitute_string(s, bindings)),
                                    other => other.clone(),
                                };
                                nm.insert(nk.clone(), nv);
                            }
                            new_args.insert(k.clone(), Value::Object(nm));
                        }
                        _ => {
                            new_args.insert(k.clone(), v.clone());
                        }
                    }
                }
                o.args = new_args;
            }
            if !op.body.is_empty() {
                o.body = substitute_ops(&op.body, bindings, template_name);
            }
            o
        })
        .collect()
}

/// Replaces every ${NAME} in `s` with `bindings[NAME]`. Unknown names are left
/// as literal ${...} so the runtime interpolation step can still complain about
/// them in the post-expansion error path (which already knows about user-scope
/// bindings).
fn substitute_string(s: &str, bindings: &BTreeMap<String, String>) -> String {
    if !s.contains("${") {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if i + 1 < b.len() && b[i] == b'$' && b[i + 1] == b'{' {
            let Some(end) = s[i + 2..].find('}') else {
                out.push_str(&s[i..]);
                return out;
            };
            let name = &s[i + 2..i + 2 + end];
            match bindings.get(name) {
                Some(v) => out.push_str(v),
                None => out.push_str(&s[i..i + 2 + end + 1]),
            }
            i += 2 + end + 1;
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Renders a positional template-call arg to the string form the substitution
/// model expects. Strings come through unquoted; numbers and bools render like
/// Go's `%v`.
fn stringify_arg(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            // Integer-valued floats render without trailing zero, matching the
            // interpreter's ToStringValue behaviour.
            match n.as_f64() {
                Some(x) if x == (x as i64) as f64 => format!("{}", x as i64),
                Some(x) => format!("{x}"),
                None => n.to_string(),
            }
        }
        other => other.to_string(),
    }
}

fn apply_config(
    cmd: Option<&mut Command>,
    catch: Option<&mut perch_domain::Catch>,
    ev: &Event,
) -> Result<(), String> {
    if let Some(cmd) = cmd {
        return apply_config_to_command(cmd, ev);
    }
    if let Some(catch) = catch {
        match ev.kind.as_str() {
            "description" => catch.description = as_string(&ev.value),
            // Explicit opt-in to binding ${proxy_args} in the catch body.
            // Without this modifier, ${proxy_args} is unbound inside the catch
            // and referencing it errors — the catch→shell forwarding pattern is
            // no longer accidental.
            "proxy_args" => catch.proxy_args = true,
            // Other config kinds silently ignored for catch.
            _ => {}
        }
        return Ok(());
    }
    Err(format!("config '{}' outside command/catch", ev.kind))
}

fn apply_config_to_command(c: &mut Command, ev: &Event) -> Result<(), String> {
    match ev.kind.as_str() {
        "description" => c.description = as_string(&ev.value),
        "private" => c.modifiers.private = true,
        "detached" => c.modifiers.detached = true,
        "proxy_args" => c.modifiers.proxy_args = true,
        "require_os" => c.modifiers.require_os.push(as_string(&ev.value)),
        "require_arch" => c.modifiers.require_arch.push(as_string(&ev.value)),
        "dir" => c.modifiers.dir = as_string(&ev.value),
        "on_signal" => c.modifiers.on_signal = as_string(&ev.value),
        "env" => {
            c.env.insert(ev.name.clone(), as_string(&ev.value));
        }
        "test" => c.modifiers.test = true,
        "test_allow_network" => c.modifiers.test_allow_network = true,
        "test_allow_shell" => c.modifiers.test_allow_shell = true,
        "test_allow_write" => c.modifiers.test_allow_write = true,
        "test_allow_subprocess" => c.modifiers.test_allow_subprocess = true,
        "test_keep_cwd" => c.modifiers.test_keep_cwd = true,
        "test_timeout" => c.modifiers.test_timeout_secs = as_floatish(&ev.value) as i64,
        _ => return Err(format!("unknown config kind: {}", go_quote(&ev.kind))),
    }
    Ok(())
}

fn as_floatish(v: &Value) -> f64 {
    v.as_f64().unwrap_or(0.0)
}

fn as_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn infer_literal_type(v: &Value) -> &'static str {
    match v {
        Value::Bool(_) => "bool",
        Value::Number(n) => {
            // JSON unmarshal gives all numbers as float64; distinguish int vs
            // float by checking the fractional part.
            let x = n.as_f64().unwrap_or(0.0);
            if x == (x as i64) as f64 {
                "int"
            } else {
                "float"
            }
        }
        Value::String(x) => {
            // Values are captured quote-optionally and normalized to a string by
            // the grammar's asString (`n = 42` and `n = "42"` are
            // indistinguishable — that's the cost of optional quotes), so
            // recover the type by parsing.
            if x == "true" || x == "false" {
                return "bool";
            }
            if x.parse::<i64>().is_ok() {
                return "int";
            }
            if x.parse::<f64>().is_ok() {
                return "float";
            }
            "string"
        }
        _ => "string",
    }
}

// ── Go path/quote helpers ───────────────────────────────────────────────────

/// Lexically cleans a path (Go's `filepath.Clean`).
fn go_clean(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// `filepath.Abs`: falls back to the input on failure.
fn go_abs(path: &str) -> String {
    if Path::new(path).is_absolute() {
        return go_clean(Path::new(path)).to_string_lossy().into_owned();
    }
    match std::env::current_dir() {
        Ok(cwd) => go_clean(&cwd.join(path)).to_string_lossy().into_owned(),
        Err(_) => path.to_string(),
    }
}

/// `filepath.Join` for two elements.
fn go_join(a: &str, b: &str) -> String {
    go_clean(&Path::new(a).join(b)).to_string_lossy().into_owned()
}

/// `filepath.Dir`.
fn go_dir(p: &str) -> String {
    match Path::new(p).parent() {
        Some(d) if !d.as_os_str().is_empty() => go_clean(d).to_string_lossy().into_owned(),
        _ => ".".to_string(),
    }
}

/// `filepath.Base`.
fn go_base(p: &str) -> String {
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        return if p.is_empty() { ".".into() } else { "/".into() };
    }
    match t.rfind('/') {
        Some(i) => t[i + 1..].to_string(),
        None => t.to_string(),
    }
}

/// Go's `%q` / `strconv.Quote`.
pub(crate) fn go_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\x0b' => out.push_str("\\v"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests;
