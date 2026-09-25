//! The library freeze's input for one processed file (findings #32/#33):
//! which functions of a mixed file are library code.
//!
//! The classification needs each function's RAW start, which a binary
//! ingesting the TS-beautified text (`--beautified-input`, phases 1–5a)
//! does not have. So while it does, the TS's own classification is
//! consumed: `--ts-library-functions <regions.json>` (a TS
//! `--dump-artifacts` dir's `regions.json`, whose `libraryFunctions` key
//! the #33 fix added). The owner is
//! `humanify_core::libdetect::function_carry::LibraryClassification`; at
//! 5b the Rust beautify's ordinal carry replaces the consumed rows and
//! this option is deleted with the other migration scaffolding.
//!
//! A file with banner regions and no classification is `Missing`: the
//! naming stage fails loud when it consults it (only when `skipLibraries`
//! is on and no wrapper IIFE was found — the TS throws the same).

use std::path::Path;

use humanify_core::libdetect::function_carry::LibraryClassification;
use humanify_core::libdetect::{CommentRegion, MixedFileDetection};
use serde_json::Value;

/// The classification for `file`: the TS's (when given) after proving its
/// banner regions equal the Rust's own for this file; `Missing` when the
/// file has regions and nothing classifies them; None otherwise.
pub fn library_classification(
    ts_regions: Option<&str>,
    file: &Path,
    mixed: &[(std::path::PathBuf, MixedFileDetection)],
) -> Result<Option<LibraryClassification>, String> {
    let regions: &[CommentRegion] = mixed
        .iter()
        .find(|(p, _)| p == file)
        .map_or(&[], |(_, m)| m.regions.as_slice());
    let Some(path) = ts_regions else {
        return Ok((!regions.is_empty()).then_some(LibraryClassification::Missing));
    };
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let theirs = ts_comment_regions(&v).map_err(|e| format!("{path}: {e}"))?;
    if theirs != regions {
        return Err(format!(
            "{path}: the TS classified banner regions {theirs:?}, the Rust found {regions:?} in {} — the classification is for another text",
            file.display()
        ));
    }
    LibraryClassification::from_regions_json(&v).map_err(|e| format!("{path}: {e}"))
}

/// regions.json `commentRegions` (spans in the minified text, UTF-8
/// bytes; an open end is null).
fn ts_comment_regions(v: &Value) -> Result<Vec<CommentRegion>, String> {
    v["commentRegions"]
        .as_array()
        .ok_or("commentRegions missing")?
        .iter()
        .map(|r| {
            Ok(CommentRegion {
                library_name: r["library"]
                    .as_str()
                    .ok_or("commentRegions library")?
                    .to_string(),
                start: r["span"]["start"]
                    .as_u64()
                    .ok_or("commentRegions span.start")? as usize,
                end: r["span"]["end"].as_u64().map(|e| e as usize),
            })
        })
        .collect()
}
