//! Linux backend: Landlock ruleset applied in a `pre_exec` hook.
use super::{ConfineError, Scopes, Support};
use landlock::{
    Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr, RulesetStatus, ABI,
};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

/// Lowest ABI we require: V1 (Linux 5.13) filesystem rules.
const ABI_MIN: ABI = ABI::V1;

pub fn probe() -> Support {
    // Creating a ruleset in a throwaway child is the reliable probe, but a
    // plain ruleset creation (not restrict_self) is side-effect free.
    match Ruleset::default().handle_access(AccessFs::from_all(ABI_MIN)) {
        Ok(r) => match r.create() {
            Ok(_) => Support::Enforced,
            Err(e) => Support::Unsupported(format!("Landlock unavailable: {e}")),
        },
        Err(e) => Support::Unsupported(format!("Landlock unavailable: {e}")),
    }
}

/// Paths needed so ordinary dynamically linked binaries can start.
const EXEC_READ: &[&str] = &["/usr", "/lib", "/lib64", "/lib32", "/bin", "/sbin", "/etc", "/proc/self"];
/// Device files: read+write file access only (dir-only rights are invalid on files).
const DEV_FILES: &[&str] = &["/dev/null", "/dev/urandom", "/dev/zero", "/dev/tty"];

pub fn confine(cmd: &mut Command, scopes: &Scopes) -> Result<(), ConfineError> {
    if let Support::Unsupported(r) = probe() {
        return Err(ConfineError::Unsupported(r));
    }
    let read: Vec<PathBuf> = scopes.read.clone();
    let write: Vec<PathBuf> = scopes.write.clone();
    // SAFETY: the closure only calls landlock syscalls and opens paths; it does
    // not allocate-and-lock across fork in a way that can deadlock beyond what
    // the landlock crate already does for this documented use.
    unsafe {
        cmd.pre_exec(move || {
            apply(&read, &write).map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))
        });
    }
    Ok(())
}

fn apply(read: &[PathBuf], write: &[PathBuf]) -> Result<(), String> {
    let all = AccessFs::from_all(ABI_MIN);
    let ro = AccessFs::from_read(ABI_MIN);
    let mut rs = Ruleset::default()
        .handle_access(all)
        .map_err(|e| e.to_string())?
        .create()
        .map_err(|e| e.to_string())?;
    let sys = EXEC_READ.iter().map(PathBuf::from);
    for p in sys.chain(read.iter().cloned()) {
        if let Ok(fd) = PathFd::new(&p) {
            rs = rs.add_rule(PathBeneath::new(fd, ro)).map_err(|e| e.to_string())?;
        }
    }
    for p in DEV_FILES {
        if let Ok(fd) = PathFd::new(p) {
            let acc = AccessFs::ReadFile | AccessFs::WriteFile;
            rs = rs.add_rule(PathBeneath::new(fd, acc)).map_err(|e| e.to_string())?;
        }
    }
    for p in write {
        let fd = PathFd::new(p).map_err(|e| e.to_string())?;
        rs = rs.add_rule(PathBeneath::new(fd, all)).map_err(|e| e.to_string())?;
    }
    let st = rs.restrict_self().map_err(|e| e.to_string())?;
    if st.ruleset == RulesetStatus::NotEnforced {
        return Err("Landlock ruleset not enforced".to_string());
    }
    Ok(())
}
