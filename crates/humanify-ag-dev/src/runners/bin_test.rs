use std::path::Path;

use super::*;

fn bin(commit: &str, dirty: bool) -> LabelBin {
    LabelBin {
        path: "/x/target/release/humanify-ag".into(),
        sha256: "s".into(),
        commit: commit.into(),
        dirty,
        built: true,
    }
}

#[test]
fn a_binary_from_another_commit_an_unknown_one_or_a_dirty_tree_is_refused() {
    assert!(
        bin_commit_refusal(&bin("", false), "abc1234", false)
            .unwrap()
            .contains("unknown")
    );
    assert!(
        bin_commit_refusal(&bin("abc1234def", true), "abc1234", false)
            .unwrap()
            .contains("DIRTY")
    );
    assert!(
        bin_commit_refusal(&bin("fff0000", false), "abc1234", false)
            .unwrap()
            .contains("this label records abc1234")
    );
    assert_eq!(
        bin_commit_refusal(&bin("abc1234def", false), "abc1234", false),
        None
    );
    // --force-mixed turns every refusal off
    assert_eq!(bin_commit_refusal(&bin("", false), "abc1234", true), None);
}

#[test]
fn only_a_binary_in_a_cargo_workspaces_target_dir_has_a_workspace() {
    let root = crate::paths::repo_root();
    let inside = root.join("target/release/humanify-ag");
    assert_eq!(
        workspace_root_of(&inside).map(|p| p.canonicalize().unwrap()),
        Some(root.canonicalize().unwrap())
    );
    assert_eq!(workspace_root_of(Path::new("/tmp/shims/humanify")), None);
}

#[test]
fn a_ref_from_before_the_cutover_is_refused() {
    let rec = crate::runners::recording_test::Rec::new("bin-ref");
    let ctx = rec.ctx(&crate::paths::repo_root());
    // the first commit of the repo has no pipeline crate
    let first = crate::proc::git(
        &crate::paths::repo_root(),
        &["rev-list", "--max-parents=0", "HEAD"],
    )
    .unwrap();
    let first = first.lines().next().unwrap();
    let e = build_ref(
        &ctx,
        &crate::paths::repo_root(),
        first,
        &rec.root.join("builds"),
    )
    .unwrap_err();
    assert!(e.contains("before the cutover"), "{e}");
    assert!(
        !rec.root.join("builds").exists(),
        "nothing is checked out for a refused ref"
    );
}

#[test]
fn a_ref_is_built_in_its_own_worktree_with_its_own_crate_name() {
    use crate::runners::recording_test::{Commit, cargo_toml, sandbox_repo};
    let rec = crate::runners::recording_test::Rec::new("bin-ref-build");
    let repo = sandbox_repo(
        &rec,
        &[
            Commit {
                files: vec![(
                    "crates/humanify-cli/Cargo.toml",
                    cargo_toml("humanify-cli", "humanify"),
                )],
                branch: Some("old"),
            },
            Commit {
                files: vec![(
                    "crates/humanify-ag-cli/Cargo.toml",
                    cargo_toml("humanify-ag-cli", "humanify-ag"),
                )],
                branch: None,
            },
        ],
    );
    let ctx = rec.ctx(&repo);
    let builds = rec.root.join("builds");
    let old = build_ref(&ctx, &repo, "old", &builds).unwrap();
    assert_eq!(
        old.name, "humanify",
        "the crate name is read from THAT commit"
    );
    let sha = crate::proc::git(&repo, &["rev-parse", "old"]).unwrap();
    assert_eq!(old.commit, sha);
    assert!(
        old.path.starts_with(builds.join(&sha[..12])),
        "{}",
        old.path.display()
    );
    let head = build_ref(&ctx, &repo, "HEAD", &builds).unwrap();
    assert_eq!(head.name, "humanify-ag");
    let calls = rec.normalise(&rec.calls(), &repo);
    assert!(
        calls.contains("cargo :: build --release --locked -p humanify-cli (in <TMP>/builds/"),
        "{calls}"
    );
    assert!(
        calls.contains("cargo :: build --release --locked -p humanify-ag-cli (in <TMP>/builds/"),
        "{calls}"
    );
    // a rebuild of the same commit reuses its worktree
    assert_eq!(
        build_ref(&ctx, &repo, "old", &builds).unwrap().path,
        old.path
    );
}
