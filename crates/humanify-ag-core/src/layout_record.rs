//! Where each piece of the output came from and what it was in the
//! previous version (`.humanify/layout.json`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::place::ledger::FossilLedgerModule;

pub mod ledger_view;
pub mod write;

pub const LAYOUT_PATH: &str = ".humanify/layout.json";

/// The one format this binary reads and writes.
pub const FORMAT: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub format: u32,
    /// The run's input files; the first number of every `at`.
    pub inputs: Vec<String>,
    /// The checksum of the previous tree's layout.
    pub prior_layout: Option<String>,
    /// The file that holds the whole named text.
    pub named_text: String,
    pub files: BTreeMap<String, FileEntry>,
    /// Previous functions the matcher saw several candidates for.
    pub prior_ambiguous: Vec<(u32, FunctionId)>,
    /// The version of the statement fingerprint the units' `hash` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_version: Option<u64>,
    /// The split's modules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modules: Option<Vec<FossilLedgerModule>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// The file's statements in written order; the lines above the first
    /// are the pipeline's.
    pub units: Vec<Unit>,
}

/// One statement.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    /// The input file and the byte it starts at; no byte: the linker
    /// wrote it for that file. `None`: no input text is known for it.
    pub at: Option<(u32, Option<u32>)>,
    /// The kind of linker text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linker: Option<String>,
    /// Of a `call`: the input file whose startup function it calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<u32>,
    /// Of a `load`: which of the file's loads it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nth: Option<u32>,
    /// Its first line in the output file.
    pub line: u32,
    /// Of a statement the split placed: its position in the named text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Of a statement the split placed: its fingerprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    /// The names it declares, as written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Row>,
    /// Of a statement with no input text of its own (a wrapper around
    /// the program, a startup function): the statements inside it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub body: Vec<Unit>,
}

/// A mark inside a unit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "RowList", from = "RowList")]
pub enum Row {
    /// From this line on the text came from this byte of the unit's
    /// input file; no byte: the linker wrote it.
    Origin { line: u32, byte: Option<u32> },
    /// A function starts on this line.
    Function {
        line: u32,
        id: FunctionId,
        matched: Match,
    },
}

/// A function within its input file: the byte it starts at, or for one
/// the linker wrote, the linker text it is in and its offset there.
/// `Unknown`: no input file holds it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FunctionId {
    Byte(u32),
    Linker(String),
    Unknown,
}

/// What the function matcher decided for a function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Match {
    New,
    /// Paired with this function of the previous layout (`None`: the
    /// previous tree had no layout).
    Exact(Option<(u32, FunctionId)>),
    Close(Option<(u32, FunctionId)>),
}

/// The checksum a layout names its previous layout's text by.
pub fn checksum(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(text.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// A row as the file spells it: a short list.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum RowList {
    Origin(u32, Option<u32>),
    New(u32, FunctionId, NewWord),
    Paired(u32, FunctionId, PairedWord, Option<u32>, FunctionId),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum NewWord {
    New,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PairedWord {
    Exact,
    Close,
}

impl From<Row> for RowList {
    fn from(row: Row) -> RowList {
        let (line, id, matched) = match row {
            Row::Origin { line, byte } => return RowList::Origin(line, byte),
            Row::Function { line, id, matched } => (line, id, matched),
        };
        let (word, partner) = match matched {
            Match::New => return RowList::New(line, id, NewWord::New),
            Match::Exact(partner) => (PairedWord::Exact, partner),
            Match::Close(partner) => (PairedWord::Close, partner),
        };
        let (input, partner) = partner.unzip();
        RowList::Paired(
            line,
            id,
            word,
            input,
            partner.unwrap_or(FunctionId::Unknown),
        )
    }
}

impl From<RowList> for Row {
    fn from(list: RowList) -> Row {
        match list {
            RowList::Origin(line, byte) => Row::Origin { line, byte },
            RowList::New(line, id, NewWord::New) => Row::Function {
                line,
                id,
                matched: Match::New,
            },
            RowList::Paired(line, id, word, input, partner) => {
                let partner = input.map(|input| (input, partner));
                let matched = match word {
                    PairedWord::Exact => Match::Exact(partner),
                    PairedWord::Close => Match::Close(partner),
                };
                Row::Function { line, id, matched }
            }
        }
    }
}

/// A function of a file, as its row and the unit around it give it.
#[derive(Clone, Copy, Debug)]
pub struct Function<'a> {
    pub input: u32,
    pub id: &'a FunctionId,
    pub matched: &'a Match,
    pub line: u32,
}

impl Unit {
    /// The unit and every unit inside it, in written order.
    pub fn with_inner(&self) -> Vec<&Unit> {
        let mut out = vec![self];
        for inner in &self.body {
            out.extend(inner.with_inner());
        }
        out
    }

    fn own_functions(&self) -> impl Iterator<Item = Function<'_>> + '_ {
        let input = self.at.map_or(0, |at| at.0);
        self.rows.iter().filter_map(move |row| match row {
            Row::Function { line, id, matched } => Some(Function {
                input,
                id,
                matched,
                line: *line,
            }),
            Row::Origin { .. } => None,
        })
    }
}

impl FileEntry {
    /// Every unit of the file, the ones inside others included, in
    /// written order.
    pub fn all_units(&self) -> impl Iterator<Item = &Unit> + '_ {
        self.units.iter().flat_map(Unit::with_inner)
    }

    /// Every function of the file in written order.
    pub fn functions(&self) -> impl Iterator<Item = Function<'_>> + '_ {
        self.all_units().flat_map(Unit::own_functions)
    }
}

impl Layout {
    /// The file's text: one line, keys in a fixed order.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string(self).expect("a layout serializes");
        text.push('\n');
        text
    }

    /// Read a layout; a format this binary does not write is refused.
    pub fn parse(text: &str) -> Result<Layout, LayoutParseError> {
        #[derive(Deserialize)]
        struct Head {
            format: u32,
        }
        let json = |e: serde_json::Error| LayoutParseError::Json(e.to_string());
        let head: Head = serde_json::from_str(text).map_err(json)?;
        if head.format != FORMAT {
            return Err(LayoutParseError::Format { found: head.format });
        }
        serde_json::from_str(text).map_err(json)
    }
}

/// Why a layout file was not read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LayoutParseError {
    #[error("layout format {found}: this binary reads format {FORMAT}")]
    Format { found: u32 },
    #[error("{0}")]
    Json(String),
}

#[cfg(test)]
#[path = "layout_record/layout_record_test.rs"]
mod layout_record_test;
