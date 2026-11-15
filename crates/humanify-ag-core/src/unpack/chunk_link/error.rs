//! Why a code-split Bun layout could not be linked into one program. Each
//! variant prints the text the linker reported before the error was typed.

use std::path::PathBuf;

use crate::error::FileError;

#[derive(Debug)]
pub enum LinkError {
    /// A chunk file could not be read.
    File(FileError),
    /// A bunfs specifier names a sibling file that does not exist.
    Missing { spec: String, path: PathBuf },
    /// A chunk does not parse as a module.
    Parse { path: PathBuf, diagnostic: String },
    /// A construct of a chunk the linker refuses (the text names it).
    Refused { path: PathBuf, what: &'static str },
    /// `import.meta.<prop>` for a property the linker has no equivalent of.
    MetaProperty { path: PathBuf, prop: String },
    /// `import.meta.<prop>` where a name its equivalent reads is bound.
    MetaShadowed {
        path: PathBuf,
        prop: String,
        name: &'static str,
    },
    /// An import names an export the chunk does not have.
    NoExport { path: PathBuf, name: String },
    /// A chunk re-exports from an external module.
    ReExportExternal { path: PathBuf, spec: String },
    /// Two edits of one chunk overlap.
    OverlappingEdits { path: PathBuf, at: usize },
    /// An embedded file name that leaves the embedded root.
    EmbeddedOutside { rest: String },
    /// An embedded file that is read but does not exist.
    EmbeddedMissing { rest: String, path: PathBuf },
    /// An embedded file required with import attributes.
    EmbeddedAttributes { path: PathBuf, rest: String },
    /// An embedded text file that decodes as neither UTF-8 nor UTF-16LE.
    EmbeddedText { rest: String, why: DecodeError },
    /// An embedded file path where `__dirname` is bound.
    DirnameBound { path: PathBuf, rest: String },
    /// A path literal could not be serialized.
    Json(serde_json::Error),
}

/// Why an embedded text file's bytes are not text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    OddLength,
    Utf16(std::char::DecodeUtf16Error),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OddLength => f.write_str("is neither UTF-8 nor UTF-16LE (odd length)"),
            Self::Utf16(e) => write!(f, "is neither UTF-8 nor UTF-16LE ({e})"),
        }
    }
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File(e) => e.fmt(f),
            Self::Missing { spec, path } => {
                write!(
                    f,
                    "{spec} is imported but {} does not exist",
                    path.display()
                )
            }
            Self::Parse { path, diagnostic } => write!(
                f,
                "{} does not parse as a module: {diagnostic}",
                path.display()
            ),
            Self::Refused { path, what } => write!(f, "{}: {what}", path.display()),
            Self::MetaProperty { path, prop } => {
                write!(f, "{}: `import.meta.{prop}` is not linked", path.display())
            }
            Self::MetaShadowed { path, prop, name } => write!(
                f,
                "{}: `import.meta.{prop}` sits where `{name}` is bound; not linked",
                path.display()
            ),
            Self::NoExport { path, name } => {
                write!(f, "{} does not export `{name}`", path.display())
            }
            Self::ReExportExternal { path, spec } => write!(
                f,
                "{}: re-exports from \"{spec}\"; not linked",
                path.display()
            ),
            Self::OverlappingEdits { path, at } => {
                write!(f, "{}: overlapping edits at {at}", path.display())
            }
            Self::EmbeddedOutside { rest } => {
                write!(
                    f,
                    "/$bunfs/root/{rest} leaves the embedded root; not linked"
                )
            }
            Self::EmbeddedMissing { rest, path } => write!(
                f,
                "/$bunfs/root/{rest} is read but {} does not exist",
                path.display()
            ),
            Self::EmbeddedAttributes { path, rest } => write!(
                f,
                "{}: /$bunfs/root/{rest} is required with import attributes (Bun's \
                 sqlite_embedded loader); not linked",
                path.display()
            ),
            Self::EmbeddedText { rest, why } => write!(f, "/$bunfs/root/{rest} {why}"),
            Self::DirnameBound { path, rest } => write!(
                f,
                "{}: /$bunfs/root/{rest} sits where `__dirname` is bound; not linked",
                path.display()
            ),
            Self::Json(e) => e.fmt(f),
        }
    }
}

impl From<FileError> for LinkError {
    fn from(e: FileError) -> Self {
        Self::File(e)
    }
}
