//! The io errors print the text the run prints (a WARNING line, a crash
//! line), so each `Display` is held to the words it had before it was
//! derived.

use std::path::PathBuf;
use std::time::Duration;

use crate::diff::DiffError;
use crate::scratch::ScratchError;
use crate::subprocess::SubprocessError;
use crate::tree::TreeError;

fn same(got: impl std::fmt::Display, want: &str) {
    assert_eq!(got.to_string(), want);
}

#[test]
fn io_errors_keep_their_words() {
    same(
        TreeError {
            path: PathBuf::from("prior/a.js"),
            source: std::io::Error::other("gone"),
        },
        "read prior/a.js: gone",
    );
    same(DiffError("write prior: full".into()), "write prior: full");
    same(
        ScratchError {
            prefix: "diff".into(),
        },
        "cannot create a diff temp dir",
    );
    same(
        SubprocessError::Spawn("not found".into()),
        "cannot start: not found",
    );
    same(
        SubprocessError::TimedOut {
            after: Duration::from_millis(1500),
            killed: true,
        },
        "timed out after 1.5s (killed)",
    );
    same(
        SubprocessError::TimedOut {
            after: Duration::from_secs(2),
            killed: false,
        },
        "timed out after 2s (the kill FAILED — the child may still be running)",
    );
    same(SubprocessError::Io("broken pipe".into()), "broken pipe");
}
