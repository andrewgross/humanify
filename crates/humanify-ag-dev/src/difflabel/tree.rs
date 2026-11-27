//! An output tree's text files and what its layout record says about
//! each line.

use std::collections::BTreeMap;
use std::path::Path;

use humanify_ag_core::layout_record::{FileEntry, LAYOUT_PATH, Layout, Row, Unit};

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

pub struct Tree {
    /// `None`: not a text file.
    files: BTreeMap<String, Option<TextFile>>,
    layout: Option<Layout>,
    rows: BTreeMap<String, Vec<(u32, Origin)>>,
    /// Per init function's name, the input file it is the init of.
    init_chunks: BTreeMap<String, u32>,
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

fn origin_of(unit: &Unit) -> Origin {
    match unit.at {
        None => Origin::Pipeline,
        Some((file, Some(byte))) => Origin::At(file, byte),
        Some((file, _)) => Origin::Linker(file),
    }
}

/// A line takes the last origin given for it; a row that repeats the
/// origin before it adds nothing.
fn push(rows: &mut Vec<(u32, Origin)>, line: u32, origin: Origin) {
    rows.pop_if(|last| last.0 == line);
    if rows.last().is_none_or(|last| last.1 != origin) {
        rows.push((line, origin));
    }
}

fn push_unit(rows: &mut Vec<(u32, Origin)>, unit: &Unit) {
    push(rows, unit.line, origin_of(unit));
    let file = unit.at.map_or(0, |at| at.0);
    for row in &unit.rows {
        if let Row::Origin { line, byte } = row {
            let origin = byte.map_or(Origin::Linker(file), |b| Origin::At(file, b));
            push(rows, *line, origin);
        }
    }
}

/// Each init function the linker wrote, with its input file.
fn init_chunks(layout: &Layout) -> BTreeMap<String, u32> {
    let units = layout.files.values().flat_map(FileEntry::all_units);
    let inits = units.filter(|u| u.linker.as_deref() == Some("init"));
    inits
        .filter_map(|u| Some((u.at?.0, &u.names)))
        .flat_map(|(file, names)| names.iter().map(move |n| (n.clone(), file)))
        .collect()
}

/// A file's rows: the pipeline's lines first, then each unit's.
fn rows_of(entry: &FileEntry) -> Vec<(u32, Origin)> {
    let mut rows = vec![(1, Origin::Pipeline)];
    entry
        .all_units()
        .for_each(|unit| push_unit(&mut rows, unit));
    rows
}

impl Tree {
    pub fn read(root: &Path) -> Result<Tree, String> {
        let files = humanify_ag_io::tree::read_tree(root, &|_| true).map_err(|e| e.to_string())?;
        let layout = files
            .get(LAYOUT_PATH)
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
        Tree::from_parts(files, layout.as_deref())
    }

    pub fn from_parts(
        files: BTreeMap<String, Vec<u8>>,
        layout: Option<&str>,
    ) -> Result<Tree, String> {
        let layout = layout
            .map(Layout::parse)
            .transpose()
            .map_err(|e| format!("{LAYOUT_PATH}: {e}"))?;
        let rows = layout.iter().flat_map(|l| &l.files);
        Ok(Tree {
            init_chunks: layout.as_ref().map(init_chunks).unwrap_or_default(),
            rows: rows.map(|(path, e)| (path.clone(), rows_of(e))).collect(),
            files: files.into_iter().map(|(p, b)| (p, text_file(b))).collect(),
            layout,
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

    fn inputs(&self) -> &[String] {
        self.layout.as_ref().map_or(&[], |l| l.inputs.as_slice())
    }

    pub fn input_name(&self, file: u32) -> &str {
        self.inputs().get(file as usize).map_or("", String::as_str)
    }

    pub fn input_index(&self, name: &str) -> Option<u32> {
        let at = self.inputs().iter().position(|n| n == name)?;
        Some(at as u32)
    }

    /// The input file an init function of this name is the init of.
    pub fn init_chunk(&self, name: &str) -> Option<u32> {
        self.init_chunks.get(name).copied()
    }

    /// Every line of a file with its origin.
    pub fn lines_with_origin<'a>(&'a self, path: &str) -> impl Iterator<Item = (u32, Origin)> + 'a {
        let rows = self.rows(path);
        let end = self.line_count(path).map_or(0, |n| n + 1);
        rows.iter()
            .enumerate()
            .flat_map(move |(i, (first, origin))| {
                let next = rows.get(i + 1).map_or(end, |r| r.0);
                (*first..next).map(move |line| (line, *origin))
            })
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

/// A layout whose files hold one unit per given row: `[line]` no input,
/// `[line, file, byte]`, `[line, file, null]` linker text.
#[cfg(test)]
pub fn layout_of_rows(inputs: &[&str], files: &str) -> String {
    use serde_json::{Value, json};
    let files: BTreeMap<String, Vec<Vec<Option<u32>>>> = serde_json::from_str(files).unwrap();
    let unit = |row: &Vec<Option<u32>>| match row.as_slice() {
        [line] => json!({"at": null, "line": line}),
        [line, file, None] => json!({"at": [file, null], "linker": "init", "line": line}),
        [line, file, byte] => json!({"at": [file, byte], "line": line}),
        other => panic!("{other:?}"),
    };
    let files: BTreeMap<String, Value> = files
        .iter()
        .map(|(path, rows)| {
            (
                path.clone(),
                json!({"units": rows.iter().map(unit).collect::<Vec<_>>()}),
            )
        })
        .collect();
    json!({
        "format": 1, "inputs": inputs, "priorLayout": null, "namedText": "",
        "files": files, "priorAmbiguous": [],
    })
    .to_string()
}

#[cfg(test)]
#[path = "tree_test.rs"]
mod tree_test;
