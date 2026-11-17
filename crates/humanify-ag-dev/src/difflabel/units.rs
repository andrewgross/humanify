//! One input file's comparable units: every function, and every other
//! top-level statement that is not an import or export.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use humanify_ag_core::ingest::Ingest;
use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{FunctionType, Statement};
use oxc_span::GetSpan;
use sha2::{Digest, Sha256};

/// A name a unit reads that it does not declare inside itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FreeName {
    /// Declared at this byte of the same file.
    Declared(u32),
    /// Not declared in the file; the hash of its spelling.
    Global(u64),
}

#[derive(Clone, Copy)]
enum Symbol {
    Declared(u32),
    Global(u64),
    Label(u64),
}

struct Token {
    start: u32,
    end: u32,
    symbol: Symbol,
    /// `{a}`: prints as `a:<number>`.
    shorthand: bool,
}

pub struct Unit {
    pub start: u32,
    pub end: u32,
    pub parent: Option<u32>,
    /// Where a function declaration declares its own name.
    own_name: Option<u32>,
    /// Hash of the text with every identifier replaced by a number, by
    /// first use.
    pub text_key: u128,
}

pub struct InputFile {
    pub name: String,
    /// Sorted by start, an enclosing unit first.
    pub units: Vec<Unit>,
    tokens: Vec<Token>,
}

fn name_hash(name: &str) -> u64 {
    let digest = Sha256::digest(name.as_bytes());
    u64::from_le_bytes(digest[..8].try_into().expect("8 bytes"))
}

struct Collected {
    tokens: Vec<Token>,
    shorthand: BTreeSet<u32>,
    /// `(start, end, own name, is a statement)`.
    spans: Vec<(u32, u32, Option<u32>, bool)>,
}

fn collect_node(kind: AstKind<'_>, ingest: &Ingest<'_>, out: &mut Collected) {
    let scoping = ingest.semantic().scoping();
    let mut token = |span: oxc_span::Span, symbol: Symbol| {
        out.tokens.push(Token {
            start: span.start,
            end: span.end,
            symbol,
            shorthand: false,
        });
    };
    match kind {
        AstKind::Function(f) if f.body.is_some() => {
            let declared = f.r#type == FunctionType::FunctionDeclaration;
            let own = f.id.as_ref().filter(|_| declared).map(|id| id.span.start);
            out.spans.push((f.span.start, f.span.end, own, false));
        }
        AstKind::ArrowFunctionExpression(f) => {
            out.spans.push((f.span.start, f.span.end, None, false));
        }
        AstKind::IdentifierReference(it) => {
            let declared = it
                .reference_id
                .get()
                .and_then(|r| scoping.get_reference(r).symbol_id())
                .map(|s| scoping.symbol_span(s).start);
            token(
                it.span,
                declared.map_or_else(|| Symbol::Global(name_hash(&it.name)), Symbol::Declared),
            );
        }
        AstKind::BindingIdentifier(it) => {
            let declared = it.symbol_id.get().map(|s| scoping.symbol_span(s).start);
            token(it.span, Symbol::Declared(declared.unwrap_or(it.span.start)));
        }
        AstKind::LabelIdentifier(it) => token(it.span, Symbol::Label(name_hash(&it.name))),
        AstKind::ObjectProperty(it) if it.shorthand => {
            out.shorthand.insert(it.key.span().start);
        }
        AstKind::BindingProperty(it) if it.shorthand => {
            out.shorthand.insert(it.key.span().start);
        }
        AstKind::AssignmentTargetPropertyIdentifier(it) => {
            out.shorthand.insert(it.binding.span.start);
        }
        _ => {}
    }
}

fn collect(ingest: &Ingest<'_>) -> Collected {
    let mut out = Collected {
        tokens: Vec::new(),
        shorthand: BTreeSet::new(),
        spans: Vec::new(),
    };
    for node in ingest.semantic().nodes().iter() {
        collect_node(node.kind(), ingest, &mut out);
    }
    for statement in &ingest.program.body {
        let wiring = matches!(
            statement,
            Statement::ImportDeclaration(_)
                | Statement::ExportNamedDeclaration(_)
                | Statement::ExportAllDeclaration(_)
                | Statement::ExportDefaultDeclaration(_)
                | Statement::FunctionDeclaration(_)
        );
        if !wiring {
            let span = statement.span();
            out.spans.push((span.start, span.end, None, true));
        }
    }
    out.tokens.sort_by_key(|t| t.start);
    out.tokens.dedup_by_key(|t| t.start);
    for t in &mut out.tokens {
        t.shorthand = out.shorthand.contains(&t.start);
    }
    out.spans
        .sort_by_key(|s| (s.0, std::cmp::Reverse(s.1), !s.3));
    out
}

fn tokens_in(tokens: &[Token], start: u32, end: u32) -> &[Token] {
    let from = tokens.partition_point(|t| t.start < start);
    let to = tokens.partition_point(|t| t.start < end);
    &tokens[from..to]
}

fn text_key(text: &str, tokens: &[Token], start: u32, end: u32, statement: bool) -> u128 {
    let mut out = String::with_capacity((end - start) as usize + 1);
    out.push(if statement { 'S' } else { 'F' });
    let mut numbers: BTreeMap<(u8, u64), usize> = BTreeMap::new();
    let mut at = start as usize;
    for t in tokens_in(tokens, start, end) {
        out.push_str(&text[at..t.start as usize]);
        if t.shorthand {
            out.push_str(&text[t.start as usize..t.end as usize]);
            out.push(':');
        }
        let key = match t.symbol {
            Symbol::Declared(d) => (0, u64::from(d)),
            Symbol::Global(h) => (1, h),
            Symbol::Label(h) => (2, h),
        };
        let next = numbers.len();
        let _ = write!(out, "${}", numbers.entry(key).or_insert(next));
        at = t.end as usize;
    }
    out.push_str(&text[at..end as usize]);
    let digest = Sha256::digest(out.as_bytes());
    u128::from_le_bytes(digest[..16].try_into().expect("16 bytes"))
}

fn with_parents(mut units: Vec<Unit>) -> Vec<Unit> {
    let mut open: Vec<u32> = Vec::new();
    for i in 0..units.len() {
        while open
            .last()
            .is_some_and(|&o| units[o as usize].end <= units[i].start)
        {
            open.pop();
        }
        units[i].parent = open.last().copied();
        open.push(i as u32);
    }
    units
}

impl InputFile {
    pub fn parse(name: &str, text: &str) -> InputFile {
        let allocator = Allocator::default();
        let ingest = Ingest::parse_module(&allocator, text);
        let Collected { tokens, spans, .. } = collect(&ingest);
        let units = spans
            .into_iter()
            .map(|(start, end, own_name, statement)| Unit {
                start,
                end,
                parent: None,
                own_name,
                text_key: text_key(text, &tokens, start, end, statement),
            })
            .collect();
        InputFile {
            name: name.to_string(),
            units: with_parents(units),
            tokens,
        }
    }

    /// The smallest unit holding the byte.
    pub fn innermost(&self, offset: u32) -> Option<u32> {
        let after = self.units.partition_point(|u| u.start <= offset);
        let mut at = after.checked_sub(1).map(|i| i as u32);
        while let Some(i) = at {
            if self.units[i as usize].end > offset {
                return at;
            }
            at = self.units[i as usize].parent;
        }
        None
    }

    /// Per identifier of the unit, in order: the outside name it is, if
    /// it is one. Two units with one text key have lists of one length.
    pub fn free_names(&self, unit: u32) -> Vec<Option<FreeName>> {
        let u = &self.units[unit as usize];
        tokens_in(&self.tokens, u.start, u.end)
            .iter()
            .map(|t| match t.symbol {
                Symbol::Label(_) => None,
                Symbol::Global(h) => Some(FreeName::Global(h)),
                Symbol::Declared(d) => (d < u.start || d >= u.end || Some(d) == u.own_name)
                    .then_some(FreeName::Declared(d)),
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "units_test.rs"]
mod units_test;
