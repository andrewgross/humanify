//! The linked program's shape: which chunks load eagerly, the order the
//! chunks are written in, which need an init, and where every imported
//! name really lives.

use oxc_semantic::SymbolId;

use super::analyse::{Chunk, ExportTarget, Imported, Spec, StmtKind};

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

/// Is `helper` (a top-level binding of `chunk`) a thunk factory written
/// at or before `chunk`?
fn is_factory(chunks: &[Chunk], order: &Order, chunk: usize, helper: SymbolId) -> bool {
    match resolve_sym(chunks, chunk, helper) {
        Ok(Target::Sym(d, s)) => {
            chunks[d].thunk_factories.contains(&s) && order.position[d] <= order.position[chunk]
        }
        _ => false,
    }
}

fn pure(chunks: &[Chunk], order: &Order, chunk: usize, purity: &super::analyse::Purity) -> bool {
    purity
        .as_ref()
        .is_some_and(|helpers| helpers.iter().all(|&h| is_factory(chunks, order, chunk, h)))
}

/// Per lazy chunk, which statements move into its init: everything that
/// does something or reads another binding. Declarations whose values are
/// pure stay at the top level (so do functions, and statements that
/// declare a top-level `var` from a nested block). Eager chunks move
/// nothing.
pub(super) fn moved_statements(chunks: &[Chunk], order: &Order) -> Vec<Vec<bool>> {
    (0..chunks.len())
        .map(|c| {
            chunks[c]
                .stmts
                .iter()
                .map(|st| !order.eager[c] && moves(chunks, order, c, &st.kind))
                .collect()
        })
        .collect()
}

fn moves(chunks: &[Chunk], order: &Order, c: usize, kind: &StmtKind) -> bool {
    match kind {
        StmtKind::Removed | StmtKind::ExportPrefix(_) | StmtKind::Function => false,
        StmtKind::Class { pure, .. } => !pure,
        StmtKind::Var(ds) => ds.iter().any(|d| !pure(chunks, order, c, &d.purity)),
        StmtKind::DefaultExpr { purity, .. } => !pure(chunks, order, c, purity),
        StmtKind::Other { nested_decl } => !nested_decl,
    }
}

/// A lazy chunk needs an init when it moved a statement or any lazy chunk
/// it imports needs one (its init runs theirs first).
pub(super) fn needs_init(chunks: &[Chunk], order: &Order, moved: &[Vec<bool>]) -> Vec<bool> {
    let mut init: Vec<bool> = moved.iter().map(|m| m.iter().any(|x| *x)).collect();
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
