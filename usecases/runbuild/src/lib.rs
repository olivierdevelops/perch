//! Produces a portable, self-extracting perch binary with the loaded program
//! embedded, and optionally a tarball of an arbitrary file tree (`--include
//! <path>`) so the resulting binary can install a Python / JS / any non-native
//! project anywhere.
use flate2::write::GzEncoder;
use flate2::Compression;
use perch_domain::Program;
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type LoadFn = Box<dyn Fn(&str) -> Result<Program, Error>>;
/// `(source_binary, program, archive, out_path)`; an empty archive means "no
/// bundle" (Go passes a nil slice).
pub type EmbedFn = Box<dyn Fn(&str, &Program, &[u8], &str) -> Result<(), Error>>;

pub struct Impl {
    pub load: LoadFn,
    pub embed: EmbedFn,
}

const USAGE: &str = "Usage of build:\n  -include string\n    \tPath (file or directory) to embed as a tarball inside the binary\n  -o string\n    \tOutput binary path (default: program name from commands.perch)";

/// Parses `-o` / `-include` string flags the way Go's `flag` package does. Go's
/// ExitOnError prints the message plus usage and exits 2; here both are the
/// returned error's text.
fn parse_flags(args: &[String]) -> Result<(String, String), Error> {
    let (mut out, mut include) = (String::new(), String::new());
    let fail = |msg: String| -> Error { format!("{msg}\n{USAGE}").into() };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if !a.starts_with('-') || a == "-" || a == "--" {
            break;
        }
        let name = a.trim_start_matches('-');
        if name.is_empty() || name.starts_with('-') || name.starts_with('=') || a.starts_with("---") {
            return Err(fail(format!("bad flag syntax: {a}")));
        }
        let (name, inline) = match name.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (name, None),
        };
        if name != "o" && name != "include" {
            return Err(fail(format!("flag provided but not defined: -{name}")));
        }
        i += 1;
        let value = match inline {
            Some(v) => v,
            None => match args.get(i) {
                Some(v) => {
                    i += 1;
                    v.clone()
                }
                None => return Err(fail(format!("flag needs an argument: -{name}"))),
            },
        };
        if name == "o" {
            out = value;
        } else {
            include = value;
        }
    }
    Ok((out, include))
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Formats an io error the way Go's `*PathError` prints (`stat P: no such file
/// or directory`).
fn go_path_err(op: &str, path: &Path, e: &std::io::Error) -> String {
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
    format!("{op} {}: {msg}", path.display())
}

/// Go's `filepath.Abs` (lexical clean; no symlink resolution).
fn abs(p: &str) -> String {
    let path = Path::new(p);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            c => out.push(c),
        }
    }
    out.to_string_lossy().into_owned()
}

impl Impl {
    /// Progress lines go to `out` (Go: stdout).
    pub fn execute(&self, config_path: &str, args: &[String], out: &mut dyn Write) -> Result<(), Error> {
        let (out_flag, include) = parse_flags(args)?;

        let p = (self.load)(config_path)?;

        let mut out_path = out_flag;
        if out_path.is_empty() {
            out_path = p.name.clone();
            if out_path.is_empty() {
                out_path = "perch-app".into();
            }
        }

        let me = std::env::current_exe().map_err(|e| -> Error { format!("locate self: {e}").into() })?;

        // Collect the include set from two sources:
        //   1. The .perch file's `bundle ... end` section (declarative).
        //   2. The CLI `--include PATH` flag (additive).
        // Relative paths in the bundle section resolve against the .perch
        // file's directory; CLI paths resolve against $cwd as ever.
        let mut includes: Vec<String> = Vec::new();
        if !p.bundle.includes.is_empty() {
            let script_dir = Path::new(&p.script_path).parent().map(|d| d.to_path_buf()).unwrap_or_default();
            let script_dir = if script_dir.as_os_str().is_empty() { PathBuf::from(".") } else { script_dir };
            for rel in &p.bundle.includes {
                let abs_p = if Path::new(rel).is_absolute() {
                    rel.clone()
                } else {
                    clean(&script_dir.join(rel)).to_string_lossy().into_owned()
                };
                includes.push(abs_p);
            }
        }
        if !include.is_empty() {
            includes.push(include);
        }

        let mut archive: Vec<u8> = Vec::new();
        if !includes.is_empty() {
            archive = tarball_paths(&includes, out).map_err(|e| -> Error { format!("tar bundle: {e}").into() })?;
            writeln!(
                out,
                "✓ embedded {} bytes from {} source{}",
                archive.len(),
                includes.len(),
                plural(includes.len())
            )?;
        }

        (self.embed)(&me.to_string_lossy(), &p, &archive, &out_path)?;

        writeln!(out, "Built binary: {}", abs(&out_path))?;
        Ok(())
    }
}

/// Lexical clean like Go's `filepath.Join` (resolves `.` and `..`).
fn clean(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            c => out.push(c),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// The multi-root tarball: each root is added to one combined gzipped tar;
/// later roots can shadow earlier ones (CLI --include is listed after the
/// declarative bundle section, so a CLI flag wins on collision). Used by
/// `bundle ... end` plus `--include`.
pub fn tarball_paths(roots: &[String], out: &mut dyn Write) -> Result<Vec<u8>, Error> {
    let gz = GzEncoder::new(Vec::new(), Compression::default());
    let mut tw = tar::Builder::new(gz);
    let mut seen: HashSet<String> = HashSet::new();
    for root in roots {
        writeln!(out, "Bundling {root} …")?;
        tar_add_root(&mut tw, Path::new(root), &mut seen).map_err(|e| -> Error { format!("{root}: {e}").into() })?;
    }
    let gz = tw.into_inner()?;
    Ok(gz.finish()?)
}

/// Adds one root (file or directory) to an open tar writer. `seen`
/// deduplicates by tar entry name so the last-write-wins semantics of multiple
/// roots stay clean.
fn tar_add_root(tw: &mut tar::Builder<GzEncoder<Vec<u8>>>, root: &Path, seen: &mut HashSet<String>) -> Result<(), Error> {
    let info = fs::metadata(root).map_err(|e| -> Error { go_path_err("stat", root, &e).into() })?;
    if !info.is_dir() {
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| ".".into());
        if !seen.insert(name.clone()) {
            return Ok(());
        }
        return tar_add_file(tw, root, &name, &info);
    }
    walk(tw, root, root, seen)
}

/// Mirrors `filepath.Walk`: lexical order, descends into a directory right
/// after visiting it.
fn walk(tw: &mut tar::Builder<GzEncoder<Vec<u8>>>, root: &Path, dir: &Path, seen: &mut HashSet<String>) -> Result<(), Error> {
    let mut names: Vec<std::ffi::OsString> = fs::read_dir(dir)
        .map_err(|e| -> Error { go_path_err("open", dir, &e).into() })?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<Result<_, _>>()?;
    names.sort();
    for n in names {
        let path = dir.join(&n);
        let fi = fs::symlink_metadata(&path).map_err(|e| -> Error { go_path_err("lstat", &path, &e).into() })?;
        let fname = n.to_string_lossy();
        if fi.is_dir()
            && matches!(fname.as_ref(), ".git" | "node_modules" | "__pycache__" | ".venv" | "venv" | ".tox" | "dist" | ".cache")
        {
            continue; // SkipDir
        }
        if fname == ".DS_Store" {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        if seen.insert(rel.clone()) {
            tar_add_file(tw, &path, &rel, &fi)?;
        }
        if fi.is_dir() {
            walk(tw, root, &path, seen)?;
        }
    }
    Ok(())
}

fn tar_add_file(tw: &mut tar::Builder<GzEncoder<Vec<u8>>>, src: &Path, name: &str, fi: &fs::Metadata) -> Result<(), Error> {
    let mut hdr = tar::Header::new_gnu();
    hdr.set_metadata(fi);
    hdr.set_path(name)?;
    if fi.is_dir() {
        hdr.set_size(0);
        hdr.set_cksum();
        tw.append(&hdr, std::io::empty())?;
        return Ok(());
    }
    if fi.file_type().is_symlink() {
        // Go writes a symlink header (size 0) then copies the target's bytes,
        // which fails with a write-too-long error for any non-empty target.
        let f = fs::read(src).map_err(|e| -> Error { go_path_err("open", src, &e).into() })?;
        if !f.is_empty() {
            return Err("archive/tar: write too long".into());
        }
        hdr.set_size(0);
        hdr.set_cksum();
        tw.append(&hdr, std::io::empty())?;
        return Ok(());
    }
    let f = fs::File::open(src).map_err(|e| -> Error { go_path_err("open", src, &e).into() })?;
    hdr.set_cksum();
    tw.append(&hdr, f)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn entries(gz: &[u8]) -> Vec<String> {
        let mut a = tar::Archive::new(GzDecoder::new(gz));
        a.entries().unwrap().map(|e| e.unwrap().path().unwrap().to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn flags() {
        assert_eq!(parse_flags(&[]).unwrap(), ("".into(), "".into()));
        assert_eq!(parse_flags(&s(&["-o", "x", "--include=/p"])).unwrap(), ("x".into(), "/p".into()));
        let e = parse_flags(&s(&["-z"])).unwrap_err().to_string();
        assert!(e.starts_with("flag provided but not defined: -z\nUsage of build:\n"));
        assert_eq!(parse_flags(&s(&["-o"])).unwrap_err().to_string().lines().next().unwrap(), "flag needs an argument: -o");
    }

    #[test]
    fn tar_skips_junk_and_dedups() {
        let d = std::env::temp_dir().join(format!("perch-build-{}", std::process::id()));
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::create_dir_all(d.join(".git")).unwrap();
        fs::create_dir_all(d.join("node_modules")).unwrap();
        fs::write(d.join("a.txt"), "a").unwrap();
        fs::write(d.join("sub/b.txt"), "b").unwrap();
        fs::write(d.join(".git/x"), "x").unwrap();
        fs::write(d.join(".DS_Store"), "x").unwrap();
        let mut sink = Vec::new();
        let gz = tarball_paths(&[d.to_string_lossy().into_owned(), d.join("a.txt").to_string_lossy().into_owned()], &mut sink).unwrap();
        assert_eq!(entries(&gz), vec!["a.txt", "sub", "sub/b.txt"]);
        assert!(String::from_utf8(sink).unwrap().starts_with("Bundling "));
        let e = tarball_paths(&["/nonexistent-zz".into()], &mut Vec::new()).unwrap_err().to_string();
        assert_eq!(e, "/nonexistent-zz: stat /nonexistent-zz: no such file or directory");
        fs::remove_dir_all(d).ok();
    }

    #[test]
    fn execute_embeds() {
        let d = std::env::temp_dir().join(format!("perch-build2-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("f.txt"), "hello").unwrap();
        let mut p = Program { name: "app".into(), script_path: d.join("commands.perch").to_string_lossy().into_owned(), ..Default::default() };
        p.bundle.includes = vec!["f.txt".into()];
        let got: Rc<RefCell<Option<(usize, String)>>> = Rc::new(RefCell::new(None));
        let g = got.clone();
        let imp = Impl {
            load: Box::new(move |_| Ok(p.clone())),
            embed: Box::new(move |_, _, arch, out| {
                *g.borrow_mut() = Some((arch.len(), out.to_string()));
                Ok(())
            }),
        };
        let mut out = Vec::new();
        imp.execute("c", &[], &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        let (n, o) = got.borrow().clone().unwrap();
        assert_eq!(o, "app");
        assert!(n > 0);
        assert!(out.contains(&format!("✓ embedded {n} bytes from 1 source\n")));
        assert!(out.ends_with(&format!("Built binary: {}\n", abs("app"))));
        fs::remove_dir_all(d).ok();
    }
}
