use super::*;
use crate::paths::repo_root;
use crate::proc::Spawn;

#[test]
fn the_pipeline_crate_is_read_from_its_manifest() {
    let toml = "[package]\nname = \"humanify-ag-cli\" # the CLI\n\n[[bin]]\nname = \"other\"\npath = \"src/other.rs\"\n\n[[bin]]\nname = \"humanify-ag\"\npath = \"src/main.rs\"\n\n[dependencies]\nname = \"not-this\"\n";
    assert_eq!(
        pipeline_crate_from_manifest(toml),
        Some(PipelineCrate {
            package: "humanify-ag-cli".into(),
            binary: "humanify-ag".into()
        })
    );
    assert_eq!(
        pipeline_crate_from_manifest("[package]\nname = \"x\"\n"),
        None
    );
}

#[test]
fn heads_pipeline_crate_is_humanify_ag() {
    let c = pipeline_crate_at(&repo_root()).unwrap();
    assert_eq!(
        (c.package.as_str(), c.binary.as_str()),
        ("humanify-ag-cli", "humanify-ag")
    );
}

/// The crate is read from the COMMIT: before exp186 it was humanify-cli /
/// humanify; before the cutover there was none (refused).
#[test]
fn the_pipeline_crate_follows_the_commit_across_the_rename() {
    let repo = repo_root();
    let before_rename = git(&repo, &["rev-parse", "f8bbb20ae^1"]).unwrap();
    let c = pipeline_crate_at_commit(&repo, &before_rename).unwrap();
    assert_eq!(
        (c.package.as_str(), c.binary.as_str()),
        ("humanify-cli", "humanify")
    );
    let after = pipeline_crate_at_commit(&repo, "f8bbb20ae").unwrap();
    assert_eq!(after.binary, "humanify-ag");
    let err = pipeline_crate_at_commit(&repo, "m4^{commit}~400").unwrap_err();
    assert!(err.contains("before the cutover"), "{err}");
}

#[test]
fn a_binary_without_config_in_its_help_is_a_legacy_leg() {
    let dir = std::env::temp_dir().join(format!("dev-legacy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // The scripts are written by a CHILD process: a file this process
    // writes can still be open for writing in a sibling test's fork when
    // it is executed (ETXTBSY, "Text file busy").
    let write = |name: &str, help: &str| {
        let p = dir.join(name);
        let script = format!(
            "printf '#!/bin/sh\\necho %s\\n' '{help}' > '{0}' && chmod 755 '{0}'",
            p.display()
        );
        crate::proc::run_ok(&Spawn::new(
            "sh",
            &["-c", &script],
            crate::proc::GIT_DEADLINE,
        ))
        .unwrap();
        p
    };
    assert!(is_legacy_leg(&write("old", "--split --llm-cache")).unwrap());
    assert!(!is_legacy_leg(&write("new", "--config FILE")).unwrap());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_binary_is_described_by_sha_commit_and_dirt() {
    let dir = std::env::temp_dir().join(format!("dev-describe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("humanify-ag");
    std::fs::write(&bin, b"abc").unwrap();
    let r = describe(&repo_root(), &bin).unwrap();
    assert_eq!(
        r.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(r.commit.len(), 40);
    assert_eq!(r.name, "humanify-ag");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn commit_spellings_match_by_prefix_and_unknown_matches_nothing() {
    assert!(same_commit("0357696ed", "0357696eddd2fc4a"));
    assert!(!same_commit("", "0357696e"));
    assert!(!same_commit("abc", "abd"));
}
