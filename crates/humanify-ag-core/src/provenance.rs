//! Which input bytes each emitted line came from (`.humanify/provenance.json`).

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::emit::cjs::FileLines;
use crate::unpack::bun::UnpackOrigins;
use crate::unpack::chunk_link::LinkMap;

pub const PROVENANCE_PATH: &str = ".humanify/provenance.json";

/// What the earlier stages recorded about one run's app file.
pub struct Sources<'a> {
    /// `None`: the input is one file, named `input_name`.
    pub link: Option<&'a LinkMap>,
    pub input_name: &'a str,
    /// `None`: the unpacked file is the input text.
    pub unpack: Option<&'a UnpackOrigins>,
    /// The formatted text and, per statement, `(offset in it, offset in
    /// the unpacked file)`.
    pub fresh: &'a str,
    pub origins: &'a [(u32, u32)],
    /// The named text the split cut, and its statements' spans.
    pub shipped: &'a str,
    pub spans: &'a [(u32, u32)],
}

/// The input file and byte a line starts from; `None` offset: written by
/// the linker for that file.
type Source = (u32, Option<u32>);

/// One row: from this line (1-based) to the next row, the source.
type Row = (u32, Option<Source>);

fn line_starts(text: &str) -> Vec<u32> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i as u32 + 1))
        .collect()
}

fn line_of(starts: &[u32], offset: u32) -> usize {
    starts.partition_point(|s| *s <= offset) - 1
}

impl Sources<'_> {
    fn input_of(&self, unpacked: u32) -> Option<Source> {
        let raw = self
            .unpack
            .map_or(unpacked, |u| u.runtime.raw_of(unpacked as usize) as u32);
        match self.link {
            Some(link) => link.source_of(raw),
            None => Some((0, Some(raw))),
        }
    }

    /// The first recorded source on each line of the formatted text.
    fn by_fresh_line(&self) -> Vec<Option<Source>> {
        let starts = line_starts(self.fresh);
        let mut out = vec![None; starts.len()];
        for &(at, unpacked) in self.origins {
            let slot = &mut out[line_of(&starts, at)];
            if slot.is_none() {
                *slot = self.input_of(unpacked);
            }
        }
        out
    }

    fn input_files(&self) -> Vec<String> {
        match self.link {
            Some(link) => link.files.clone(),
            None => vec![self.input_name.to_string()],
        }
    }
}

/// Why no provenance could be written.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProvenanceError {
    #[error("the named text has {shipped} lines, the formatted text {fresh}")]
    LinesDiffer { fresh: usize, shipped: usize },
}

/// One emitted file's rows. `final_lines` is the file's line count in the
/// written tree: later passes add lines above the first statement only.
fn file_rows(
    lines: &FileLines,
    final_lines: u32,
    by_line: &[Option<Source>],
    shipped_starts: &[u32],
    spans: &[(u32, u32)],
) -> Vec<Row> {
    let mut rows: Vec<Row> = vec![(1, None)];
    let mut at = 1 + lines.header_lines + final_lines.saturating_sub(lines.total());
    for &(statement, count) in &lines.statements {
        let (start, end) = spans[statement];
        let first = line_of(shipped_starts, start);
        let last = line_of(shipped_starts, end.saturating_sub(1).max(start));
        let same_shape = (last - first + 1) as u32 == count;
        let mut current = by_line[first..=last].iter().flatten().next().copied();
        for k in 0..count {
            let marked = by_line[first + k as usize].filter(|_| same_shape);
            if k == 0 || marked.is_some_and(|m| Some(m) != current) {
                current = marked.or(current);
                rows.pop_if(|last| last.0 == at + k);
                rows.push((at + k, current));
            }
        }
        at += count;
    }
    rows.dedup_by(|b, a| a.1 == b.1);
    rows
}

fn row_json(row: &Row) -> Value {
    match row.1 {
        Some((file, offset)) => json!([row.0, file, offset]),
        None => json!([row.0]),
    }
}

/// The provenance file's text. `line_count` gives a written file's lines.
pub fn provenance_json(
    sources: &Sources<'_>,
    emitted: &[FileLines],
    line_count: &dyn Fn(&str) -> u32,
) -> Result<String, ProvenanceError> {
    let shipped_starts = line_starts(sources.shipped);
    let by_line = sources.by_fresh_line();
    if by_line.len() != shipped_starts.len() {
        return Err(ProvenanceError::LinesDiffer {
            fresh: by_line.len(),
            shipped: shipped_starts.len(),
        });
    }
    let mut files: BTreeMap<&str, Value> = BTreeMap::new();
    for lines in emitted {
        let rows = file_rows(
            lines,
            line_count(&lines.path),
            &by_line,
            &shipped_starts,
            sources.spans,
        );
        files.insert(&lines.path, rows.iter().map(row_json).collect());
    }
    for ((start, _), path) in sources.unpack.iter().flat_map(|u| &u.vendor) {
        let source = match sources.link {
            Some(link) => link.source_of(*start),
            None => Some((0, Some(*start))),
        };
        files.insert(path, json!([row_json(&(1, source))]));
    }
    let doc = json!({
        "version": 1,
        "rows": "[first line, input file, byte offset]; a row runs to the next row; \
    [line] alone: written by the pipeline",
        "inputFiles": sources.input_files(),
        "files": files,
    });
    Ok(format!("{doc}\n"))
}

#[cfg(test)]
mod provenance_test;
