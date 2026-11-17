//! An output tree's text files and what its provenance file says about
//! each line.

use std::collections::BTreeMap;
use std::path::Path;

use humanify_ag_core::provenance::PROVENANCE_PATH;
use serde::Deserialize;

/// Where a line of an output file came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Written by the pipeline.
    Pipeline,
    /// Written by the chunk linker for this input file.
    Linker(u32),
    /// This input file, from this byte.
    At(u32, u32),
}

struct TextFile {
    text: String,
    /// Each line's first byte.
    starts: Vec<u32>,
}

#[derive(Deserialize)]
struct ProvenanceFile {
    #[serde(rename = "inputFiles")]
    input_files: Vec<String>,
    files: BTreeMap<String, Vec<Vec<Option<u32>>>>,
}

pub struct Tree {
    /// `None`: not a text file.
    files: BTreeMap<String, Option<TextFile>>,
    input_files: Vec<String>,
    rows: BTreeMap<String, Vec<(u32, Origin)>>,
}

fn text_file(bytes: Vec<u8>) -> Option<TextFile> {
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8(bytes)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    let mut starts: Vec<u32> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i as u32 + 1))
        .collect();
    if starts.last().is_some_and(|s| *s as usize == text.len()) {
        starts.pop();
    }
    Some(TextFile { text, starts })
}

fn row(raw: &[Option<u32>]) -> Result<(u32, Origin), String> {
    match raw {
        [Some(line)] => Ok((*line, Origin::Pipeline)),
        [Some(line), Some(file), None] => Ok((*line, Origin::Linker(*file))),
        [Some(line), Some(file), Some(offset)] => Ok((*line, Origin::At(*file, *offset))),
        other => Err(format!(
            "a provenance row this reader does not know: {other:?}"
        )),
    }
}

impl Tree {
    pub fn read(root: &Path) -> Result<Tree, String> {
        let mut files =
            humanify_ag_io::tree::read_tree(root, &|_| true).map_err(|e| e.to_string())?;
        let provenance = files
            .remove(PROVENANCE_PATH)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
        Tree::from_parts(files, provenance.as_deref())
    }

    pub fn from_parts(
        files: BTreeMap<String, Vec<u8>>,
        provenance: Option<&str>,
    ) -> Result<Tree, String> {
        let parsed: Option<ProvenanceFile> = provenance
            .map(serde_json::from_str)
            .transpose()
            .map_err(|e| format!("{PROVENANCE_PATH}: {e}"))?;
        let (input_files, raw_rows) = parsed.map(|p| (p.input_files, p.files)).unwrap_or_default();
        let mut rows = BTreeMap::new();
        for (path, raw) in raw_rows {
            let parsed: Result<Vec<_>, _> = raw.iter().map(|r| row(r)).collect();
            rows.insert(path, parsed?);
        }
        Ok(Tree {
            files: files.into_iter().map(|(p, b)| (p, text_file(b))).collect(),
            input_files,
            rows,
        })
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    pub fn has(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    pub fn text(&self, path: &str) -> Option<&str> {
        self.files.get(path)?.as_ref().map(|f| f.text.as_str())
    }

    /// A text file's line count.
    pub fn line_count(&self, path: &str) -> Option<u32> {
        self.files
            .get(path)?
            .as_ref()
            .map(|f| f.starts.len() as u32)
    }

    /// A line's text (1-based), without its line end.
    pub fn line(&self, path: &str, line: u32) -> Option<&str> {
        let file = self.files.get(path)?.as_ref()?;
        let start = *file.starts.get(line.checked_sub(1)? as usize)? as usize;
        let rest = &file.text[start..];
        Some(rest.split('\n').next().unwrap_or(rest))
    }

    pub fn input_name(&self, file: u32) -> &str {
        self.input_files
            .get(file as usize)
            .map_or("", String::as_str)
    }

    /// A file's rows: `(first line, origin)`, each running to the next.
    pub fn rows(&self, path: &str) -> &[(u32, Origin)] {
        self.rows.get(path).map_or(&[], Vec::as_slice)
    }

    pub fn files_with_rows(&self) -> impl Iterator<Item = &str> {
        self.rows.keys().map(String::as_str)
    }

    pub fn origin(&self, path: &str, line: u32) -> Option<Origin> {
        let rows = self.rows(path);
        let after = rows.partition_point(|r| r.0 <= line);
        Some(rows.get(after.checked_sub(1)?)?.1)
    }
}

#[cfg(test)]
#[path = "tree_test.rs"]
mod tree_test;
