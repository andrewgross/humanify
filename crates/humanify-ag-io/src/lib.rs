//! The shell's I/O library (functional core, imperative shell): the
//! child-process runner, the scratch-directory maker, the system `diff`
//! and the whole-folder reader (the prior release's tree). The pure core
//! (`humanify-ag-core`) does not link it (enforced: census:effects); it
//! receives what these produce through the traits it declares (`Differ`, the
//! webcrack `ExternalUnpacker`), and the shell (cli, index, the harness)
//! calls them.
#![forbid(unsafe_code)]

pub mod diff;
pub mod scratch;
pub mod subprocess;
pub mod tree;
