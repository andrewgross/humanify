//! The prior release read once: what the loader finds in a prior folder,
//! which ledger is in force, and that a prior which is there but cannot be
//! read fails the run instead of reading as "no prior".

use std::path::{Path, PathBuf};

use humanify_ag_testkit::ScratchDir;

use super::{PriorLoadError, load_prior};

/// A ledger this binary reads.
fn ledger_text(file: &str) -> String {
    format!(
        r#"{{"version":1,"files":["{file}"],"nameToFiles":{{}},"order":["{file}"],"hashVersion":{}}}"#,
        humanify_ag_core::hash::forms::STATEMENT.version
    )
}

fn write(root: &Path, rel: &str, text: &str) -> PathBuf {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

/// A prior output folder: the humanified file, its ledger, an app file,
/// a vendor manifest and naming source, and an installed package and a
/// hidden folder the loader skips.
fn prior_folder(dir: &ScratchDir) -> PathBuf {
    let root = dir.path();
    write(root, "src/app.js", "app");
    write(
        root,
        "vendor/_bun-modules.json",
        &format!(
            r#"{{"adapter":"bun","hashVersion":{},"factories":[]}}"#,
            humanify_ag_core::hash::forms::MATCH_KEY.version
        ),
    );
    write(root, "node_modules/x/index.js", "installed");
    write(root, ".git/HEAD", "ref");
    write(
        root,
        ".humanify/split-ledger.json",
        &ledger_text("src/app.js"),
    );
    write(root, ".humanify/vendor-humanified.js", "joined");
    write(root, ".humanify/humanified.js", "prior code")
}

#[test]
fn a_prior_folder_is_read_once_into_values() {
    let dir = ScratchDir::new("prior-load-folder");
    let prior = prior_folder(&dir);
    let inputs = load_prior(Some(&prior), None).unwrap();
    let tree = inputs.tree.expect("a prior tree");
    assert_eq!(tree.code, "prior code");
    assert_eq!(tree.files.text("src/app.js"), Some("app"));
    assert!(!tree.files.contains("node_modules/x/index.js"));
    assert!(!tree.files.contains(".git/HEAD"));
    assert_eq!(tree.vendor_naming.as_deref(), Some("joined"));
    assert!(tree.vendor.is_some(), "the vendor manifest is read");
    let ledger = inputs.ledger.expect("the ledger beside the prior");
    assert_eq!(ledger.path, dir.path().join(".humanify/split-ledger.json"));
    assert_eq!(ledger.ledger.files, vec!["src/app.js".to_string()]);
    // The values outlive the folder: a run writing into the prior folder
    // (output folder = prior folder) cannot change what it read.
    std::fs::remove_dir_all(dir.path()).unwrap();
    assert_eq!(tree.files.text("src/app.js"), Some("app"));
}

#[test]
fn no_prior_version_is_the_only_absent_prior() {
    let inputs = load_prior(None, None).unwrap();
    assert!(inputs.tree.is_none() && inputs.ledger.is_none());
}

#[test]
fn split_ledger_wins_over_the_ledger_beside_the_prior() {
    let dir = ScratchDir::new("prior-load-override");
    let prior = prior_folder(&dir);
    // The beside ledger is broken: with the override it is not read.
    write(dir.path(), ".humanify/split-ledger.json", "not json");
    let given = write(
        dir.path(),
        "elsewhere/ledger.json",
        &ledger_text("src/given.js"),
    );
    let inputs = load_prior(Some(&prior), Some(&given)).unwrap();
    let ledger = inputs.ledger.expect("the given ledger");
    assert_eq!(ledger.path, given);
    assert_eq!(ledger.ledger.files, vec!["src/given.js".to_string()]);
    // A ledger with no prior is still read.
    let alone = load_prior(None, Some(&given)).unwrap();
    assert!(alone.tree.is_none());
    assert_eq!(alone.ledger.unwrap().path, given);
}

#[test]
fn a_broken_ledger_beside_the_prior_fails_with_its_path() {
    let dir = ScratchDir::new("prior-load-bad-ledger");
    let prior = prior_folder(&dir);
    let path = write(dir.path(), ".humanify/split-ledger.json", "not json");
    let err = load_prior(Some(&prior), None).expect_err("a broken ledger is not absent");
    assert!(matches!(err, PriorLoadError::Ledger { .. }), "{err}");
    assert!(
        err.to_string()
            .starts_with(&format!("{}: ", path.display())),
        "{err}"
    );
    write(
        dir.path(),
        ".humanify/split-ledger.json",
        &ledger_text("src/app.js").replace(r#""version":1"#, r#""version":9"#),
    );
    let err = load_prior(Some(&prior), None).expect_err("another ledger version");
    assert_eq!(
        err.to_string(),
        format!("Unsupported split ledger version in {}", path.display())
    );
}

#[test]
fn a_vendor_manifest_this_binary_cannot_read_fails_with_its_path() {
    let dir = ScratchDir::new("prior-load-bad-manifest");
    let prior = prior_folder(&dir);
    let manifest = write(
        dir.path(),
        "vendor/_bun-modules.json",
        r#"{"factories":[]}"#,
    );
    let err = load_prior(Some(&prior), None).expect_err("refused");
    let text = err.to_string();
    assert!(
        text.starts_with(&format!(
            "prior vendor manifest {} is not one this binary reads (",
            manifest.display()
        )),
        "{text}"
    );
    assert!(text.contains("hashVersion"), "{text}");
}

#[test]
fn a_missing_prior_file_fails_as_the_read_does() {
    let dir = ScratchDir::new("prior-load-missing");
    let prior = dir.path().join(".humanify/humanified.js");
    let err = load_prior(Some(&prior), None).expect_err("no prior file");
    assert_eq!(
        err.to_string(),
        format!(
            "ENOENT: no such file or directory, open '{}'",
            prior.display()
        )
    );
}

#[test]
fn a_prior_file_among_the_tree_files_stays_readable_by_path() {
    let dir = ScratchDir::new("prior-load-in-tree");
    let prior = write(dir.path(), "index.js", "entry");
    let tree = load_prior(Some(&prior), None).unwrap().tree.unwrap();
    assert_eq!(tree.code, "entry");
    assert_eq!(tree.files.text("index.js"), Some("entry"));
}
