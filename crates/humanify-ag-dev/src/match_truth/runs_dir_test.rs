//! A run without `--out` takes the first free numbered directory, and
//! takes it by creating it: two runs started together never share one.

use super::fresh_out_dir;

#[test]
fn two_runs_started_together_get_two_directories() {
    let root = std::env::temp_dir().join(format!(
        "match-truth-runs-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let a = fresh_out_dir(&root).unwrap();
    let b = fresh_out_dir(&root).unwrap();
    assert_ne!(a, b);
    assert!(a.is_dir() && b.is_dir(), "each is created when picked");
    assert_eq!(a.file_name().unwrap(), "run-0001");
    assert_eq!(b.file_name().unwrap(), "run-0002");
}
