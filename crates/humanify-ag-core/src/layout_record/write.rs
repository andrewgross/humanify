//! Builds a run's layout from what its stages recorded.

use std::collections::{BTreeMap, HashMap};

use super::{FORMAT, FileEntry, FunctionId, Layout, LayoutParseError, Match, Row, Unit, checksum};
use crate::babel_view::{line_of, line_starts};
use crate::emit::cjs::FileLines;
use crate::matching::decisions::MatchDecisions;
use crate::place::ledger::SplitLedger;
use crate::unpack::bun::UnpackOrigins;
use crate::unpack::chunk_link::{LinkMap, LinkerText};

/// The run's input.
pub struct Input<'a> {
    /// `None`: the input is one file, named `name`.
    pub link: Option<&'a LinkMap>,
    pub name: &'a str,
    /// `None`: the unpacked file is the input text.
    pub unpack: Option<&'a UnpackOrigins>,
}

/// The named text and what the formatter recorded for it.
pub struct Text<'a> {
    /// The formatted text the marks are offsets into.
    pub fresh: &'a str,
    /// The named text, line for line the formatted one.
    pub shipped: &'a str,
    /// Per statement: `(offset in fresh, offset in the unpacked file)`.
    pub statements: &'a [(u32, u32)],
    /// Per statement: how many statements enclose it, itself included.
    pub depths: &'a [u32],
    /// Per function: `(offset in fresh, offset in the unpacked file)`.
    pub functions: &'a [(u32, Option<u32>)],
    /// The line ends of the named text as the tree holds it.
    pub written_line_ends: u32,
}

/// What the function matcher decided against the previous tree.
pub struct Previous<'a> {
    pub matches: &'a MatchDecisions,
    /// The previous tree's layout file, when it has one.
    pub layout: Option<&'a str>,
}

/// The files the split wrote from the named text.
pub struct Split<'a> {
    /// The statements the split cut: their spans in the shipped text.
    pub spans: &'a [(u32, u32)],
    pub files: &'a [FileLines],
    /// What the split recorded for each statement.
    pub ledger: Option<&'a SplitLedger>,
    /// A written file's line ends.
    pub line_count: &'a dyn Fn(&str) -> u32,
}

pub struct Run<'a> {
    pub input: Input<'a>,
    pub text: Text<'a>,
    pub previous: Option<Previous<'a>>,
    /// The path of the whole named text in the tree.
    pub named_text: &'a str,
    pub split: Option<Split<'a>>,
}

/// Why no layout could be written.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LayoutError {
    #[error("the split's ledger was not read: {0}")]
    SplitLedger(String),
    #[error("the named text has {shipped} lines, the formatted text {fresh}")]
    LinesDiffer { fresh: usize, shipped: usize },
    #[error("the named text in the tree has {written} lines, the formatted text {fresh}")]
    WrittenLinesDiffer { fresh: usize, written: usize },
    #[error("the linker wrote the text at linked byte {at} and did not say what it is")]
    LinkerTextUnnamed { at: u32 },
    #[error("line {line} of {path} is from another input file than its statement")]
    AnotherInputFile { path: String, line: u32 },
    #[error("the matcher paired a function at byte {at} that the formatter did not print")]
    FunctionNotPrinted { at: u32 },
    #[error("the previous layout: {0}")]
    PreviousLayout(LayoutParseError),
    #[error("the previous layout has no entry for its named text")]
    PreviousNamedText,
    #[error(
        "the previous layout lists {layout} functions, the matcher read {matcher} in the previous text"
    )]
    PreviousFunctions { layout: usize, matcher: usize },
}

/// Where a piece of the named text came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Input {
        file: u32,
        byte: u32,
    },
    Linker {
        file: u32,
        text: LinkerText,
        within: u32,
    },
    /// Linked text that belongs to no input file, by its linked byte.
    Program {
        at: u32,
    },
}

impl Source {
    fn file(self) -> Option<u32> {
        match self {
            Source::Input { file, .. } | Source::Linker { file, .. } => Some(file),
            Source::Program { .. } => None,
        }
    }

    fn is_startup(self) -> bool {
        matches!(self, Source::Linker { text, .. } if text == LinkerText::Init)
    }
}

fn linker_name(text: LinkerText) -> &'static str {
    match text {
        LinkerText::Init => "init",
        LinkerText::Namespace => "namespace",
        LinkerText::Exports => "exports",
        LinkerText::ExternalNames => "externalNames",
        LinkerText::Load { .. } => "load",
        LinkerText::Call { .. } => "call",
        LinkerText::Decl { .. } => "decl",
    }
}

fn unit_at(source: Option<Source>, line: u32) -> Unit {
    let mut unit = Unit {
        line,
        ..Unit::default()
    };
    match source {
        None | Some(Source::Program { .. }) => {}
        Some(Source::Input { file, byte }) => unit.at = Some((file, Some(byte))),
        Some(Source::Linker { file, text, .. }) => {
            unit.at = Some((file, None));
            unit.linker = Some(linker_name(text).to_string());
            match text {
                LinkerText::Load { nth } => unit.nth = Some(nth),
                LinkerText::Call { target } => unit.target = Some(target),
                LinkerText::Decl { src } => unit.at = Some((file, Some(src))),
                _ => {}
            }
        }
    }
    unit
}

fn function_id(source: Source) -> FunctionId {
    match source {
        Source::Input { byte, .. } => FunctionId::Byte(byte),
        Source::Program { at } => FunctionId::Linker(format!("program+{at}")),
        Source::Linker { text, within, .. } => {
            let which = match text {
                LinkerText::Load { nth: n }
                | LinkerText::Call { target: n }
                | LinkerText::Decl { src: n } => format!(":{n}"),
                _ => String::new(),
            };
            FunctionId::Linker(format!("{}{which}+{within}", linker_name(text)))
        }
    }
}

/// A statement the formatter printed: its line in the named text (from
/// 0), its depth and its source.
struct Mark {
    line: u32,
    depth: u32,
    source: Option<Source>,
}

struct FunctionMark {
    line: u32,
    source: Option<Source>,
    matched: Match,
}

/// The statements and functions of the named text, by line.
struct Marked {
    marks: Vec<Mark>,
    functions: Vec<FunctionMark>,
    lines: u32,
    /// The depth of the outermost statements.
    outer: u32,
}

/// The lines of the named text one statement takes, and where it was
/// written.
#[derive(Clone, Copy)]
struct Place<'a> {
    path: &'a str,
    /// First and last line in the named text, from 0.
    lines: (u32, u32),
    /// Its first line in the written file, from 1.
    at: u32,
    /// The written statement has the same lines as in the named text.
    same_lines: bool,
    functions: bool,
}

impl Input<'_> {
    fn source_of(&self, unpacked: u32) -> Result<Option<Source>, LayoutError> {
        let raw = self
            .unpack
            .map_or(unpacked, |u| u.runtime.raw_of(unpacked as usize) as u32);
        let Some(link) = self.link else {
            return Ok(Some(Source::Input { file: 0, byte: raw }));
        };
        match link.source_of(raw) {
            None => Ok(Some(Source::Program { at: raw })),
            Some((file, Some(byte))) => Ok(Some(Source::Input { file, byte })),
            Some((_, None)) => {
                let (file, text, within) = link
                    .linker_text_at(raw)
                    .ok_or(LayoutError::LinkerTextUnnamed { at: raw })?;
                Ok(Some(Source::Linker { file, text, within }))
            }
        }
    }

    fn files(&self) -> Vec<String> {
        match self.link {
            Some(link) => link.files.clone(),
            None => vec![self.name.to_string()],
        }
    }
}

/// The previous layout's functions in the order of its named text, and
/// its checksum.
struct PreviousFunctions {
    checksum: String,
    functions: Vec<(u32, FunctionId)>,
}

fn previous_functions(text: &str) -> Result<PreviousFunctions, LayoutError> {
    let layout = Layout::parse(text).map_err(LayoutError::PreviousLayout)?;
    let named = layout
        .files
        .get(&layout.named_text)
        .ok_or(LayoutError::PreviousNamedText)?;
    Ok(PreviousFunctions {
        checksum: checksum(text),
        functions: named.functions().map(|f| (f.input, f.id.clone())).collect(),
    })
}

/// Per fresh function start: what the matcher decided.
fn decided(
    previous: Option<&Previous<'_>>,
    known: Option<&PreviousFunctions>,
) -> Result<HashMap<u32, Match>, LayoutError> {
    let Some(previous) = previous else {
        return Ok(HashMap::new());
    };
    let matches = previous.matches;
    if let Some(known) = known
        && known.functions.len() != matches.prior as usize
    {
        return Err(LayoutError::PreviousFunctions {
            layout: known.functions.len(),
            matcher: matches.prior as usize,
        });
    }
    let partner = |prior: u32| known.map(|k| k.functions[prior as usize].clone());
    let mut out = HashMap::new();
    for &(prior, fresh) in &matches.exact {
        out.insert(matches.fresh[fresh as usize], Match::Exact(partner(prior)));
    }
    for &(prior, fresh) in &matches.close {
        out.insert(matches.fresh[fresh as usize], Match::Close(partner(prior)));
    }
    Ok(out)
}

fn marked(run: &Run<'_>, mut decided: HashMap<u32, Match>) -> Result<Marked, LayoutError> {
    let text = &run.text;
    let starts = line_starts(text.fresh);
    let shipped = line_starts(text.shipped).len();
    if shipped != starts.len() {
        return Err(LayoutError::LinesDiffer {
            fresh: starts.len(),
            shipped,
        });
    }
    if text.written_line_ends as usize + 1 != starts.len() {
        return Err(LayoutError::WrittenLinesDiffer {
            fresh: starts.len(),
            written: text.written_line_ends as usize + 1,
        });
    }
    let mut marks = Vec::with_capacity(text.statements.len());
    for (&(at, unpacked), &depth) in text.statements.iter().zip(text.depths) {
        marks.push(Mark {
            line: line_of(&starts, at),
            depth,
            source: run.input.source_of(unpacked)?,
        });
    }
    let mut functions = Vec::with_capacity(text.functions.len());
    for &(at, unpacked) in text.functions {
        let source = match unpacked {
            Some(unpacked) => run.input.source_of(unpacked)?,
            None => None,
        };
        functions.push(FunctionMark {
            line: line_of(&starts, at),
            source,
            matched: decided.remove(&at).unwrap_or(Match::New),
        });
    }
    if let Some(at) = decided.into_keys().min() {
        return Err(LayoutError::FunctionNotPrinted { at });
    }
    Ok(Marked {
        outer: marks.iter().map(|m| m.depth).min().unwrap_or(0),
        marks,
        functions,
        lines: starts.len() as u32,
    })
}

impl Marked {
    /// The statement marks on lines `first..=last`.
    fn marks_in(&self, (first, last): (u32, u32)) -> &[Mark] {
        let from = self.marks.partition_point(|m| m.line < first);
        let to = self.marks.partition_point(|m| m.line <= last);
        &self.marks[from..to]
    }

    fn functions_in(&self, (first, last): (u32, u32)) -> &[FunctionMark] {
        let from = self.functions.partition_point(|f| f.line < first);
        let to = self.functions.partition_point(|f| f.line <= last);
        &self.functions[from..to]
    }

    /// The line ranges of the outermost statements.
    fn outermost(&self) -> Vec<(u32, u32)> {
        let firsts: Vec<u32> = self
            .marks
            .iter()
            .filter(|m| m.depth == self.outer)
            .map(|m| m.line)
            .collect();
        let ends = firsts.iter().skip(1).copied().chain([self.lines]);
        firsts
            .iter()
            .copied()
            .zip(ends.map(|next| next - 1))
            .collect()
    }

    /// Whether the statement lists the statements inside it: a wrapper
    /// around the program that no input file holds, or a startup function
    /// the linker wrote.
    fn holds_statements(&self, own: &Mark) -> bool {
        match own.source {
            Some(Source::Input { .. }) => false,
            Some(source @ Source::Linker { .. }) => source.is_startup(),
            Some(Source::Program { .. }) | None => own.depth == self.outer,
        }
    }

    /// One statement as a unit.
    fn unit(&self, place: &Place<'_>) -> Result<Unit, LayoutError> {
        let marks = self.marks_in(place.lines);
        let (first, last) = place.lines;
        let own = marks.first().filter(|m| m.line == first);
        let inner: Vec<u32> = match own {
            Some(own) if place.same_lines && self.holds_statements(own) => marks
                .iter()
                .filter(|m| m.depth == own.depth + 1 && m.line > first)
                .map(|m| m.line)
                .collect(),
            _ => Vec::new(),
        };
        let Some(&first_inner) = inner.first() else {
            return self.leaf(place, marks);
        };
        let mut unit = unit_at(own.and_then(|m| m.source), place.at);
        let line_in_file = |line: u32| place.at + (line - first);
        self.add_functions(&mut unit, place, (first, first_inner - 1))?;
        let ends = inner.iter().skip(1).map(|next| next - 1).chain([last]);
        for (&from, to) in inner.iter().zip(ends) {
            unit.body.push(self.unit(&Place {
                lines: (from, to),
                at: line_in_file(from),
                ..*place
            })?);
        }
        Ok(unit)
    }

    /// A statement whose lines all come from one input file.
    fn leaf(&self, place: &Place<'_>, marks: &[Mark]) -> Result<Unit, LayoutError> {
        let source = marks.iter().find_map(|m| m.source);
        let mut unit = unit_at(source, place.at);
        if !place.same_lines {
            return Ok(unit);
        }
        let Some(file) = source.and_then(Source::file) else {
            self.add_functions(&mut unit, place, place.lines)?;
            return Ok(unit);
        };
        let line_in_file = |line: u32| place.at + (line - place.lines.0);
        let mut current = source;
        for group in marks.chunk_by(|a, b| a.line == b.line) {
            let line = group[0].line;
            let first = group.iter().find_map(|m| m.source);
            if line == place.lines.0 || first.is_none() || first == current {
                continue;
            }
            current = first;
            if first.and_then(Source::file) != Some(file) {
                return Err(LayoutError::AnotherInputFile {
                    path: place.path.to_string(),
                    line: line_in_file(line),
                });
            }
            let byte = match first {
                Some(Source::Input { byte, .. }) => Some(byte),
                _ => None,
            };
            unit.rows.push(Row::Origin {
                line: line_in_file(line),
                byte,
            });
        }
        self.add_functions(&mut unit, place, place.lines)?;
        Ok(unit)
    }

    /// The functions that start on `lines`, as rows of the unit.
    fn add_functions(
        &self,
        unit: &mut Unit,
        place: &Place<'_>,
        lines: (u32, u32),
    ) -> Result<(), LayoutError> {
        if !place.functions {
            return Ok(());
        }
        let file = unit.at.map(|at| at.0);
        for f in self.functions_in(lines) {
            let line = place.at + (f.line - place.lines.0);
            if f.source.and_then(Source::file) != file {
                return Err(LayoutError::AnotherInputFile {
                    path: place.path.to_string(),
                    line,
                });
            }
            let after = unit.rows.partition_point(|r| row_line(r) <= line);
            let row = Row::Function {
                line,
                id: f.source.map_or(FunctionId::Unknown, function_id),
                matched: f.matched.clone(),
            };
            unit.rows.insert(after, row);
        }
        Ok(())
    }
}

fn row_line(row: &Row) -> u32 {
    match row {
        Row::Origin { line, .. } | Row::Function { line, .. } => *line,
    }
}

/// The whole named text as one file: its outermost statements.
fn whole_text(run: &Run<'_>, marked: &Marked) -> Result<FileEntry, LayoutError> {
    let mut units = Vec::new();
    for lines in marked.outermost() {
        units.push(marked.unit(&Place {
            path: run.named_text,
            lines,
            at: lines.0 + 1,
            same_lines: true,
            functions: true,
        })?);
    }
    Ok(FileEntry { units })
}

/// One file the split wrote. Later passes add lines above the first
/// statement only.
fn split_file(
    split: &Split<'_>,
    shipped_starts: &[u32],
    lines: &FileLines,
    marked: &Marked,
    names: &[Vec<String>],
) -> Result<FileEntry, LayoutError> {
    let written = (split.line_count)(&lines.path);
    let mut at = 1 + lines.header_lines + written.saturating_sub(lines.total());
    let mut units = Vec::with_capacity(lines.statements.len());
    for &(statement, count) in &lines.statements {
        if count == 0 {
            continue;
        }
        let (start, end) = split.spans[statement];
        let first = line_of(shipped_starts, start);
        let last = line_of(shipped_starts, end.saturating_sub(1).max(start));
        let mut unit = marked.unit(&Place {
            path: &lines.path,
            lines: (first, last),
            at,
            same_lines: last - first + 1 == count,
            functions: false,
        })?;
        if let Some(ledger) = split.ledger {
            unit.n = Some(statement as u32);
            unit.hash = ledger.hashes.get(statement).cloned();
            unit.names = names.get(statement).cloned().unwrap_or_default();
        }
        units.push(unit);
        at += count;
    }
    Ok(FileEntry { units })
}

/// The names each statement declares, by its position in the named text.
fn statement_names(ledger: &SplitLedger) -> Vec<Vec<String>> {
    let mut names = vec![Vec::new(); ledger.emit_names.len()];
    for (slot, written) in ledger.emit_names.iter().enumerate() {
        let statement = ledger.emit_indexes.as_ref().map_or(slot, |at| at[slot]);
        if let (Some(written), Some(own)) = (written, names.get_mut(statement)) {
            *own = written.split(',').map(str::to_string).collect();
        }
    }
    names
}

/// The layout of one run.
pub fn layout_of(run: &Run<'_>) -> Result<Layout, LayoutError> {
    let previous = run.previous.as_ref();
    let known = previous
        .and_then(|p| p.layout)
        .map(previous_functions)
        .transpose()?;
    let marked = marked(run, decided(previous, known.as_ref())?)?;
    let mut files = BTreeMap::new();
    files.insert(run.named_text.to_string(), whole_text(run, &marked)?);
    if let Some(split) = &run.split {
        let starts = line_starts(run.text.shipped);
        let names = split.ledger.map(statement_names).unwrap_or_default();
        for lines in split.files {
            let entry = split_file(split, &starts, lines, &marked, &names)?;
            files.insert(lines.path.clone(), entry);
        }
    }
    for ((start, _), path) in run.input.unpack.iter().flat_map(|u| &u.vendor) {
        let source = match run.input.link {
            Some(link) => link
                .source_of(*start)
                .and_then(|(file, byte)| Some(Source::Input { file, byte: byte? })),
            None => Some(Source::Input {
                file: 0,
                byte: *start,
            }),
        };
        let units = vec![unit_at(source, 1)];
        files.insert(path.clone(), FileEntry { units });
    }
    let ambiguous = previous.map_or(&[][..], |p| p.matches.ambiguous.as_slice());
    let ledger = run.split.as_ref().and_then(|s| s.ledger);
    Ok(Layout {
        format: FORMAT,
        inputs: run.input.files(),
        prior_layout: known.as_ref().map(|k| k.checksum.clone()),
        named_text: run.named_text.to_string(),
        files,
        prior_ambiguous: known.as_ref().map_or_else(Vec::new, |k| {
            let listed = ambiguous.iter();
            listed.map(|&p| k.functions[p as usize].clone()).collect()
        }),
        hash_version: ledger.map(|l| l.hash_version),
        modules: ledger.and_then(|l| l.fossil_modules.clone()),
    })
}
