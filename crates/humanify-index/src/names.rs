//! The names pass (exp159): a minified build's unit prints get the
//! package's ORIGINAL identifier names, recovered from the build's own
//! sourcemap and the package's installed sources. The mechanism is
//! positional, never a hash join (the census measured the minifier's
//! structural rewrites at a ~20% hash-alignment cap —
//! `experiments/159-restore-data/README.md`): Bun anchors every statement
//! and declarator of the minified output to the original line it came
//! from, so a declared identifier anchors to an original LINE, and the
//! original file's identifiers on that line pair with it in order.
//!
//! Builder-only: the pipeline never has a sourcemap.

use std::collections::HashMap;
use std::path::Path;

use humanify_core::ingest::Ingest;
use humanify_core::libindex::names::{
    OrigDecl, RestoredName, declared_decls, line_of, line_starts, names_for_print,
};
use oxc_span::Span;

use crate::sourcemap::SourceMap;

/// The sourcemap as the join reads it: every segment's generated position
/// as a byte offset of the bundle, with its source and original line.
/// Sorted by offset (the decode order is (line, column) order).
pub struct Anchors {
    segments: Vec<(usize, u32, u32)>,
    line_starts: Vec<u32>,
}

impl Anchors {
    /// The bundle's map (`segments` are `(line, utf16 column, source,
    /// original line)` — `humanify_index::sourcemap::decode_mappings`).
    pub fn new(code: &str, map: &SourceMap) -> Anchors {
        let positions: Vec<(u32, u32)> = map.segments.iter().map(|&(l, c, _, _)| (l, c)).collect();
        let mut at: Vec<(usize, u32, u32)> =
            crate::sourcemap::utf16_to_byte_offsets(code, &positions)
                .into_iter()
                .zip(&map.segments)
                .filter_map(|(o, &(_, _, s, ol))| o.map(|o| (o, s, ol)))
                .collect();
        at.sort_unstable();
        Anchors {
            segments: at,
            line_starts: line_starts(code),
        }
    }

    /// The `(source, original line)` a bundle byte position anchors to —
    /// the last segment at or before it ON ITS LINE. None when the line
    /// has no segment before it (nothing anchors the position).
    pub fn at(&self, at: u32) -> Option<(u32, u32)> {
        let i = self
            .segments
            .partition_point(|&(o, _, _)| o <= usize::try_from(at).unwrap_or(usize::MAX));
        let (offset, src, line) = *self.segments.get(i.checked_sub(1)?)?;
        let line_start = usize::try_from(
            *self
                .line_starts
                .get(line_of(&self.line_starts, at) as usize)?,
        )
        .ok()?;
        (offset >= line_start).then_some((src, line))
    }
}

/// The package's ORIGINAL sources as the join's other side, by source
/// index: each attributed source file parsed once, its declared
/// identifiers grouped by line. Builder-only (the files live in the
/// install's `node_modules`).
pub fn original_decls(out: &Path, map: &SourceMap, wanted: &[u32]) -> HashMap<u32, Vec<OrigDecl>> {
    let mut out_map: HashMap<u32, Vec<OrigDecl>> = HashMap::new();
    for &src in wanted {
        let path = crate::sourcemap::lexical(&out.join(&map.sources[src as usize]));
        let Some(code) = std::fs::read_to_string(&path).ok() else {
            continue;
        };
        let allocator = oxc_allocator::Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, &code);
        if !ingest.errors.is_empty() {
            continue;
        }
        out_map.insert(src, declared_decls(&code, ingest.semantic()));
    }
    out_map
}

/// One unit print's restored names (see
/// `humanify_core::libindex::names::names_for_print`). `mapping` is the
/// print's canonical slot mapping, `unit` its span, `attributed` the
/// source the unit's segments credited.
pub fn print_names(
    mapping: &humanify_core::libindex::units::Mapping,
    unit: Span,
    attributed: u32,
    semantic: &oxc_semantic::Semantic<'_>,
    anchors: &Anchors,
    orig: &HashMap<u32, Vec<OrigDecl>>,
) -> Vec<RestoredName> {
    let scoping = semantic.scoping();
    let empty: Vec<OrigDecl> = Vec::new();
    names_for_print(
        mapping,
        unit,
        attributed,
        |s| scoping.symbol_span(s),
        |at| anchors.at(at).filter(|&(src, _)| src == attributed),
        orig.get(&attributed).unwrap_or(&empty),
    )
}
