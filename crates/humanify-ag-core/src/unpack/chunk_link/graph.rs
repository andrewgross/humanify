//! The linked program's shape: which chunks load eagerly, the order the
//! chunks are written in, which need an init, and where every imported
//! name really lives.

use oxc_semantic::SymbolId;

use super::analyse::{Chunk, Declarator, ExportTarget, Imported, Spec, StmtKind};

/// A binding of the linked program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Target {
    /// A top-level binding a chunk declares (or an external import).
    Sym(usize, SymbolId),
    /// A chunk's namespace object (made by the linker).
    Ns(usize),
    /// A chunk's `export default <expression>` value (made by the linker).
    Default(usize),
}

/// Where `chunk`'s export `imported` lives, following re-export chains.
pub(super) fn resolve_export(
    chunks: &[Chunk],
    chunk: usize,
    imported: &Imported,
) -> Result<Target, String> {
    let mut at = (chunk, imported.clone());
    for _ in 0..chunks.len() + 1 {
        let (c, imp) = at;
        let name = match imp {
            Imported::Namespace => return Ok(Target::Ns(c)),
            Imported::Default => "default".to_string(),
            Imported::Name(n) => n,
        };
        let Some((_, target)) = chunks[c].exports.iter().find(|(n, _)| *n == name) else {
            return Err(format!(
                "{} does not export `{name}`",
                chunks[c].path.display()
            ));
        };
        at = match target {
            ExportTarget::Local(s) => match import_of(&chunks[c], *s) {
                Some((Spec::Chunk(d), imp)) => (d, imp),
                _ => return Ok(Target::Sym(c, *s)),
            },
            ExportTarget::From(Spec::Chunk(d), imp) => (*d, imp.clone()),
            ExportTarget::From(Spec::External(s), _) => {
                return Err(format!(
                    "{}: re-exports from \"{s}\"; not linked",
                    chunks[c].path.display()
                ));
            }
            ExportTarget::DefaultExpr => return Ok(Target::Default(c)),
        };
    }
    Err(format!(
        "{}: a re-export cycle",
        chunks[chunk].path.display()
    ))
}

/// The import declaring `sym`, when `sym` is an import binding.
fn import_of(chunk: &Chunk, sym: SymbolId) -> Option<(Spec, Imported)> {
    chunk.imports.iter().find_map(|d| {
        d.specifiers
            .iter()
            .find(|(_, local)| *local == sym)
            .map(|(imp, _)| (d.spec.clone(), imp.clone()))
    })
}

/// The binding a chunk's top-level symbol stands for: itself, or (an
/// import of a chunk) the binding the import resolves to.
pub(super) fn resolve_sym(chunks: &[Chunk], chunk: usize, sym: SymbolId) -> Result<Target, String> {
    match import_of(&chunks[chunk], sym) {
        Some((Spec::Chunk(d), imp)) => resolve_export(chunks, d, &imp),
        _ => Ok(Target::Sym(chunk, sym)),
    }
}

/// ESM evaluation order from `start` over static imports: each chunk
/// after its imports (depth first, in source order), each once.
fn postorder(chunks: &[Chunk], start: usize, placed: &mut [bool], out: &mut Vec<usize>) {
    if placed[start] {
        return;
    }
    placed[start] = true;
    let mut stack = vec![(start, 0usize)];
    while let Some((c, i)) = stack.pop() {
        let Some(&d) = chunks[c].static_deps.get(i) else {
            out.push(c);
            continue;
        };
        stack.push((c, i + 1));
        if !placed[d] {
            placed[d] = true;
            stack.push((d, 0));
        }
    }
}

/// The written order and which chunks load eagerly.
pub(super) struct Order {
    /// One depth-first walk from the entry, each chunk after the chunks
    /// it reaches (postorder): an eager chunk walks its static imports
    /// first (so the eager chunks keep their ESM evaluation order), then
    /// its lazy loads; a lazy chunk walks its lazy loads, then its static
    /// imports; a lazy chunk never walks into an eager one (that one is
    /// written at its own turn). A lazily loaded chunk so lands beside the
    /// chunk that loads it, and the entry comes last — closer to Bun's
    /// single-file layout than "every eager chunk, then every lazy one"
    /// (2.1.242's unpacked app text against
    /// 2.1.241's: 55% of the shared unique string anchors in order, against
    /// 38%; a normal hop reads 100%).
    pub chunks: Vec<usize>,
    pub eager: Vec<bool>,
    pub position: Vec<usize>,
}

/// The chunks a chunk's walk visits, in order (see [`Order`]).
fn walk_edges(chunks: &[Chunk], eager: &[bool], c: usize) -> Vec<usize> {
    let lazy = chunks[c].lazy.iter().map(|e| e.target);
    let statics = chunks[c].static_deps.iter().copied();
    let edges: Vec<usize> = if eager[c] {
        statics.chain(lazy).collect()
    } else {
        lazy.chain(statics).collect()
    };
    edges
        .into_iter()
        .filter(|&d| eager[c] || !eager[d])
        .collect()
}

pub(super) fn order(chunks: &[Chunk]) -> Order {
    let n = chunks.len();
    let mut placed = vec![false; n];
    let mut eager_order = Vec::new();
    postorder(chunks, 0, &mut placed, &mut eager_order);
    let mut eager = vec![false; n];
    eager_order.iter().for_each(|&c| eager[c] = true);
    let mut visited = vec![false; n];
    visited[0] = true;
    let mut written = Vec::with_capacity(n);
    let mut stack = vec![(0usize, walk_edges(chunks, &eager, 0), 0usize)];
    while let Some((c, edges, i)) = stack.pop() {
        let Some(&d) = edges.get(i) else {
            written.push(c);
            continue;
        };
        stack.push((c, edges, i + 1));
        if !visited[d] {
            visited[d] = true;
            stack.push((d, walk_edges(chunks, &eager, d), 0));
        }
    }
    let mut position = vec![usize::MAX; n];
    written
        .iter()
        .enumerate()
        .for_each(|(i, &c)| position[c] = i);
    Order {
        chunks: written,
        eager,
        position,
    }
}

/// Where each statement of the linked program runs.
pub(super) struct Placement {
    /// Per chunk, per statement: does (some of) it run in the chunk's
    /// init?
    pub moved: Vec<Vec<bool>>,
    /// Per chunk, per statement: for a `var`, which declarators stay at
    /// the top level (one flag per declarator; empty for other kinds).
    pub stays: Vec<Vec<Vec<bool>>>,
}

/// ESM evaluation, exactly: a lazily loaded chunk's statements all run in
/// its init, in order, when it is first loaded (exp218). What stays at
/// the top level is only what is order-free — evaluating it earlier
/// cannot be observed:
///
/// - function declarations (ESM hoists them when the module graph is
///   instantiated, before any module body runs);
/// - a thunk factory, `var h = (…) => () => …` never written (building a
///   closure reads nothing: Bun's `__commonJS` / `__esm` helpers);
/// - a factory call, `var x = h(fn, …)`, when `h` is a thunk factory
///   written earlier in the program (top level: a thunk factory always
///   stays, and an eager chunk's statements all do) — calling it builds a
///   thunk and runs nothing. These are Bun's `__commonJS` modules and
///   `__esm` inits, which its single-file bundles keep at the top level
///   and where the bun adapter extracts them.
///
/// A `var` statement whose declarators differ is split: the order-free
/// ones are written at the top level, the rest are assigned in the init.
/// Eager chunks run at the top level in ESM postorder and move nothing.
///
/// Refused: a lazy chunk's statement that declares a top-level `var` from
/// a nested block (`if (x) { var y }`) — moved into the init, its `var`
/// would become the init's own. Bun hoists those itself: none in any
/// Claude Code release from 2.1.242 to 2.1.322.
pub(super) fn placement(chunks: &[Chunk], order: &Order) -> Result<Placement, String> {
    let mut moved = Vec::with_capacity(chunks.len());
    let mut stays = Vec::with_capacity(chunks.len());
    for (c, chunk) in chunks.iter().enumerate() {
        let nested = chunk
            .stmts
            .iter()
            .any(|st| matches!(st.kind, StmtKind::Other { nested_decl: true }));
        if nested && !order.eager[c] {
            return Err(format!(
                "{}: a lazily loaded chunk declares a top-level `var` inside a nested block; not linked",
                chunk.path.display()
            ));
        }
        let per: Vec<Vec<bool>> = chunk
            .stmts
            .iter()
            .enumerate()
            .map(|(i, st)| match &st.kind {
                StmtKind::Var(ds) => ds
                    .iter()
                    .map(|d| order.eager[c] || order_free(chunks, order, (c, i), d))
                    .collect(),
                _ => Vec::new(),
            })
            .collect();
        moved.push(
            chunk
                .stmts
                .iter()
                .zip(&per)
                .map(|(st, s)| !order.eager[c] && moves(&st.kind, s))
                .collect(),
        );
        stays.push(per);
    }
    Ok(Placement { moved, stays })
}

fn moves(kind: &StmtKind, stays: &[bool]) -> bool {
    match kind {
        StmtKind::Removed | StmtKind::Function => false,
        StmtKind::Var(_) => stays.iter().any(|s| !s),
        StmtKind::Class { .. } | StmtKind::DefaultExpr { .. } | StmtKind::Other { .. } => true,
    }
}

/// Is a lazy chunk's declarator order-free (see [`placement`])? `at` is
/// its (chunk, statement).
fn order_free(chunks: &[Chunk], order: &Order, at: (usize, usize), d: &Declarator) -> bool {
    if d.thunk_factory {
        return true;
    }
    let Some(helper) = d.factory_call else {
        return false;
    };
    match resolve_sym(chunks, at.0, helper) {
        Ok(Target::Sym(hc, hs)) => chunks[hc]
            .thunk_factories
            .get(&hs)
            .is_some_and(|&hi| (order.position[hc], hi) < (order.position[at.0], at.1)),
        _ => false,
    }
}

/// Does the chunk import anything that is not a chunk? A lazy chunk loads
/// those in its init (Bun resolves some packages itself, `ws`; loaded
/// eagerly under Node they would fail the whole program).
fn has_externals(chunk: &Chunk) -> bool {
    chunk
        .imports
        .iter()
        .any(|d| matches!(d.spec, Spec::External(_)))
}

/// A lazy chunk needs an init when it moved a statement or any lazy chunk
/// it imports needs one (its init runs theirs first).
pub(super) fn needs_init(chunks: &[Chunk], order: &Order, moved: &[Vec<bool>]) -> Vec<bool> {
    let mut init: Vec<bool> = moved
        .iter()
        .zip(chunks)
        .enumerate()
        .map(|(c, (m, chunk))| m.iter().any(|x| *x) || (!order.eager[c] && has_externals(chunk)))
        .collect();
    loop {
        let mut changed = false;
        for c in 0..chunks.len() {
            if init[c] || order.eager[c] {
                continue;
            }
            if chunks[c]
                .static_deps
                .iter()
                .any(|&d| init[d] && !order.eager[d])
            {
                init[c] = true;
                changed = true;
            }
        }
        if !changed {
            return init;
        }
    }
}
