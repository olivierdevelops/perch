//! Library facade tests (plan T-20..T-23 plus load/check/scan).
use perch::{Input, Policy, Runtime};
use std::time::Duration;

const SRC_BODY: &str = r#"
command hello
    description "Greet someone"
    arg name
        type string
        default "world"
        description "Who"
    end
    do
        print "hello ${name}"
    end
end

command boom
    do
        fail "kaboom"
    end
end

command sh
    do
        shell "echo hi"
    end
end

command put
    arg path
        type string
    end
    do
        write_file "${path}" "x"
        print "wrote"
    end
end

command nap
    do
        sleep 0.5
        print "late"
    end
end

command secret
    private
    do
        print "no"
    end
end
"#;

/// The test program, declaring `dir` as its write root (programs get zero
/// ambient authority: writes need a declared root).
fn src(dir: &std::path::Path) -> String {
    // `\` starts an escape inside a perch string literal, so spell Windows paths with `/`.
    let root = dir.display().to_string().replace('\\', "/");
    format!("requires\n    write \"{root}\"\nend\n{SRC_BODY}")
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("perch-rt-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn rt(p: Policy) -> (Runtime, perch::Loaded) {
    let rt = Runtime::new(p);
    let l = rt.load_str(&src(&scratch("misc"))).expect("load");
    (rt, l)
}

#[test]
fn t20_runs_a_command_with_structured_result() {
    let (rt, l) = rt(Policy::default());
    let r = rt.run(&l, "hello", &["-name".into(), "perch".into()]);
    assert!(r.ok, "{:?}", r.error);
    assert_eq!(r.stdout, "hello perch\n");
    assert_eq!(r.stderr, "");
    assert!(r.error.is_none());

    let r = rt.run(&l, "hello", &[]);
    assert_eq!(r.stdout, "hello world\n");

    let r = rt.run(&l, "boom", &[]);
    assert!(!r.ok);
    let e = r.error.unwrap();
    assert_eq!(e.kind, "user_fail");
    assert!(e.message.contains("kaboom"));

    let r = rt.run(&l, "nope", &[]);
    assert_eq!(r.error.unwrap().kind, "command_not_found");
    let r = rt.run(&l, "secret", &[]);
    assert_eq!(r.error.unwrap().kind, "command_not_found");
}

#[test]
fn t21_policy_denies_shell() {
    let (rt, l) = rt(Policy::default().no_shell(true));
    let r = rt.run(&l, "sh", &[]);
    assert!(!r.ok);
    let e = r.error.unwrap();
    assert_eq!(e.kind, "cap_shell_denied", "{e:?}");
    assert_eq!(e.op, "shell");
}

#[test]
fn t22_deadline_is_honored() {
    let (rt, l) = rt(Policy::default().max_runtime(Duration::from_millis(100)));
    let r = rt.run(&l, "nap", &[]);
    assert!(!r.ok);
    assert_eq!(r.error.unwrap().kind, "timeout_exceeded");
    // A running op is not interrupted; the NEXT op sees the deadline.
    assert!(r.duration < Duration::from_secs(3), "{:?}", r.duration);
    assert!(!r.stdout.contains("late"));
}

#[test]
fn t23_two_runtimes_two_policies_concurrently() {
    let dir = scratch("t23");
    let source = src(&dir);
    let allow = Runtime::new(Policy::default());
    let deny = Runtime::new(Policy::default().no_write(true));
    let la = allow.load_str(&source).unwrap();
    let ld = deny.load_str(&source).unwrap();
    let (fa, fd) = (dir.join("a.txt"), dir.join("d.txt"));
    std::thread::scope(|s| {
        let ha = s.spawn(|| {
            (0..20).map(|_| allow.run(&la, "put", &["-path".into(), fa.to_string_lossy().into_owned()])).collect::<Vec<_>>()
        });
        let hd = s.spawn(|| {
            (0..20).map(|_| deny.run(&ld, "put", &["-path".into(), fd.to_string_lossy().into_owned()])).collect::<Vec<_>>()
        });
        for r in ha.join().unwrap() {
            assert!(r.ok, "{:?}", r.error);
            assert_eq!(r.stdout, "wrote\n");
        }
        for r in hd.join().unwrap() {
            assert!(!r.ok);
            assert_eq!(r.error.unwrap().kind, "cap_write_denied");
            assert_eq!(r.stdout, "");
        }
    });
    assert!(fa.exists());
    assert!(!fd.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn working_dir_is_per_runtime() {
    let dir = scratch("cwd");
    let rt = Runtime::new(Policy::default().working_dir(&dir));
    let l = rt.load_str(&src(&dir)).unwrap();
    let r = rt.run(&l, "put", &["-path".into(), "rel.txt".into()]);
    assert!(r.ok, "{:?}", r.error);
    assert!(dir.join("rel.txt").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_str_reports_errors() {
    let rt = Runtime::new(Policy::default());
    let e = rt.load_str("command broken\n  this is not valid\n").unwrap_err();
    assert!(!e.message.is_empty());
    let e = rt.load_path(std::path::Path::new("/definitely/not/here.perch")).unwrap_err();
    assert!(!e.message.is_empty());
}

#[test]
fn commands_check_and_scan() {
    let (rt, l) = rt(Policy::default());
    let cmds = l.commands();
    let names: Vec<&str> = cmds.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["boom", "hello", "nap", "put", "sh"]);
    let hello = cmds.iter().find(|c| c.name == "hello").unwrap();
    assert_eq!(hello.description, "Greet someone");
    assert_eq!(hello.args[0].name, "name");
    assert!(hello.args[0].optional);

    assert!(l.check().iter().all(|i| i.severity != "error"), "{:?}", l.check());

    let bad = rt
        .load_str("command a\n    arg n\n        type int\n        default \"abc\"\n    end\n    do\n        print \"x\"\n    end\nend\n")
        .unwrap();
    assert!(bad.check().iter().any(|i| i.severity == "error"));

    let rep = l.scan();
    assert_eq!(rep.schema, 1);
    assert!(rep.inferred.shell.calls >= 1);
    assert!(rep.inferred.write);
}

#[test]
fn stdin_bytes_and_inherit_defaults() {
    let p = Policy::default().stdin(Input::Bytes(b"abc".to_vec()));
    assert_eq!(p.stdin, Input::Bytes(b"abc".to_vec()));
    assert_eq!(Policy::default().stdout, perch::Output::Capture);
}
