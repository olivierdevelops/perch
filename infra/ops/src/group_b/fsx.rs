//! Cross-platform shims for the Unix-only bits of Go's `os` package: file
//! modes, symlinks, hostname. Windows has no POSIX mode bits, so modes there
//! are approximated the way Go does (only the read-only bit is meaningful).
use std::fs;
use std::io;

/// `fi.Mode().Perm()`-ish bits of a file's metadata.
pub fn mode_of(md: &fs::Metadata) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        md.mode()
    }
    #[cfg(not(unix))]
    {
        // Go reports 0666/0444 for files and 0777/0555 for dirs on Windows.
        let ro = md.permissions().readonly();
        match (md.is_dir(), ro) {
            (true, false) => 0o777,
            (true, true) => 0o555,
            (false, false) => 0o666,
            (false, true) => 0o444,
        }
    }
}

/// `os.Chmod(path, mode)`.
pub fn set_mode(path: &str, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        // Only the owner-write bit maps to Windows' read-only attribute.
        let mut p = fs::metadata(path)?.permissions();
        p.set_readonly(mode & 0o200 == 0);
        fs::set_permissions(path, p)
    }
}

/// Applies `mode` to new files opened with `opts` (no-op on Windows).
pub fn open_mode(opts: &mut fs::OpenOptions, mode: u32) -> &mut fs::OpenOptions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(mode)
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        opts
    }
}

/// Applies `mode` to directories created by `b` (no-op on Windows).
pub fn dir_mode(b: &mut fs::DirBuilder, mode: u32) -> &mut fs::DirBuilder {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(mode)
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
        b
    }
}

/// `os.Symlink(target, link)`.
pub fn symlink(target: &str, link: &str) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        // Windows distinguishes file and directory links; pick by the target.
        let base = std::path::Path::new(link).parent().unwrap_or_else(|| std::path::Path::new("."));
        let resolved = base.join(target);
        if resolved.is_dir() {
            std::os::windows::fs::symlink_dir(target, link)
        } else {
            std::os::windows::fs::symlink_file(target, link)
        }
    }
}

/// `os.Hostname()`.
pub fn hostname() -> io::Result<String> {
    #[cfg(unix)]
    {
        let mut buf = vec![0u8; 256];
        // SAFETY: the buffer is valid for its full length.
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(String::from_utf8_lossy(&buf[..end]).into_owned())
    }
    #[cfg(not(unix))]
    {
        std::env::var("COMPUTERNAME").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "hostname unavailable"))
    }
}

/// Whether `e` is ENOTDIR (a path component is not a directory).
pub fn is_not_dir(e: &io::Error) -> bool {
    #[cfg(unix)]
    {
        e.raw_os_error() == Some(libc::ENOTDIR)
    }
    #[cfg(not(unix))]
    {
        let _ = e;
        false
    }
}

/// Whether `e` is EAFNOSUPPORT (address family unavailable, e.g. no IPv6).
pub fn is_af_unsupported(e: &io::Error) -> bool {
    #[cfg(unix)]
    {
        e.raw_os_error() == Some(libc::EAFNOSUPPORT)
    }
    #[cfg(not(unix))]
    {
        let _ = e;
        false
    }
}
