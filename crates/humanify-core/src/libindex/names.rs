//! Restoring a package's ORIGINAL identifier names (exp159): what the
//! index holds beside a unit row, and the join that recovers it.
//!
//! A minified bundle's unit prints are name-free (locals slot); the
//! original names live in the package's own source files, which the index
//! builder has installed. The bridge is the build's sourcemap: Bun anchors
//! every statement and declarator of the minified output to the original
//! line it came from, so a declared identifier of a unit anchors to an
//! original LINE, and the original file's identifiers on that line pair
//! with it in order. Nothing here aligns by PRINT HASH — measured, the
//! minifier's structural rewrites (comma sequences, dropped braces, merged
//! declarators) make only ~20% of a build's units hash-equal their
//! unminified build (exp159 census A); the positional join is not capped.
//!
//! Builder-only machinery until the wiring: the pipeline never has a
//! sourcemap (`humanify-index/src/sourcemap.rs`), so this module holds the
//! data shapes and the pure join, and `humanify-index` drives it.

use oxc_ast::AstKind;
use oxc_span::{GetSpan, Span};
use oxc_syntax::symbol::SymbolId;
use serde::{Deserialize, Serialize};

/// What kind of identifier a declaration is (the ORIGINAL source's kind:
/// the minifier's own shape is not the truth — a class declaration
/// minifies to a var).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NameKind {
    Param,
    Local,
    Function,
    Class,
    Catch,
    Import,
    Other,
}

impl NameKind {
    /// Every kind in order: an ordinal's one meaning, shared by the names
    /// table's binary encoding (the jsonl spells it; the bin numbers it).
    pub const ALL: [NameKind; 7] = [
        NameKind::Param,
        NameKind::Local,
        NameKind::Function,
        NameKind::Class,
        NameKind::Catch,
        NameKind::Import,
        NameKind::Other,
    ];

    /// The kind's number in [`NameKind::ALL`].
    pub fn ordinal(self) -> u32 {
        Self::ALL
            .iter()
            .position(|k| *k == self)
            .expect("a kind is in ALL") as u32
    }

    /// The kind [`NameKind::ALL`]'s `ordinal` holds; None for one it does
    /// not (corruption is refused, never a silently wrong kind).
    pub fn from_ordinal(ordinal: u32) -> Option<NameKind> {
        Self::ALL.get(ordinal as usize).copied()
    }
}

/// One restored name: the slot of a unit's canonical print (`$slot`) and
/// the spelling the ORIGINAL source declared, with its kind. Slot-indexed:
/// a slot the join could not recover is simply absent, so a lookup's
/// answer stays position-true for the slots it does name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RestoredName {
    pub slot: u32,
    pub name: String,
    pub kind: NameKind,
}

/// The minified side of the join, before the map: a declared identifier of
/// a unit's print — its slot and its declaration's start in the bundle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotDecl {
    pub slot: u32,
    pub start: u32,
}

/// [`SlotDecl`] with its anchor: the ORIGINAL line the sourcemap resolves
/// the declaration to, and the declaration's bundle position (the
/// within-line order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinDecl {
    pub slot: u32,
    pub line: u32,
    pub at: u32,
}

/// The original side: a declared identifier of the package's source file,
/// grouped by the line it is declared on (its own line, `at` ordering).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrigDecl {
    pub line: u32,
    pub at: u32,
    pub name: String,
    pub kind: NameKind,
}

/// What a declaration node's kind names the identifier (ONE owner for both
/// sides of the join).
pub fn classify(kind: AstKind<'_>) -> NameKind {
    use oxc_ast::AstKind;
    match kind {
        AstKind::Function(_) | AstKind::MethodDefinition(_) => NameKind::Function,
        AstKind::Class(_) => NameKind::Class,
        AstKind::FormalParameter(_)
        | AstKind::FormalParameterRest(_)
        | AstKind::BindingRestElement(_)
        | AstKind::AssignmentPattern(_) => NameKind::Param,
        AstKind::CatchParameter(_) => NameKind::Catch,
        AstKind::VariableDeclarator(_) | AstKind::BindingProperty(_) => NameKind::Local,
        AstKind::ImportDefaultSpecifier(_)
        | AstKind::ImportNamespaceSpecifier(_)
        | AstKind::ImportSpecifier(_) => NameKind::Import,
        _ => NameKind::Other,
    }
}

/// The start of every line of `code` (0-based line numbers, the sourcemap's).
pub fn line_starts(code: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    starts.extend(
        code.match_indices('\n')
            .map(|(i, _)| u32::try_from(i + 1).expect("line start fits u32")),
    );
    starts
}

/// The 0-based line a byte offset of `code` is on (`starts` from
/// [`line_starts`]).
pub fn line_of(starts: &[u32], at: u32) -> u32 {
    match starts.partition_point(|&s| s <= at) {
        0 => 0,
        n => u32::try_from(n - 1).expect("line index fits u32"),
    }
}

/// A unit print's declared identifiers: the canonical mapping's slots whose
/// symbol is DECLARED inside the unit (`decl_span` is the symbol's
/// declaration identifier span; `scoping.symbol_span`). In slot order.
pub fn slot_decls(
    mapping: &[(String, Option<SymbolId>, String)],
    unit: Span,
    decl_span: impl Fn(SymbolId) -> Span,
) -> Vec<SlotDecl> {
    mapping
        .iter()
        .enumerate()
        .filter_map(|(slot, (_, symbol, _))| {
            let symbol = (*symbol)?;
            let span = decl_span(symbol);
            (span.start >= unit.start && span.start < unit.end).then(|| SlotDecl {
                slot: u32::try_from(slot).expect("slot ordinal fits u32"),
                start: span.start,
            })
        })
        .collect()
}

/// The declared identifiers of one parsed source file: every symbol's
/// declaration, grouped by its line, ordered within the line. The ORIGINAL
/// side of the join (parse the package's own file, never a bundle's).
pub fn declared_decls(code: &str, semantic: &oxc_semantic::Semantic<'_>) -> Vec<OrigDecl> {
    let scoping = semantic.scoping();
    let starts = line_starts(code);
    let mut decls: Vec<OrigDecl> = scoping
        .symbol_ids()
        .filter_map(|symbol| {
            let node = semantic
                .nodes()
                .get_node(scoping.symbol_declaration(symbol));
            let name = scoping.symbol_name(symbol);
            (!name.is_empty()).then(|| OrigDecl {
                line: line_of(&starts, node.span().start),
                at: node.span().start,
                name: name.to_string(),
                kind: classify(node.kind()),
            })
        })
        .collect();
    // `at` is a byte offset; within a line the byte order IS the order the
    // minified side pairs against (statement order is preserved).
    decls.sort_unstable_by_key(|d| (d.line, d.at));
    decls
}

/// The restored names of one unit print: `anchored` resolves a bundle byte
/// position to `(source index, original line)` — None when the position is
/// not the index's attributed source (a helper the bundler injected is
/// nobody's identifier) — and `orig` is the attributed source file's
/// [`declared_decls`]. In slot order; a slot whose line the join could not
/// pair (a group whose sides differ — a minifier-dropped local, an
/// injected binding) is absent, never shifted.
pub fn names_for_print(
    mapping: &[(String, Option<SymbolId>, String)],
    unit: Span,
    attributed: u32,
    decl_span: impl Fn(SymbolId) -> Span,
    anchored: impl Fn(u32) -> Option<(u32, u32)>,
    orig: &[OrigDecl],
) -> Vec<RestoredName> {
    let decls: Vec<MinDecl> = slot_decls(mapping, unit, decl_span)
        .into_iter()
        .filter_map(|d| {
            let (src, line) = anchored(d.start)?;
            (src == attributed).then_some(MinDecl {
                slot: d.slot,
                line,
                at: d.start,
            })
        })
        .collect();
    join(&decls, orig)
        .into_iter()
        .zip(decls)
        .filter_map(|(name, d)| {
            name.map(|(name, kind)| RestoredName {
                slot: d.slot,
                name,
                kind,
            })
        })
        .collect()
}

/// The join itself: pair the minified declarations with the original ones
/// LINE by LINE, in order, when the two sides of a line agree in count.
/// Output is parallel to `min` (input order).
pub fn join(min: &[MinDecl], orig: &[OrigDecl]) -> Vec<Option<(String, NameKind)>> {
    use std::collections::BTreeMap;
    let mut by_line: BTreeMap<u32, Vec<&MinDecl>> = BTreeMap::new();
    for decl in min {
        by_line.entry(decl.line).or_default().push(decl);
    }
    for group in by_line.values_mut() {
        group.sort_unstable_by_key(|d| d.at);
    }
    let mut orig_by_line: BTreeMap<u32, Vec<&OrigDecl>> = BTreeMap::new();
    for decl in orig {
        orig_by_line.entry(decl.line).or_default().push(decl);
    }
    min.iter()
        .map(|decl| {
            let mine = by_line.get(&decl.line)?;
            let theirs = orig_by_line.get(&decl.line)?;
            if mine.len() != theirs.len() {
                return None;
            }
            let i = mine.iter().position(|d| std::ptr::eq(*d, decl))?;
            theirs.get(i).map(|o| (o.name.clone(), o.kind))
        })
        .collect()
}
