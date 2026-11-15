//! The golden is the retired Python leaderboard's output on a label pair kept as a test
//! fixture, recorded from the Python (the oracle) before the port.

use super::*;

fn results() -> std::path::PathBuf {
    crate::paths::repo_root().join(crate::paths::LIBRARY_LABEL_FIXTURES)
}

fn labels(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_text_is_leaderboard_py_s() {
    let pair = ["bun-dev-exp150-main", "bun-dev-exp153-ix4-base"];
    let got = leaderboard(&results(), true, &labels(&pair), false).unwrap();
    assert_eq!(got, include_str!("leaderboard-bun-dev.golden.txt"));
}

#[test]
fn labels_of_two_corpora_are_refused_unless_forced() {
    let pair = labels(&["bun-dev-exp150-main", "apps-exp127-off"]);
    let err = leaderboard(&results(), true, &pair, false).unwrap_err();
    assert!(err.contains("corpus.name"), "{err}");
    let shown = leaderboard(&results(), true, &pair, true).unwrap();
    assert!(shown.starts_with("!! MIXED (LibraryLeaderboard"), "{shown}");
}
