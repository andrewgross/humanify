use std::time::{Duration, Instant};

use super::*;

#[test]
fn a_child_runs_with_its_stdin_cwd_and_env() {
    let mut spawn = Spawn::new("sh", &["-c", "cat; pwd; echo \"$DEV_PROBE\""], GIT_DEADLINE)
        .cwd(Path::new("/"))
        .env("DEV_PROBE", "seen");
    spawn.stdin = Some(b"in\n".to_vec());
    assert_eq!(run_ok(&spawn).unwrap(), "in\n/\nseen\n");
}

#[test]
fn a_failing_child_is_an_error_naming_it_and_its_stderr() {
    let spawn = Spawn::new("sh", &["-c", "echo boom >&2; exit 3"], GIT_DEADLINE);
    let err = run_ok(&spawn).unwrap_err();
    assert!(err.contains("sh -c") && err.contains("boom"), "{err}");
}

#[test]
fn the_deadline_kills_a_child_that_overruns_it() {
    let spawn = Spawn::new("sh", &["-c", "exec sleep 30"], Duration::from_millis(300));
    let started = Instant::now();
    let err = run(&spawn).unwrap_err();
    assert!(err.contains("timed out"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn git_answers_from_the_repo() {
    let root = crate::paths::repo_root();
    let top = git(&root, &["rev-parse", "--show-toplevel"]).unwrap();
    assert!(Path::new(&top).join("Cargo.toml").exists(), "{top}");
}

#[test]
fn a_streamed_child_reports_its_status_and_refuses_stdin() {
    let s = Spawn::new("sh", &["-c", "exit 4"], Duration::from_secs(30));
    assert_eq!(run_streamed(&s).unwrap().code(), Some(4));
    let mut with_input = s.clone();
    with_input.stdin = Some(b"x".to_vec());
    assert!(
        run_streamed(&with_input)
            .unwrap_err()
            .contains("takes no stdin")
    );
}
