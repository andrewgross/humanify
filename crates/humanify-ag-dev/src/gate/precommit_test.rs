use super::*;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn rust_files_get_rustfmt_and_the_staged_file_census() {
    let plan = plan(&s(&["crates/a/src/x.rs", "docs/y.md", "crates/b/src/z.rs"]));
    assert_eq!(plan.rustfmt, s(&["crates/a/src/x.rs", "crates/b/src/z.rs"]));
    assert_eq!(
        plan.census,
        Some(s(&["crates/a/src/x.rs", "crates/b/src/z.rs"]))
    );
    assert_eq!(
        plan.address_scan,
        s(&["crates/a/src/x.rs", "docs/y.md", "crates/b/src/z.rs"])
    );
}

#[test]
fn a_staged_allowlist_runs_the_whole_census() {
    let plan = plan(&s(&["dev/census/concurrency-allowlist.json"]));
    assert!(plan.rustfmt.is_empty());
    assert_eq!(plan.census, Some(Vec::new()), "empty = the whole tree");
}

#[test]
fn nothing_rust_staged_runs_no_census() {
    let plan = plan(&s(&["README.md"]));
    assert_eq!(plan.census, None);
    assert!(plan.rustfmt.is_empty());
}

#[test]
fn the_census_launch_names_the_files_or_none() {
    assert_eq!(
        census_args(&s(&["a.rs", "b.rs"])),
        s(&["census", "concurrency", "--files", "a.rs,b.rs"])
    );
    assert_eq!(census_args(&[]), s(&["census", "concurrency"]));
}

#[test]
fn the_committed_hook_runs_this_verb() {
    let hook = std::fs::read_to_string(crate::paths::repo_root().join(HOOK)).unwrap();
    assert!(hook.starts_with("#!/bin/sh\n"), "{hook}");
    assert!(hook.contains("cargo dev precommit"), "{hook}");
}
