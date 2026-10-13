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
