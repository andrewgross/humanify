//! The label of one changed line.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use super::truth::Side;
use super::{Origin, Standing, Tree, Truth};

/// Why a line is in the diff. Every changed line gets exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Label {
    InputChanged,
    InputDuplicate,
    InputOutsideUnits,
    MovedFile,
    MovedInFile,
    Renamed,
    Reshaped,
    NotInOtherTree,
    Require,
    Export,
    PipelineWritten,
    LinkerWritten,
    NoProvenance,
}

impl Label {
    pub const ALL: [Label; 13] = [
        Label::InputChanged,
        Label::InputDuplicate,
        Label::InputOutsideUnits,
        Label::MovedFile,
        Label::MovedInFile,
        Label::Renamed,
        Label::Reshaped,
        Label::NotInOtherTree,
        Label::Require,
        Label::Export,
        Label::PipelineWritten,
        Label::LinkerWritten,
        Label::NoProvenance,
    ];

    /// `(name, group, what it means)`.
    pub fn describe(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Label::InputChanged => (
                "input-changed",
                "real",
                "from input code that changed, is new, or was removed",
            ),
            Label::InputDuplicate => (
                "input-duplicate",
                "unknown",
                "from input code whose text occurs more than once, so it has no certain pair",
            ),
            Label::InputOutsideUnits => (
                "input-outside-units",
                "unknown",
                "from an input import or export statement",
            ),
            Label::MovedFile => (
                "moved-file",
                "noise",
                "unchanged input, same text, now in another file",
            ),
            Label::MovedInFile => (
                "moved-in-file",
                "noise",
                "unchanged input, same text, same file, another position",
            ),
            Label::Renamed => (
                "renamed",
                "noise",
                "unchanged input, only identifiers differ",
            ),
            Label::Reshaped => (
                "reshaped",
                "noise",
                "unchanged input, the pipeline printed other code for it",
            ),
            Label::NotInOtherTree => (
                "not-in-other-tree",
                "noise",
                "unchanged input the other tree has no located lines for",
            ),
            Label::Require => (
                "require-line",
                "wiring",
                "a require line the pipeline wrote",
            ),
            Label::Export => ("export-line", "wiring", "an export line the pipeline wrote"),
            Label::PipelineWritten => (
                "pipeline-written",
                "wiring",
                "other text the pipeline wrote (headers, glue)",
            ),
            Label::LinkerWritten => (
                "linker-written",
                "wiring",
                "text the chunk linker wrote in place of imports and exports",
            ),
            Label::NoProvenance => (
                "no-provenance",
                "unknown",
                "a file or line the provenance file does not cover",
            ),
        }
    }
}

/// One output line of an unchanged unit.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct UnitLine<'a> {
    offset: u32,
    path: &'a str,
    line: u32,
}

type UnitLines<'a> = BTreeMap<u32, Vec<UnitLine<'a>>>;

/// A unit's lines in one tree, for a unit the two trees print on
/// different numbers of lines.
struct LineSet<'a> {
    exact: BTreeSet<(&'a str, &'a str)>,
    masked: BTreeSet<String>,
}

pub struct Labeller<'a> {
    truth: &'a Truth,
    from: &'a Tree,
    to: &'a Tree,
    /// Per side (from, to), per pair.
    unit_lines: [UnitLines<'a>; 2],
    sets: RefCell<BTreeMap<(bool, u32), LineSet<'a>>>,
}

/// The line with every identifier-like word replaced by `$`.
fn masked(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_word = false;
    let mut word_is_name = false;
    for c in line.chars() {
        let word_char = c.is_alphanumeric() || c == '_' || c == '$';
        if word_char && !in_word {
            word_is_name = !c.is_ascii_digit();
            if word_is_name {
                out.push('$');
            }
        }
        if !word_char || !word_is_name {
            out.push(c);
        }
        in_word = word_char;
    }
    out
}

fn wiring(text: &str) -> Label {
    let text = text.trim_start();
    let declares = ["const ", "var ", "let "]
        .iter()
        .any(|k| text.starts_with(k));
    let exports = [
        "module.exports",
        "exports.",
        "Object.defineProperty(module.exports",
        "Object.defineProperty(exports",
    ];
    if declares && text.contains("= require(") {
        Label::Require
    } else if exports.iter().any(|e| text.starts_with(e)) {
        Label::Export
    } else {
        Label::PipelineWritten
    }
}

fn unit_lines<'a>(tree: &'a Tree, side: &Side) -> UnitLines<'a> {
    let mut out = UnitLines::new();
    for path in tree.files_with_rows() {
        let rows = tree.rows(path);
        let end = tree.line_count(path).map_or(0, |n| n + 1);
        for (i, (first, origin)) in rows.iter().enumerate() {
            let Origin::At(file, offset) = *origin else {
                continue;
            };
            let Standing::Unchanged(pair) = side.at(tree.input_name(file), offset) else {
                continue;
            };
            let next = rows.get(i + 1).map_or(end, |r| r.0);
            out.entry(pair)
                .or_default()
                .extend((*first..next).map(|line| UnitLine { offset, path, line }));
        }
    }
    out.values_mut().for_each(|lines| lines.sort_unstable());
    out
}

fn compare(mine: &str, theirs: &str, same_path: bool) -> Label {
    if mine == theirs {
        if same_path {
            Label::MovedInFile
        } else {
            Label::MovedFile
        }
    } else if masked(mine) == masked(theirs) {
        Label::Renamed
    } else {
        Label::Reshaped
    }
}

impl<'a> Labeller<'a> {
    pub fn new(truth: &'a Truth, from: &'a Tree, to: &'a Tree) -> Labeller<'a> {
        Labeller {
            truth,
            from,
            to,
            unit_lines: [unit_lines(from, &truth.from), unit_lines(to, &truth.to)],
            sets: RefCell::new(BTreeMap::new()),
        }
    }

    fn tree(&self, to_side: bool) -> &'a Tree {
        if to_side { self.to } else { self.from }
    }

    fn text(&self, to_side: bool, at: &UnitLine<'a>) -> &'a str {
        self.tree(to_side).line(at.path, at.line).unwrap_or("")
    }

    fn against_set(
        &self,
        to_side: bool,
        pair: u32,
        theirs: &[UnitLine<'a>],
        path: &str,
        text: &str,
    ) -> Label {
        let mut sets = self.sets.borrow_mut();
        let set = sets.entry((to_side, pair)).or_insert_with(|| LineSet {
            exact: theirs
                .iter()
                .map(|l| (self.text(!to_side, l), l.path))
                .collect(),
            masked: theirs
                .iter()
                .map(|l| masked(self.text(!to_side, l)))
                .collect(),
        });
        if set.exact.contains(&(text, path)) {
            Label::MovedInFile
        } else if set
            .exact
            .range((text, "")..)
            .next()
            .is_some_and(|(t, _)| *t == text)
        {
            Label::MovedFile
        } else if set.masked.contains(&masked(text)) {
            Label::Renamed
        } else {
            Label::Reshaped
        }
    }

    fn unchanged(&self, to_side: bool, pair: u32, at: UnitLine<'a>) -> Label {
        let side = usize::from(to_side);
        let (Some(mine), Some(theirs)) = (
            self.unit_lines[side].get(&pair),
            self.unit_lines[1 - side].get(&pair),
        ) else {
            return Label::NotInOtherTree;
        };
        let text = self.text(to_side, &at);
        if mine.len() != theirs.len() {
            return self.against_set(to_side, pair, theirs, at.path, text);
        }
        match mine.binary_search(&at) {
            Ok(i) => compare(
                text,
                self.text(!to_side, &theirs[i]),
                theirs[i].path == at.path,
            ),
            Err(_) => Label::NoProvenance,
        }
    }

    /// The label of one line (1-based) of a file of one tree.
    pub fn label(&self, to_side: bool, path: &'a str, line: u32) -> Label {
        let tree = self.tree(to_side);
        let side = if to_side {
            &self.truth.to
        } else {
            &self.truth.from
        };
        match tree.origin(path, line) {
            None => Label::NoProvenance,
            Some(Origin::Pipeline) => wiring(tree.line(path, line).unwrap_or("")),
            Some(Origin::Linker(_)) => Label::LinkerWritten,
            Some(Origin::At(file, offset)) => match side.at(tree.input_name(file), offset) {
                Standing::Unchanged(pair) => {
                    self.unchanged(to_side, pair, UnitLine { offset, path, line })
                }
                Standing::Duplicate => Label::InputDuplicate,
                Standing::Changed => Label::InputChanged,
                Standing::OutsideUnits => Label::InputOutsideUnits,
                Standing::UnknownFile => Label::NoProvenance,
            },
        }
    }
}

#[cfg(test)]
#[path = "label_test.rs"]
mod label_test;
