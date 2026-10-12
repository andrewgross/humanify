//! The run manifest's commit read (`git rev-parse --short HEAD`): one
//! deadline over the whole child, killed and WARNED when it expires; a
//! checkout that is not a git repository stays the decided "unknown".

use std::process::Command;
use std::time::{Duration, Instant};

use super::{GIT_DEADLINE, short_sha};

fn sh(script: &str) -> Command {
    let mut c = Command::new("sh");
    c.args(["-c", script, "sh"]);
    c
}

#[test]
fn a_git_that_answers_gives_its_trimmed_sha() {
    assert_eq!(
        short_sha(&mut sh("echo abc1234"), GIT_DEADLINE),
        ("abc1234".to_string(), None)
    );
}

#[test]
fn not_a_git_checkout_is_the_quiet_unknown() {
    assert_eq!(
        short_sha(
            &mut sh("echo 'fatal: not a git repository' >&2; exit 128"),
            GIT_DEADLINE
        ),
        ("unknown".to_string(), None)
    );
}

#[test]
fn a_git_past_its_deadline_is_killed_and_warned() {
    let started = Instant::now();
    let (sha, warning) = short_sha(&mut sh("sleep 60"), Duration::from_millis(300));
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(sha, "unknown");
    let warning = warning.expect("a WARNING");
    assert!(warning.starts_with("WARNING: git rev-parse"), "{warning}");
    assert!(warning.contains("timed out after 0.3s"), "{warning}");
}

#[test]
fn the_production_deadline_is_one_minute() {
    assert_eq!(GIT_DEADLINE, Duration::from_secs(60));
}
