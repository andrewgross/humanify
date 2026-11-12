//! Stage 2's input side (exp216): the registry of INPUT LAYOUTS — how a
//! run's input file becomes the one program every later stage reads.
//!
//! The same shape as the unpack-adapter and library-detector registries:
//! each layout answers `supports(path, text)`, the first that does wins,
//! and the fallback, which supports everything, is registered last.
//!
//! - [`InputLayout::BunChunks`] — a Bun code-split layout (Claude Code
//!   from 2.1.242: an ESM loader importing `/$bunfs/root/<chunk>.js`
//!   siblings), linked into one program (`unpack::chunk_link`);
//! - [`InputLayout::SingleFile`] — everything else, returned
//!   byte-for-byte.
//!
//! It runs BEFORE detection, not as an unpack adapter: detection, the
//! library recipe, the run-time index, the split's markers and the unpack
//! adapter all read the input text, so a layout that is not one file must
//! become one first. [`read_input`] is the one owner that consults the
//! registry; every input reader (the pipeline, `detect`, `unpack`,
//! `discover`, `match`) goes through it.

use std::path::Path;

use super::chunk_link::{self, LinkReport};

/// The registered input layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputLayout {
    BunChunks,
    /// The fallback: the file is the program.
    SingleFile,
}

/// Registry order: the first layout whose `supports` holds wins; the
/// fallback is last because it supports everything.
pub const INPUT_LAYOUTS: [InputLayout; 2] = [InputLayout::BunChunks, InputLayout::SingleFile];

impl InputLayout {
    /// The layout's registered name (the stats file's `inputLink.layout`).
    pub fn name(self) -> &'static str {
        match self {
            InputLayout::BunChunks => "bun-chunks",
            InputLayout::SingleFile => "single-file",
        }
    }

    /// Does this layout read the file at `path`, whose text is `text`?
    pub fn supports(self, path: &Path, text: &str) -> bool {
        match self {
            InputLayout::BunChunks => chunk_link::is_chunked_entry(path, text),
            InputLayout::SingleFile => true,
        }
    }

    /// The program text this layout makes of the file, and the link
    /// report when it linked anything.
    fn read(self, path: &Path, text: String) -> Result<(String, Option<LinkReport>), String> {
        match self {
            InputLayout::BunChunks => {
                chunk_link::link(path).map(|(text, report)| (text, Some(report)))
            }
            InputLayout::SingleFile => Ok((text, None)),
        }
    }
}

/// The first registered layout that supports the file.
pub fn choose_input_layout(path: &Path, text: &str) -> InputLayout {
    INPUT_LAYOUTS
        .into_iter()
        .find(|l| l.supports(path, text))
        .unwrap_or(InputLayout::SingleFile)
}

/// A run's input: the text every stage reads, the layout that read it,
/// and the link report when the layout linked a multi-file input.
#[derive(Debug)]
pub struct InputBundle {
    pub text: String,
    pub layout: InputLayout,
    pub link: Option<LinkReport>,
}

/// Read a run's input file (invalid UTF-8 becomes U+FFFD) through the
/// layout registry.
pub fn read_input(path: &Path) -> Result<InputBundle, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    input_from_text(path, String::from_utf8_lossy(&bytes).into_owned())
}

/// [`read_input`] for a caller that read the file itself (and reports a
/// failed read its own way): `text` is the file at `path`.
pub fn input_from_text(path: &Path, text: String) -> Result<InputBundle, String> {
    let layout = choose_input_layout(path, &text);
    let (text, link) = layout.read(path, text)?;
    Ok(InputBundle { text, layout, link })
}

#[cfg(test)]
mod input_test;
