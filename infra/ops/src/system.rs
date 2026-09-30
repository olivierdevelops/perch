//! System-fact ops: OS / arch / env / standard directories / process identity.
use crate::common::{arg_string, go_dir, go_join, user_cache_dir, user_config_dir, user_home_dir};
use crate::requires::{check_env_declared, check_subprocess_bin};
use perch_interpreter::{err, go_arch, go_os, handler, Args, Bindings, Handler, Interpreter, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Command;

fn s(v: impl Into<String>) -> Result<Value> {
    Ok(Value::String(v.into()))
}

fn env_name_valid(name: &str) -> bool {
    !name.is_empty() && !name.contains(['=', '\0'])
}

/// The canonical (symlink-resolved) path of the running binary.
fn exe_resolved() -> std::io::Result<String> {
    let p = std::env::current_exe()?;
    Ok(p.canonicalize().unwrap_or(p).to_string_lossy().into_owned())
}

fn app_data_dir_of(h: &str) -> String {
    if go_os() == "darwin" {
        go_join(&[h, "Library", "Application Support"])
    } else {
        go_join(&[h, ".local", "share"])
    }
}

pub fn register_system(m: &mut HashMap<String, Handler>) {
    let mut reg = |k: &str, h: Handler| {
        m.insert(k.to_string(), h);
    };
    reg("get_os", handler(|_i, _b, _a| s(go_os())));
    reg("get_arch", handler(|_i, _b, _a| s(go_arch())));
    reg(
        "get_env",
        handler(|i, _b, args| {
            let name = arg_string(args, &["name", "_0"]);
            check_env_declared(i, &name)?;
            s(std::env::var(&name).unwrap_or_default())
        }),
    );
    reg(
        "unset_env",
        handler(|i, b, args| {
            let name = arg_string(args, &["name", "_0"]);
            check_env_declared(i, &name)?;
            b.env.remove(&name); // drop from the binding overlay
            if env_name_valid(&name) {
                std::env::remove_var(&name); // and from the process env
            }
            Ok(Value::Null)
        }),
    );
    reg(
        "set_env",
        handler(|i, _b, args| {
            let name = arg_string(args, &["name", "_0"]);
            check_env_declared(i, &name)?;
            let value = arg_string(args, &["value", "_1"]);
            if !env_name_valid(&name) || value.contains('\0') {
                return Err(err("setenv: invalid argument"));
            }
            std::env::set_var(&name, value);
            Ok(Value::Null)
        }),
    );
    reg("cwd", handler(|_i, b, _a| s(b.cwd.clone())));
    reg("temp_dir", handler(|_i, _b, _a| s(std::env::temp_dir().to_string_lossy().into_owned())));
    reg("home_dir", handler(|_i, _b, _a| s(user_home_dir().unwrap_or_default())));
    reg(
        "app_data_dir",
        handler(|_i, _b, _a| {
            if go_os() == "windows" {
                return s(std::env::var("APPDATA").unwrap_or_default());
            }
            s(app_data_dir_of(&user_home_dir().unwrap_or_default()))
        }),
    );
    reg("cache_dir", handler(|_i, _b, _a| user_cache_dir().map(Value::String).map_err(err)));
    reg("pid", handler(|_i, _b, _a| Ok(Value::from(std::process::id()))));
    reg("user", handler(|_i, _b, _a| s(std::env::var("USER").unwrap_or_default())));
    reg("config_dir", handler(|_i, _b, _a| user_config_dir().map(Value::String).map_err(err)));
    reg(
        "data_dir",
        handler(|_i, _b, _a| {
            if go_os() == "windows" {
                return s(std::env::var("APPDATA").unwrap_or_default());
            }
            let h = user_home_dir().map_err(err)?;
            s(app_data_dir_of(&h))
        }),
    );
    reg("exe_path", handler(|_i, _b, _a| exe_resolved().map(Value::String).map_err(|e| err(e.to_string()))));
    reg(
        "exe_dir",
        handler(|_i, _b, _a| {
            let p = exe_resolved().map_err(|e| err(e.to_string()))?;
            s(go_dir(&p))
        }),
    );
    reg("script_path", handler(|i, _b, _a| s(i.program.script_path.clone())));
    reg(
        "script_dir",
        handler(|i, _b, _a| {
            if i.program.script_path.is_empty() {
                return s("");
            }
            s(go_dir(&i.program.script_path))
        }),
    );
    reg(
        "cpu_count",
        handler(|_i, _b, _a| Ok(Value::from(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)))),
    );
    // os_version: best-effort string identifying the running OS release. Uses
    // `sw_vers -productVersion` on macOS, `uname -r` on linux, `cmd /c ver` on
    // windows. Returns "" if probing fails.
    reg("os_version", handler(op_os_version));
    reg(
        "env_default",
        handler(|i, _b, args| {
            let name = arg_string(args, &["name", "_0"]);
            check_env_declared(i, &name)?;
            let def = arg_string(args, &["default", "_1"]);
            match std::env::var_os(&name) {
                Some(v) if !v.is_empty() => s(v.to_string_lossy().into_owned()),
                _ => s(def),
            }
        }),
    );
    reg(
        "env_has",
        handler(|i, _b, args| {
            let name = arg_string(args, &["name", "_0"]);
            check_env_declared(i, &name)?;
            Ok(Value::Bool(std::env::var_os(&name).is_some()))
        }),
    );
    reg("path_sep", handler(|_i, _b, _a| s(if cfg!(windows) { "\\" } else { "/" })));
    reg("path_list_sep", handler(|_i, _b, _a| s(if cfg!(windows) { ";" } else { ":" })));
    reg("exe_ext", handler(|_i, _b, _a| s(if go_os() == "windows" { ".exe" } else { "" })));
    reg("null_device", handler(|_i, _b, _a| s(if go_os() == "windows" { "NUL" } else { "/dev/null" })));
}

fn op_os_version(i: &Interpreter, _b: &mut Bindings, _args: &Args<'_>) -> Result<Value> {
    let tool = match go_os() {
        "darwin" => "sw_vers",
        "windows" => "cmd",
        _ => "uname",
    };
    check_subprocess_bin(i, tool)?;
    let mut cmd = match go_os() {
        "darwin" => {
            let mut c = Command::new("sw_vers");
            c.arg("-productVersion");
            c
        }
        "windows" => {
            let mut c = Command::new("cmd");
            c.arg("/c").arg("ver");
            c
        }
        _ => {
            let mut c = Command::new("uname");
            c.arg("-r");
            c
        }
    };
    match cmd.output() {
        Ok(o) if o.status.success() => {
            let mut v = o.stdout;
            v.extend(o.stderr);
            s(String::from_utf8_lossy(&v).trim())
        }
        _ => s(""),
    }
}
