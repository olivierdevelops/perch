//! T-12..T-19: confinement of spawned binaries (PLAN-2026-0001 R03).
// Helpers are used only by platform-gated tests, so they look dead elsewhere.
#![allow(dead_code)]
use perch_ops::{confine, confine_probe, Scopes, Support};
use std::path::{Path, PathBuf};
use std::process::Command;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("perch-confine-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::canonicalize(&d).unwrap()
}

fn run(scopes: &Scopes, script: &str, cwd: &Path) -> std::process::Output {
    let mut c = Command::new("/bin/sh");
    c.arg("-c").arg(script).current_dir(cwd);
    confine(&mut c, scopes).expect("confine");
    c.output().expect("spawn")
}

#[test]
fn t12_probe_enforced_on_supported_platforms() {
    let p = confine_probe();
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        // Linux CI kernels older than 5.13 legitimately report Unsupported.
        if cfg!(target_os = "macos") {
            assert_eq!(p, Support::Enforced);
        }
    } else {
        assert!(matches!(p, Support::Unsupported(_)));
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod enforced {
    use super::*;

    fn enforced() -> bool {
        confine_probe() == Support::Enforced
    }

    #[test]
    fn t13_declared_write_works() {
        if !enforced() {
            return;
        }
        let root = tmp("w");
        let allowed = root.join("allowed");
        std::fs::create_dir_all(&allowed).unwrap();
        let s = Scopes { write: vec![allowed.clone()], ..Default::default() };
        let o = run(&s, "echo in > allowed/ok.txt", &root);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert_eq!(std::fs::read_to_string(allowed.join("ok.txt")).unwrap().trim(), "in");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn t14_write_escape_refused() {
        if !enforced() {
            return;
        }
        let root = tmp("esc");
        let outside = tmp("esc-out");
        let allowed = root.join("allowed");
        std::fs::create_dir_all(&allowed).unwrap();
        let s = Scopes { write: vec![allowed], ..Default::default() };
        let o = run(&s, &format!("echo out > {}/x", outside.display()), &root);
        assert!(!o.status.success());
        assert!(!outside.join("x").exists());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn t15_read_outside_scope_refused() {
        if !enforced() {
            return;
        }
        let root = tmp("rd");
        let secret = tmp("rd-secret");
        std::fs::write(secret.join("s.txt"), "topsecret").unwrap();
        std::fs::write(root.join("pub.txt"), "public").unwrap();
        let s = Scopes { read: vec![root.clone()], ..Default::default() };
        let ok = run(&s, "cat pub.txt", &root);
        assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
        assert_eq!(String::from_utf8_lossy(&ok.stdout).trim(), "public");
        let bad = run(&s, &format!("cat {}/s.txt", secret.display()), &root);
        assert!(!bad.status.success());
        assert!(!String::from_utf8_lossy(&bad.stdout).contains("topsecret"));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&secret);
    }

    #[test]
    fn t16_write_implies_read() {
        if !enforced() {
            return;
        }
        let root = tmp("wr");
        std::fs::write(root.join("f"), "data").unwrap();
        let s = Scopes { write: vec![root.clone()], ..Default::default() };
        let o = run(&s, "cat f", &root);
        assert!(o.status.success());
        assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "data");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn t17_network_denied_without_hosts() {
        if !enforced() {
            return;
        }
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let root = tmp("net");
        let s = Scopes { read: vec![root.clone()], ..Default::default() };
        let script = format!("exec 3<>/dev/tcp/127.0.0.1/{port}");
        // /bin/sh on macOS is bash-in-posix-mode and supports /dev/tcp.
        let denied = run(&s, &script, &root);
        assert!(!denied.status.success(), "network should be denied");
        let s2 = Scopes { hosts: vec!["127.0.0.1".into()], ..s };
        let allowed = run(&s2, &script, &root);
        assert!(allowed.status.success(), "{}", String::from_utf8_lossy(&allowed.stderr));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn t18_ordinary_binaries_run() {
        if !enforced() {
            return;
        }
        let root = tmp("ord");
        let s = Scopes { read: vec![root.clone()], ..Default::default() };
        let o = run(&s, "echo hi; cat /dev/null; ls . >/dev/null; git --version >/dev/null 2>&1; true", &root);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "hi");
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[test]
fn t19_unsupported_platform_refuses() {
    let mut c = Command::new("true");
    assert!(confine(&mut c, &Scopes::default()).is_err());
}
