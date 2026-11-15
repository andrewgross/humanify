//! The adapter's decisions over what the shell's shim run returned: the
//! contract line, and that every reported file lies inside the directory
//! the shim wrote into. The process itself is the shell's (cli
//! `webcrack`, whose tests drive real children).

use std::path::{Path, PathBuf};

use super::{ExternalUnpacker, ShimRun, UnpackerError, unpack_webcrack};

/// A shim run that already happened: it wrote into `dir` and printed
/// `stdout` (or failed with `error`).
struct Ran {
    dir: PathBuf,
    stdout: String,
    error: Option<String>,
}

impl ExternalUnpacker for Ran {
    fn run(&self, _code: &str, _out_dir: &Path) -> Result<ShimRun, UnpackerError> {
        match &self.error {
            Some(e) => Err(UnpackerError(e.clone())),
            None => Ok(ShimRun {
                out_dir: self.dir.clone(),
                stdout: self.stdout.clone(),
            }),
        }
    }
}

fn ran(stdout: &str) -> Ran {
    Ran {
        dir: PathBuf::from("/run/out"),
        stdout: stdout.to_string(),
        error: None,
    }
}

#[test]
fn files_inside_the_shims_directory_are_returned_in_order() {
    let shim = ran(
        "a dependency's log line\n{\"files\":[{\"path\":\"/run/out/b.js\"},{\"path\":\"/run/out/a.js\"}]}\n",
    );
    let unpacked = unpack_webcrack("x", Path::new("out"), &shim).expect("parses");
    let paths: Vec<_> = unpacked.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(
        paths,
        [
            PathBuf::from("/run/out/b.js"),
            PathBuf::from("/run/out/a.js")
        ]
    );
}

/// The shim's file list is trusted only inside the out dir: a reported
/// path elsewhere (a webcrack that wrote beside it) is an error naming
/// it, never a file the run reads.
#[test]
fn a_shim_file_outside_the_out_dir_is_refused() {
    let shim = ran("{\"files\":[{\"path\":\"/etc/passwd\"}]}\n");
    let err = unpack_webcrack("x", Path::new("out"), &shim).expect_err("outside the out dir");
    assert!(err.to_string().contains("/etc/passwd"), "{err}");
    assert!(err.to_string().contains("/run/out"), "{err}");
}

#[test]
fn a_failed_shim_run_is_the_unpacks_error_word_for_word() {
    let shim = Ran {
        dir: PathBuf::new(),
        stdout: String::new(),
        error: Some("webcrack shim exited exit status: 3: boom".to_string()),
    };
    let err = unpack_webcrack("x", Path::new("out"), &shim).expect_err("failed run");
    assert_eq!(err.to_string(), "webcrack shim exited exit status: 3: boom");
}

#[test]
fn a_shim_that_printed_nothing_is_an_error() {
    let err = unpack_webcrack("x", Path::new("out"), &ran("\n  \n")).expect_err("no output");
    assert_eq!(err.to_string(), "webcrack shim: no output");
}
