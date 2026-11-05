//! The subprocess owner (docs/concurrency.md rule 4): pipes drained
//! together, ONE deadline over the whole run, the child's whole process
//! group killed on expiry, every failure an error.

use std::process::Command;
use std::time::{Duration, Instant};

use super::{SubprocessError, run};

fn sh(script: &str) -> Command {
    let mut c = Command::new("sh");
    c.args(["-c", script, "sh"]);
    c
}

#[test]
fn stdin_is_fed_while_both_pipes_drain() {
    // The child writes more than a pipe buffer to BOTH outputs before it
    // reads a byte, then echoes a 4 MiB stdin back: any order of
    // "write all, then read" would deadlock (audit W1).
    let input = vec![b'x'; 4 << 20];
    let out = run(
        &mut sh("head -c 262144 /dev/zero >&2; head -c 262144 /dev/zero; cat; exit 4"),
        Some(input.clone()),
        Duration::from_secs(30),
    )
    .expect("finished");
    assert_eq!(
        out.status.code(),
        Some(4),
        "the status is the caller's to read"
    );
    assert_eq!(out.stderr.len(), 262144);
    assert_eq!(out.stdout.len(), 262144 + input.len());
}

#[test]
fn stdin_is_null_when_none_is_given() {
    let out = run(&mut sh("cat; echo done"), None, Duration::from_secs(30)).expect("finished");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "done\n");
}

#[test]
fn a_child_that_cannot_start_is_a_spawn_error() {
    let err = run(
        &mut Command::new("/nonexistent/humanify-no-such-program"),
        None,
        Duration::from_secs(30),
    )
    .unwrap_err();
    assert!(matches!(err, SubprocessError::Spawn(_)), "{err:?}");
}

#[test]
fn a_child_that_exits_0_without_reading_its_input_is_an_error() {
    let err = run(
        &mut sh("exit 0"),
        Some(vec![b'x'; 4 << 20]),
        Duration::from_secs(30),
    )
    .unwrap_err();
    assert!(
        matches!(&err, SubprocessError::Io(e) if e.contains("write stdin")),
        "{err:?}"
    );
}

/// The deadline covers the WHOLE run and kills the whole process group:
/// `sleep | cat` is two processes holding the output pipe, the shape of
/// `npx tsx shim` (the node grandchild holds the pipes, not npx).
#[test]
fn a_child_past_its_deadline_is_killed_with_its_group_and_reported() {
    let dir = std::env::temp_dir().join(format!("humanify-subprocess-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let pidfile = dir.join("grandchild.pid");
    let script = format!("sleep 60 | cat & echo $! > {}; wait", pidfile.display());
    let started = Instant::now();
    let err = run(&mut sh(&script), None, Duration::from_millis(500)).unwrap_err();
    let took = started.elapsed();
    assert_eq!(
        err,
        SubprocessError::TimedOut {
            after: Duration::from_millis(500),
            killed: true
        }
    );
    assert!(
        took < Duration::from_secs(10),
        "returned at the deadline, took {took:?}"
    );
    assert!(err.to_string().contains("timed out after 0.5s"), "{err}");
    let pid = std::fs::read_to_string(&pidfile).unwrap();
    assert!(
        gone_within(pid.trim(), Duration::from_secs(10)),
        "the grandchild {pid} survived the group kill"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `kill -0` fails once the process is reaped; a zombie (state Z, waiting
/// for init) counts as gone.
// concurrency: test: polls the process table for the reaper, bounded by a deadline
#[allow(clippy::disallowed_methods)]
fn gone_within(pid: &str, limit: Duration) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        let state = Command::new("ps")
            .args(["-o", "stat=", "-p", pid])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        if state.is_empty() || state.starts_with('Z') {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}
