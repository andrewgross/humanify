//! A live run's claim on the folder it writes: a second run on the same
//! folder is REFUSED at once (never queued), naming the run that holds it;
//! the claim ends with the run, and a marker a dead run left behind (or a
//! reused pid) does not block anyone.

use super::claim::{claim, claim_folder, marker_of};

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "claim-test-{name}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn the_marker_sits_beside_the_folder_never_inside_it() {
    assert_eq!(
        marker_of(std::path::Path::new("/w/neutrality-cache")),
        std::path::PathBuf::from("/w/neutrality-cache.live-run")
    );
    assert_eq!(
        marker_of(std::path::Path::new("/w/walk/")),
        std::path::PathBuf::from("/w/walk.live-run")
    );
}

#[test]
fn a_second_run_on_a_claimed_folder_is_refused_naming_the_first() {
    let d = tmp("second");
    let folder = d.join("store");
    let first = claim_folder(&folder, "index build spec-a").unwrap();
    let err = claim_folder(&folder, "index build spec-b").unwrap_err();
    assert!(err.contains(&folder.display().to_string()), "{err}");
    assert!(
        err.contains("index build spec-a"),
        "names the holder: {err}"
    );
    assert!(
        err.contains(&format!("pid {}", std::process::id())),
        "{err}"
    );
    drop(first);
    let again = claim_folder(&folder, "index build spec-b");
    assert!(again.is_ok(), "the claim ends with its run: {again:?}");
}

#[test]
fn a_marker_left_by_a_dead_run_blocks_nobody() {
    let d = tmp("stale");
    let marker = d.join("work.live-run");
    // above Linux's pid_max ceiling: no such process
    std::fs::write(&marker, "pid 4194399 start 1 — a run that crashed\n").unwrap();
    let c = claim(&marker, &d.join("work"), "eval walk");
    assert!(c.is_ok(), "{c:?}");
    let text = std::fs::read_to_string(&marker).unwrap();
    assert!(text.contains("eval walk"), "{text}");
    assert!(
        !text.contains("crashed"),
        "the stale holder is overwritten: {text}"
    );
}

#[test]
fn a_claim_does_not_create_the_folder_it_guards() {
    let d = tmp("nocreate");
    let folder = d.join("cache");
    let _c = claim_folder(&folder, "eval neutrality").unwrap();
    assert!(!folder.exists(), "the run creates its own folder");
}

#[test]
fn a_reused_pid_is_not_mistaken_for_the_holder() {
    let d = tmp("reused");
    let marker = d.join("cache.live-run");
    // this process's pid, but not its start time: an older run's pid reused
    let stale = format!("pid {} start 0 — an older run\n", std::process::id());
    std::fs::write(&marker, stale).unwrap();
    let c = claim(&marker, &d.join("cache"), "eval neutrality");
    assert!(c.is_ok(), "{c:?}");
}

#[test]
fn dropping_a_claim_removes_its_marker() {
    let d = tmp("drop");
    let folder = d.join("walk");
    let c = claim_folder(&folder, "eval walk").unwrap();
    assert!(marker_of(&folder).is_file());
    drop(c);
    assert!(!marker_of(&folder).exists());
}
