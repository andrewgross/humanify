//! The label of one changed line.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use super::truth::Side;
use super::{Origin, Standing, Tree, Truth};

/// Why a line is in the diff. Every changed line gets exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Label {
    InputChanged,
    InputReindented,
    KeptRenamed,
    KeptMoved,
    InputDuplicate,
    InputOutsideUnits,
    MovedFile,
    MovedInFile,
    Renamed,
    Reshaped,
    NotInOtherTree,
    RequireMoved,
    RequireRenamed,
    RequireEdge,
    Export,
    PipelineWritten,
    LinkerMoved,
    ImportsReordered,
    LinkerRenamed,
    LinkerChanged,
    LinkerUnpaired,
    NoOrigin,
}

impl Label {
    pub const ALL: [Label; 22] = [
        Label::InputChanged,
        Label::InputReindented,
        Label::KeptRenamed,
        Label::KeptMoved,
        Label::InputDuplicate,
        Label::InputOutsideUnits,
        Label::MovedFile,
        Label::MovedInFile,
        Label::Renamed,
        Label::Reshaped,
        Label::NotInOtherTree,
        Label::RequireMoved,
        Label::RequireRenamed,
        Label::RequireEdge,
        Label::Export,
        Label::PipelineWritten,
        Label::LinkerMoved,
        Label::ImportsReordered,
        Label::LinkerRenamed,
        Label::LinkerChanged,
        Label::LinkerUnpaired,
        Label::NoOrigin,
    ];

    /// `(name, group, what it means)`.
    pub fn describe(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Label::InputChanged => (
                "input-changed",
                "real",
                "from input code that changed, is new, or was removed, with no like line on the other side",
            ),
            Label::InputReindented => (
                "input-reindented",
                "real",
                "from changed input; the other side has the same text at another indentation",
            ),
            Label::KeptRenamed => (
                "kept-renamed",
                "kept",
                "from changed input; the other side has the line in the same hunk, only identifiers differ",
            ),
            Label::KeptMoved => (
                "kept-moved",
                "kept",
                "from changed input; the other side has the line in another hunk, identifiers aside",
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
            Label::RequireMoved => (
                "require-moved",
                "noise",
                "a require line the file has on both sides, at another position",
            ),
            Label::RequireRenamed => (
                "require-renamed",
                "noise",
                "a require of the same module under another local name",
            ),
            Label::RequireEdge => (
                "require-edge",
                "wiring",
                "a require of a module the file does not require on the other side",
            ),
            Label::Export => ("export-line", "wiring", "an export line the pipeline wrote"),
            Label::PipelineWritten => (
                "pipeline-written",
                "wiring",
                "other text the pipeline wrote (headers, glue)",
            ),
            Label::LinkerMoved => (
                "linker-moved",
                "noise",
                "linker text the paired input file also has, at another position",
            ),
            Label::ImportsReordered => (
                "imports-reordered",
                "noise",
                "a linker init call the paired input file also has, at another position: the bundler wrote the same imports in another order",
            ),
            Label::LinkerRenamed => (
                "linker-renamed",
                "noise",
                "linker text the paired input file has with other identifiers",
            ),
            Label::LinkerChanged => (
                "linker-changed",
                "real",
                "linker text the paired input file has no counterpart for: its imports or declarations changed",
            ),
            Label::LinkerUnpaired => (
                "linker-unpaired",
                "unknown",
                "linker text of an input file that has no paired file in the other version",
            ),
            Label::NoOrigin => (
                "no-origin",
                "unknown",
                "a file or line the layout record does not cover",
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
    /// Per side, per input file.
    linker_lines: [BTreeMap<u32, Vec<Place<'a>>>; 2],
    /// Per side and group (an input file's linker text, an output file's
    /// require lines): how each of its lines is found on the other side.
    wiring: RefCell<BTreeMap<(bool, String), BTreeMap<Place<'a>, Found>>>,
}

/// The line with every identifier-like word replaced by `$`.
pub(super) fn masked(line: &str) -> String {
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

fn masked_owned(line: &str) -> String {
    masked(line)
}

/// The other tree's require lines of a path it may not have.
fn require_lines_in<'a>(tree: &'a Tree, path: &str) -> Vec<Place<'a>> {
    match tree.files_with_rows().find(|p| *p == path) {
        Some(own) => require_lines(tree, own),
        None => Vec::new(),
    }
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
        Label::RequireEdge
    } else if exports.iter().any(|e| text.starts_with(e)) {
        Label::Export
    } else {
        Label::PipelineWritten
    }
}

/// A statement that only calls a name with no arguments, as the linker
/// writes a chunk's init: `name();` or `(0, module.name)();`.
fn is_init_call(text: &str) -> bool {
    let Some(callee) = text.trim().strip_suffix("();") else {
        return false;
    };
    let callee = callee
        .strip_prefix("(0, ")
        .and_then(|c| c.strip_suffix(')'))
        .unwrap_or(callee);
    !callee.is_empty()
        && callee
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
}

fn unit_lines<'a>(tree: &'a Tree, side: &Side) -> UnitLines<'a> {
    let mut out = UnitLines::new();
    for path in tree.files_with_rows() {
        for (line, origin) in tree.lines_with_origin(path) {
            let Origin::At(file, offset) = origin else {
                continue;
            };
            if let Standing::Unchanged(pair) = side.at(tree.input_name(file), offset) {
                out.entry(pair)
                    .or_default()
                    .push(UnitLine { offset, path, line });
            }
        }
    }
    out.values_mut().for_each(|lines| lines.sort_unstable());
    out
}

type Place<'a> = (&'a str, u32);

/// Per input file, the lines the linker wrote for it.
fn linker_lines(tree: &Tree) -> BTreeMap<u32, Vec<Place<'_>>> {
    let mut out: BTreeMap<u32, Vec<Place<'_>>> = BTreeMap::new();
    for path in tree.files_with_rows() {
        for (line, origin) in tree.lines_with_origin(path) {
            if let Origin::Linker(file) = origin {
                out.entry(file).or_default().push((path, line));
            }
        }
    }
    out
}

/// A file's require lines.
fn require_lines<'a>(tree: &'a Tree, path: &'a str) -> Vec<Place<'a>> {
    tree.lines_with_origin(path)
        .filter(|(line, origin)| {
            let text = tree.line(path, *line).unwrap_or("");
            *origin == Origin::Pipeline && wiring(text) == Label::RequireEdge
        })
        .map(|(line, _)| (path, line))
        .collect()
}

/// What a require line loads.
fn require_target(text: &str) -> String {
    text.split_once("require(")
        .map_or("", |(_, rest)| rest)
        .to_string()
}

/// How a line of one side is found on the other.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Found {
    SameText,
    SameKey,
    Missing,
}

fn counts<'t>(items: impl Iterator<Item = &'t str>) -> BTreeMap<&'t str, u32> {
    let mut out = BTreeMap::new();
    for item in items {
        *out.entry(item).or_default() += 1;
    }
    out
}

/// Per line of `mine` (`(text, key)`): whether the other side has a line
/// of that text left for it, else one of that key.
pub(super) fn match_lines(mine: &[(&str, String)], theirs: &[(&str, String)]) -> Vec<Found> {
    let their_texts = counts(theirs.iter().map(|l| l.0));
    let my_texts = counts(mine.iter().map(|l| l.0));
    let unmatched = |lines: &[(&str, String)], other: &BTreeMap<&str, u32>| -> Vec<bool> {
        let mut seen: BTreeMap<&str, u32> = BTreeMap::new();
        let mut left = Vec::with_capacity(lines.len());
        for line in lines {
            let n = seen.entry(line.0).or_default();
            *n += 1;
            left.push(*n > other.get(line.0).copied().unwrap_or(0));
        }
        left
    };
    let (mine_left, theirs_left) = (unmatched(mine, &their_texts), unmatched(theirs, &my_texts));
    let their_keys = counts(
        theirs
            .iter()
            .zip(&theirs_left)
            .filter(|(_, left)| **left)
            .map(|(l, _)| l.1.as_str()),
    );
    let mut seen: BTreeMap<&str, u32> = BTreeMap::new();
    mine.iter()
        .zip(mine_left)
        .map(|(line, left)| {
            if !left {
                return Found::SameText;
            }
            let n = seen.entry(line.1.as_str()).or_default();
            *n += 1;
            if *n <= their_keys.get(line.1.as_str()).copied().unwrap_or(0) {
                Found::SameKey
            } else {
                Found::Missing
            }
        })
        .collect()
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
            linker_lines: [linker_lines(from), linker_lines(to)],
            wiring: RefCell::new(BTreeMap::new()),
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
            Err(_) => Label::NoOrigin,
        }
    }

    fn keyed(
        &self,
        to_side: bool,
        lines: &[Place<'a>],
        key: fn(&str) -> String,
    ) -> Vec<(&'a str, String)> {
        let text = |place: &Place<'a>| self.tree(to_side).line(place.0, place.1).unwrap_or("");
        lines.iter().map(|p| (text(p), key(text(p)))).collect()
    }

    /// How a wiring line is found on the other side, among its group's
    /// lines there.
    fn found(
        &self,
        to_side: bool,
        group: String,
        at: Place<'a>,
        lines: impl FnOnce() -> (Vec<Place<'a>>, Vec<Place<'a>>),
        key: fn(&str) -> String,
    ) -> Found {
        let mut wiring = self.wiring.borrow_mut();
        let found = wiring.entry((to_side, group)).or_insert_with(|| {
            let (mine, theirs) = lines();
            let matched = match_lines(
                &self.keyed(to_side, &mine, key),
                &self.keyed(!to_side, &theirs, key),
            );
            mine.into_iter().zip(matched).collect()
        });
        found.get(&at).copied().unwrap_or(Found::Missing)
    }

    fn linker(&self, to_side: bool, file: u32, at: Place<'a>) -> Label {
        let side = usize::from(to_side);
        let name = self.tree(to_side).input_name(file);
        let lines = || {
            let partner = self
                .truth
                .paired_file(to_side, name)
                .and_then(|other| self.tree(!to_side).input_index(other));
            let of = |side: usize, file: Option<u32>| {
                file.and_then(|f| self.linker_lines[side].get(&f))
                    .cloned()
                    .unwrap_or_default()
            };
            (of(side, Some(file)), of(1 - side, partner))
        };
        if self.truth.paired_file(to_side, name).is_none() {
            return Label::LinkerUnpaired;
        }
        match self.found(to_side, format!("linker {name}"), at, lines, masked_owned) {
            Found::SameText if is_init_call(self.tree(to_side).line(at.0, at.1).unwrap_or("")) => {
                Label::ImportsReordered
            }
            Found::SameText => Label::LinkerMoved,
            Found::SameKey => Label::LinkerRenamed,
            Found::Missing => Label::LinkerChanged,
        }
    }

    fn require(&self, to_side: bool, at: Place<'a>) -> Label {
        let lines = || {
            (
                require_lines(self.tree(to_side), at.0),
                require_lines_in(self.tree(!to_side), at.0),
            )
        };
        match self.found(
            to_side,
            format!("require {}", at.0),
            at,
            lines,
            require_target,
        ) {
            Found::SameText => Label::RequireMoved,
            Found::SameKey => Label::RequireRenamed,
            Found::Missing => Label::RequireEdge,
        }
    }

    fn pipeline(&self, to_side: bool, at: Place<'a>) -> Label {
        match wiring(self.tree(to_side).line(at.0, at.1).unwrap_or("")) {
            Label::RequireEdge => self.require(to_side, at),
            other => other,
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
            None => Label::NoOrigin,
            Some(Origin::Pipeline) => self.pipeline(to_side, (path, line)),
            Some(Origin::Linker(file)) => self.linker(to_side, file, (path, line)),
            Some(Origin::At(file, offset)) => match side.at(tree.input_name(file), offset) {
                Standing::Unchanged(pair) => {
                    self.unchanged(to_side, pair, UnitLine { offset, path, line })
                }
                Standing::Duplicate => Label::InputDuplicate,
                Standing::Changed => Label::InputChanged,
                Standing::OutsideUnits => Label::InputOutsideUnits,
                Standing::UnknownFile => Label::NoOrigin,
            },
        }
    }
}

#[cfg(test)]
#[path = "label_test.rs"]
mod label_test;
