//! The shim is a child process on three pipes: the bundle goes in on
//! stdin while the child may already be writing stdout/stderr. Feeding
//! stdin to completion BEFORE draining the other two deadlocks as soon as
//! the child fills a pipe buffer (64 KiB on Linux) before it has read all
//! of its input — the parent blocks writing, the child blocks writing, and
//! neither ever returns (concurrency audit 2026-10, finding W1).

use std::sync::mpsc;
use std::time::Duration;

use super::{WEBCRACK_DEADLINE, WebcrackShim, unpack_webcrack};

/// A shim that writes 256 KiB to stderr (a dependency's warnings, say)
/// before it reads its stdin, then reads it all and prints the contract
/// line.
fn chatty_shim() -> WebcrackShim {
    WebcrackShim {
        program: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "head -c 262144 /dev/zero >&2; cat >/dev/null; echo '{\"files\":[]}'".to_string(),
            "shim".to_string(),
        ],
        cwd: None,
        deadline: WEBCRACK_DEADLINE,
    }
}

#[test]
fn a_shim_that_writes_before_it_reads_does_not_deadlock() {
    // Bigger than any pipe buffer, so the stdin write cannot complete
    // until the child reads.
    let code = "x".repeat(4 << 20);
    let out_dir = std::env::temp_dir();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(unpack_webcrack(&code, &out_dir, &chatty_shim()));
    });
    let result = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("unpack_webcrack deadlocked on the shim's pipes");
    let unpacked = result.expect("the shim's contract line parses");
    assert!(unpacked.files.is_empty());
}

#[test]
fn a_shim_that_exits_without_reading_is_an_error_not_a_hang() {
    let shim = WebcrackShim {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "exit 3".to_string(), "shim".to_string()],
        cwd: None,
        deadline: WEBCRACK_DEADLINE,
    };
    let code = "x".repeat(4 << 20);
    let out_dir = std::env::temp_dir();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(unpack_webcrack(&code, &out_dir, &shim));
    });
    let result = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("unpack_webcrack hung on a shim that exited");
    assert!(result.is_err(), "a failing shim is an error");
}

/// A webcrack that never finishes (an infinite loop in a transform, a hung
/// npx) used to hang the run forever: the W1 follow-up. Now the shim's
/// whole run sits under one deadline, the shim is killed and the unpack
/// fails loudly, naming the deadline.
#[test]
fn a_shim_past_its_deadline_is_killed_and_fails_loudly() {
    let shim = WebcrackShim {
        program: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "cat >/dev/null; sleep 60".to_string(),
            "shim".to_string(),
        ],
        cwd: None,
        deadline: Duration::from_millis(500),
    };
    let out_dir = std::env::temp_dir();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(unpack_webcrack("x", &out_dir, &shim));
    });
    let result = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("unpack_webcrack hung past the shim's deadline");
    let err = result.expect_err("a shim past its deadline is an error");
    assert!(err.contains("timed out after 0.5s"), "{err}");
}

/// The deadline is chosen, not defaulted: 30 minutes against a measured
/// 12 s for a 2.9 MB / 3,000-module webpack bundle (docs/concurrency.md,
/// "Subprocesses").
#[test]
fn the_production_deadline_is_thirty_minutes() {
    assert_eq!(WEBCRACK_DEADLINE, Duration::from_secs(30 * 60));
}

/// A scratch directory, removed on drop.
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "humanify-webcrack-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Scratch(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `target` spelled RELATIVE to this process's working directory (up to
/// the root, then down), without changing the working directory — tests
/// run in parallel threads of one process.
fn relative_to_cwd(target: &std::path::Path) -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap();
    let mut rel = std::path::PathBuf::new();
    for _ in cwd.components().skip(1) {
        rel.push("..");
    }
    rel.join(target.strip_prefix("/").unwrap())
}

/// A shim shaped like the real one: it runs in ITS OWN working directory
/// (the repo root, where node_modules resolves), writes
/// `<out>/deobfuscated.js` and reports `path.join(outputDir, file)` — a
/// path in whatever spelling the out dir arrived in.
fn webcrack_like_shim(cwd: &std::path::Path) -> WebcrackShim {
    WebcrackShim {
        program: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "cat >/dev/null; mkdir -p \"$1\" && echo x > \"$1/deobfuscated.js\" && \
             printf '{\"files\":[{\"path\":\"%s/deobfuscated.js\"}]}\\n' \"$1\""
                .to_string(),
            "shim".to_string(),
        ],
        cwd: Some(cwd.to_path_buf()),
        deadline: WEBCRACK_DEADLINE,
    }
}

/// exp182: `-o out` (any relative output dir) went to the shim as-is and
/// the shim resolved it against ITS working directory — the repo root —
/// so webcrack wrote `<repo>/out/deobfuscated.js` (the stray `out/` in the
/// main checkout) and reported `out/deobfuscated.js`, which the run then
/// read against ITS OWN working directory: "read out/deobfuscated.js: No
/// such file". The adapter hands the shim an absolute directory, so the
/// files land in the run's output dir and the reported paths are the ones
/// written.
#[test]
fn a_relative_out_dir_lands_in_the_runs_out_dir_not_the_shims_cwd() {
    let repo = Scratch::new("repo");
    let run = Scratch::new("run");
    let out_abs = run.0.join("out");
    let out_rel = relative_to_cwd(&out_abs);
    assert!(out_rel.is_relative());
    let unpacked = unpack_webcrack("x", &out_rel, &webcrack_like_shim(&repo.0))
        .expect("the shim's contract line parses");
    assert_eq!(unpacked.files.len(), 1);
    let file = &unpacked.files[0].path;
    assert!(file.is_absolute(), "{}", file.display());
    assert!(file.starts_with(&out_abs), "{}", file.display());
    assert!(
        out_abs.join("deobfuscated.js").is_file(),
        "written in the run's out dir"
    );
    let stray: Vec<_> = std::fs::read_dir(&repo.0).unwrap().collect();
    assert!(
        stray.is_empty(),
        "nothing written in the shim's cwd: {stray:?}"
    );
}

/// The shim's file list is trusted only inside the out dir: a reported
/// path elsewhere (a webcrack that wrote beside it) is an error naming
/// it, never a file the run reads.
#[test]
fn a_shim_file_outside_the_out_dir_is_refused() {
    let run = Scratch::new("outside");
    let out = run.0.join("out");
    let shim = WebcrackShim {
        program: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "cat >/dev/null; echo '{\"files\":[{\"path\":\"/etc/passwd\"}]}'".to_string(),
            "shim".to_string(),
        ],
        cwd: None,
        deadline: WEBCRACK_DEADLINE,
    };
    let err = unpack_webcrack("x", &out, &shim).expect_err("outside the out dir");
    assert!(err.contains("/etc/passwd"), "{err}");
}
