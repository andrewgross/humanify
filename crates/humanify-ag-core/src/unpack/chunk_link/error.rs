//! Why a code-split Bun layout could not be linked into one program. Each
//! variant prints the text the linker reported before the error was typed.

use std::path::PathBuf;

use crate::error::FileError;

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    /// A chunk file could not be read.
    #[error(transparent)]
    File(#[from] FileError),
    /// A bunfs specifier names a sibling file that does not exist.
    #[error("{spec} is imported but {} does not exist", path.display())]
    Missing { spec: String, path: PathBuf },
    /// A chunk does not parse as a module.
    #[error("{} does not parse as a module: {diagnostic}", path.display())]
    Parse { path: PathBuf, diagnostic: String },
    /// A construct of a chunk the linker refuses (the text names it).
    #[error("{}: {what}", path.display())]
    Refused { path: PathBuf, what: &'static str },
    /// `import.meta.<prop>` for a property the linker has no equivalent of.
    #[error("{}: `import.meta.{prop}` is not linked", path.display())]
    MetaProperty { path: PathBuf, prop: String },
    /// `import.meta.<prop>` where a name its equivalent reads is bound.
    #[error("{}: `import.meta.{prop}` sits where `{name}` is bound; not linked", path.display())]
    MetaShadowed {
        path: PathBuf,
        prop: String,
        name: &'static str,
    },
    /// An import names an export the chunk does not have.
    #[error("{} does not export `{name}`", path.display())]
    NoExport { path: PathBuf, name: String },
    /// A chunk re-exports from an external module.
    #[error("{}: re-exports from \"{spec}\"; not linked", path.display())]
    ReExportExternal { path: PathBuf, spec: String },
    /// Two edits of one chunk overlap.
    #[error("{}: overlapping edits at {at}", path.display())]
    OverlappingEdits { path: PathBuf, at: usize },
    /// An embedded file name that leaves the embedded root.
    #[error("/$bunfs/root/{rest} leaves the embedded root; not linked")]
    EmbeddedOutside { rest: String },
    /// An embedded file that is read but does not exist.
    #[error("/$bunfs/root/{rest} is read but {} does not exist", path.display())]
    EmbeddedMissing { rest: String, path: PathBuf },
    /// An embedded file required with import attributes.
    #[error(
        "{}: /$bunfs/root/{rest} is required with import attributes (Bun's sqlite_embedded loader); not linked",
        path.display()
    )]
    EmbeddedAttributes { path: PathBuf, rest: String },
    /// An embedded text file that decodes as neither UTF-8 nor UTF-16LE.
    #[error("/$bunfs/root/{rest} {why}")]
    EmbeddedText { rest: String, why: DecodeError },
    /// An embedded file path where `__dirname` is bound.
    #[error("{}: /$bunfs/root/{rest} sits where `__dirname` is bound; not linked", path.display())]
    DirnameBound { path: PathBuf, rest: String },
    /// A path literal could not be serialized.
    #[error(transparent)]
    Json(serde_json::Error),
}

/// Why an embedded text file's bytes are not text.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("is neither UTF-8 nor UTF-16LE (odd length)")]
    OddLength,
    #[error("is neither UTF-8 nor UTF-16LE ({0})")]
    Utf16(std::char::DecodeUtf16Error),
}
