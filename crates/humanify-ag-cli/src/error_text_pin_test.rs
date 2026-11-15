//! The shell's typed errors print the text the sites printed when their
//! errors were strings: an error's text reaches the crash line, a verb's
//! stderr or a recorded file, so a changed word is a changed output. Each
//! expected string is the format string the site used, filled with the
//! same sample values.

use std::path::PathBuf;

use humanify_ag_core::error::{FileError, FileOp};
use humanify_ag_core::place::ledger::LedgerParseError;
use humanify_ag_core::prior_tree::PriorTreeError;

use crate::output_tree::WriteError;
use crate::prior_tree::PriorLoadError;

fn io(kind: std::io::ErrorKind, why: &str) -> std::io::Error {
    std::io::Error::new(kind, why.to_string())
}

fn same(got: impl std::fmt::Display, want: &str) {
    assert_eq!(got.to_string(), want);
}

#[test]
fn prior_load_errors_keep_their_words() {
    use std::io::ErrorKind::{NotFound, Other};
    same(
        PriorLoadError::PriorFile {
            path: PathBuf::from("p/a.js"),
            source: io(NotFound, "gone"),
        },
        "ENOENT: no such file or directory, open 'p/a.js'",
    );
    same(
        PriorLoadError::Tree(humanify_ag_io::tree::TreeError {
            path: PathBuf::from("p/x"),
            source: io(Other, "gone"),
        }),
        "cannot read the prior tree: read p/x: gone",
    );
    same(
        PriorLoadError::Refused {
            root: PathBuf::from("p"),
            error: PriorTreeError::VendorManifest { why: "bad".into() },
        },
        &format!(
            "prior vendor manifest p/{} is not one this binary reads (bad); \
             rebuild the prior release with this binary",
            humanify_ag_core::prior_tree::vendor_manifest_path()
        ),
    );
    same(
        PriorLoadError::Refused {
            root: PathBuf::from("p"),
            error: PriorTreeError::VendorNamingNotText { why: "bad".into() },
        },
        &format!(
            "p: {}",
            PriorTreeError::VendorNamingNotText { why: "bad".into() }
        ),
    );
    same(
        PriorLoadError::LedgerRead {
            path: PathBuf::from("p/l.json"),
            source: io(Other, "gone"),
        },
        "p/l.json: gone",
    );
    same(
        PriorLoadError::Ledger {
            path: PathBuf::from("p/l.json"),
            error: LedgerParseError::UnsupportedVersion,
        },
        "Unsupported split ledger version in p/l.json",
    );
    same(
        PriorLoadError::Ledger {
            path: PathBuf::from("p/l.json"),
            error: LedgerParseError::Json("not JSON".into()),
        },
        "p/l.json: not JSON",
    );
}

#[test]
fn output_tree_write_errors_keep_their_words() {
    same(
        WriteError::File(FileError::at(FileOp::Write, &PathBuf::from("o/a.js"))(io(
            std::io::ErrorKind::Other,
            "full",
        ))),
        "write o/a.js: full",
    );
    same(
        WriteError::EmbeddedCopy {
            from: PathBuf::from("in/db"),
            to: PathBuf::from("o/db"),
            source: io(std::io::ErrorKind::Other, "full"),
        },
        "copy the embedded file in/db to o/db: full",
    );
}

#[test]
fn verb_errors_keep_their_words() {
    use crate::match_verb::VerbError;
    let gone = || io(std::io::ErrorKind::Other, "gone");
    same(
        VerbError::Read("p.js".into(), gone()),
        "cannot read p.js: gone",
    );
    same(
        VerbError::EmptyPrior("p.js".into()),
        "--prior-version file is empty: p.js",
    );
    same(
        VerbError::Override {
            what: "bundler".into(),
            name: "vite".into(),
            valid: "bun, webpack".into(),
        },
        "unknown bundler \"vite\" (valid: bun, webpack)",
    );
    same(VerbError::Mkdir("w".into(), gone()), "mkdir w: gone");
    same(
        VerbError::Write("o.json".into(), gone()),
        "write o.json: gone",
    );
    same(VerbError::Stdout(gone()), "stdout: gone");
    same(
        VerbError::NothingMatched,
        "every unpacked file failed; no matching ran at all — see the recorded parse and \
         match errors",
    );
    same(
        VerbError::Settings(vec!["a: bad".into(), "b: bad".into()]),
        "a: bad\nb: bad",
    );
    let unpack = humanify_ag_core::unpack::UnpackError::NoShim;
    let unpack_text = unpack.to_string();
    same(VerbError::Unpack(unpack), &unpack_text);
}

#[test]
fn setting_value_errors_keep_their_words() {
    use crate::registry::ValueError;
    same(
        ValueError::Setting {
            raw: "8x".into(),
            who: "jobs".into(),
            what: "a non-negative integer is expected",
        },
        "invalid value '8x' for 'jobs': a non-negative integer is expected",
    );
    same(
        ValueError::Flag {
            raw: "8x".into(),
            who: "--jobs <N>".into(),
            what: "a number is expected",
        },
        "error: invalid value '8x' for '--jobs <N>': a number is expected\n",
    );
    use crate::config::file::TomlValueError;
    same(
        TomlValueError::Kind {
            key: "llm.model".into(),
            kind: "a string",
            got: "3".into(),
        },
        "llm.model must be a string, got 3",
    );
    same(
        TomlValueError::Negative {
            key: "llm.retries".into(),
            n: -1,
        },
        "llm.retries = -1 is negative",
    );
    same(
        crate::kill_switches::SwitchError {
            what: "probe",
            name: "x".into(),
            valid: "a, b".into(),
        },
        "switches.probe: unknown probe switch \"x\" — valid: a, b",
    );
}

#[test]
fn library_index_errors_keep_their_words() {
    use crate::config::{IndexDirError, IndexStaleError, StaleWhy};
    same(
        IndexDirError::NoSuchId {
            given: "ab".into(),
            store: "s".into(),
        },
        "library index ab: no such index under library.index_store (s)",
    );
    same(
        IndexDirError::NeedsStore("ab".into()),
        "library index ab: an index id needs library.index_store (or pass the index \
         directory)",
    );
    same(
        IndexDirError::NoSuchDir("x".into()),
        "library index x: no such directory",
    );
    let stale = |why| IndexStaleError {
        dir: "i".into(),
        why,
    };
    same(
        stale(StaleWhy::Unreadable(io(std::io::ErrorKind::Other, "gone"))),
        "library index i: manifest.json unreadable: gone",
    );
    let json = serde_json::from_str::<u8>("x").unwrap_err();
    same(
        stale(StaleWhy::NotAManifest(json)),
        &format!(
            "library index i: manifest.json is not an index manifest: {}",
            serde_json::from_str::<u8>("x").unwrap_err()
        ),
    );
    let stamps = humanify_ag_core::libindex::format::ManifestError::Stamps(vec!["f".into()]);
    let stamps_text = stamps.to_string();
    same(
        stale(StaleWhy::Stamps(stamps)),
        &format!("library index i: {stamps_text}"),
    );
}

#[test]
fn run_errors_keep_their_words() {
    same(
        crate::util::NodeFsError {
            source: io(std::io::ErrorKind::NotFound, "gone"),
            syscall: "open",
            path: "run.log".into(),
        },
        "ENOENT: no such file or directory, open 'run.log'",
    );
    let split = humanify_ag_core::emit::stable_split::SplitError::LibraryNeedsFossil {
        regime: humanify_ag_core::place::placement_dump::Regime::Cluster,
    };
    let split_text = split.to_string();
    same(
        crate::split_stage::SplitFailed(split),
        &format!("stable split failed before any tree was written: {split_text}"),
    );
}
