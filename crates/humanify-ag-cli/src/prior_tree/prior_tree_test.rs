//! The prior release read once: what the loader finds in a prior folder,
//! the split's memory its layout record holds, and that a prior which is there but cannot be
//! read fails the run instead of reading as "no prior".

use std::path::{Path, PathBuf};

use humanify_ag_testkit::ScratchDir;

use super::{PriorLoadError, load_prior};

/// A layout record that holds one split statement in `file`.
fn layout_text(file: &str) -> String {
    format!(
        r#"{{"format":1,"inputs":["in.js"],"priorLayout":null,"namedText":".humanify/humanified.js","files":{{"{file}":{{"units":[{{"at":[0,0],"line":1,"n":0,"hash":"aa","names":["a"]}}]}}}},"priorAmbiguous":[],"hashVersion":{}}}"#,
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
    write(root, ".humanify/layout.json", &layout_text("src/app.js"));
    write(root, ".humanify/vendor-humanified.js", "joined");
    write(root, ".humanify/humanified.js", "prior code")
}

#[test]
fn a_prior_folder_is_read_once_into_values() {
    let dir = ScratchDir::new("prior-load-folder");
    let prior = prior_folder(&dir);
    let inputs = load_prior(Some(&prior)).unwrap();
    let tree = inputs.tree.expect("a prior tree");
    assert_eq!(tree.code, "prior code");
    assert_eq!(tree.files.text("src/app.js"), Some("app"));
    assert!(!tree.files.contains("node_modules/x/index.js"));
    assert!(!tree.files.contains(".git/HEAD"));
    assert_eq!(tree.vendor_naming.as_deref(), Some("joined"));
    assert!(tree.vendor.is_some(), "the vendor manifest is read");
    let ledger = inputs.ledger.expect("the ledger the layout record holds");
    assert_eq!(ledger.path, dir.path().join(".humanify/layout.json"));
    assert_eq!(ledger.ledger.files, vec!["src/app.js".to_string()]);
    assert_eq!(ledger.ledger.name_to_files["a"], ["src/app.js"]);
    // The values outlive the folder: a run writing into the prior folder
    // (output folder = prior folder) cannot change what it read.
    std::fs::remove_dir_all(dir.path()).unwrap();
    assert_eq!(tree.files.text("src/app.js"), Some("app"));
}

#[test]
fn no_prior_version_is_the_only_absent_prior() {
    let inputs = load_prior(None).unwrap();
    assert!(inputs.tree.is_none() && inputs.ledger.is_none());
}

#[test]
fn a_layout_record_that_does_not_read_fails_with_its_path() {
    let dir = ScratchDir::new("prior-load-bad-layout");
    let prior = prior_folder(&dir);
    let path = write(dir.path(), ".humanify/layout.json", "not json");
    let err = load_prior(Some(&prior)).expect_err("a broken layout record is not absent");
    assert!(matches!(err, PriorLoadError::Layout { .. }), "{err}");
    let head = format!("prior layout record {} was not read: ", path.display());
    assert!(err.to_string().starts_with(&head), "{err}");
}

#[test]
fn a_tree_with_a_ledger_file_and_no_layout_record_is_refused() {
    let dir = ScratchDir::new("prior-load-old-tree");
    let prior = prior_folder(&dir);
    std::fs::remove_file(dir.path().join(".humanify/layout.json")).unwrap();
    write(dir.path(), ".humanify/split-ledger.json", "{}");
    let err = load_prior(Some(&prior)).expect_err("a tree of an older binary");
    assert!(
        err.to_string()
            .ends_with("rebuild the prior release with this binary"),
        "{err}"
    );
}

#[test]
fn a_tree_that_was_not_split_holds_no_ledger() {
    let dir = ScratchDir::new("prior-load-unsplit");
    let prior = prior_folder(&dir);
    std::fs::remove_file(dir.path().join(".humanify/layout.json")).unwrap();
    assert!(load_prior(Some(&prior)).unwrap().ledger.is_none());
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
    let err = load_prior(Some(&prior)).expect_err("refused");
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
    let err = load_prior(Some(&prior)).expect_err("no prior file");
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
    let tree = load_prior(Some(&prior)).unwrap().tree.unwrap();
    assert_eq!(tree.code, "entry");
    assert_eq!(tree.files.text("index.js"), Some("entry"));
}
