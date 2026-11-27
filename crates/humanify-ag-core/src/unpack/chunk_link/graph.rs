//! The linked program's shape: which chunks load eagerly, the order the
//! chunks are written in, which need an init, and where every imported
//! name really lives.

use oxc_semantic::SymbolId;

use super::analyse::{Chunk, Declarator, ExportTarget, ImportDecl, Imported, Spec, StmtKind};
use super::error::LinkError;

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
) -> Result<Target, LinkError> {
    let mut at = (chunk, imported.clone());
    for _ in 0..chunks.len() + 1 {
        let (c, imp) = at;
        let name = match imp {
            Imported::Namespace => return Ok(Target::Ns(c)),
            Imported::Default => "default".to_string(),
            Imported::Name(n) => n,
        };
        let Some((_, target)) = chunks[c].exports.iter().find(|(n, _)| *n == name) else {
            return Err(LinkError::NoExport {
                path: chunks[c].path.clone(),
                name,
            });
        };
        at = match target {
            ExportTarget::Local(s) => match import_of(&chunks[c], *s) {
                Some((Spec::Chunk(d), imp)) => (d, imp),
                _ => return Ok(Target::Sym(c, *s)),
            },
            ExportTarget::From(Spec::Chunk(d), imp) => (*d, imp.clone()),
            ExportTarget::From(Spec::External(s), _) => {
                return Err(LinkError::ReExportExternal {
                    path: chunks[c].path.clone(),
                    spec: s.clone(),
                });
            }
            ExportTarget::DefaultExpr => return Ok(Target::Default(c)),
        };
    }
    Err(LinkError::Refused {
        path: chunks[chunk].path.clone(),
        what: "a re-export cycle",
    })
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
pub(super) fn resolve_sym(
    chunks: &[Chunk],
    chunk: usize,
    sym: SymbolId,
) -> Result<Target, LinkError> {
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
/// - a declarator whose value runs no code and reads no binding, and a
///   class that does the same (`inert`). Bun's own bundler keeps literals
///   and closures at the top level of a lazily wrapped module, as Claude
///   Code's single-file bundles up to 2.1.241 did;
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
pub(super) fn placement(chunks: &[Chunk], order: &Order) -> Result<Placement, LinkError> {
    let mut moved = Vec::with_capacity(chunks.len());
    let mut stays = Vec::with_capacity(chunks.len());
    for (c, chunk) in chunks.iter().enumerate() {
        let nested = chunk
            .stmts
            .iter()
            .any(|st| matches!(st.kind, StmtKind::Other { nested_decl: true }));
        if nested && !order.eager[c] {
            return Err(LinkError::Refused {
                path: chunk.path.clone(),
                what: "a lazily loaded chunk declares a top-level `var` inside a nested block; not linked",
            });
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
        StmtKind::Class { inert, .. } => !inert,
        StmtKind::DefaultExpr { .. } | StmtKind::Other { .. } => true,
    }
}

/// Is a lazy chunk's declarator order-free (see [`placement`])? `at` is
/// its (chunk, statement).
fn order_free(chunks: &[Chunk], order: &Order, at: (usize, usize), d: &Declarator) -> bool {
    if d.thunk_factory || d.inert_value {
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

/// Does the chunk import anything a lazy chunk loads in its init? (Bun
/// resolves some packages itself, `ws`; loaded eagerly under Node they
/// would fail the whole program.)
fn has_externals(chunk: &Chunk) -> bool {
    chunk.imports.iter().any(ImportDecl::loads_in_init)
}

/// Per chunk: can a walk from it over `edges` come back to it, or reach a
/// chunk that can? (Found by removing, until none is left, every chunk
/// with no edge to a remaining chunk.)
fn reaches_a_cycle(edges: &[Vec<usize>]) -> Vec<bool> {
    let mut left = vec![true; edges.len()];
    loop {
        let before = left.iter().filter(|l| **l).count();
        for c in 0..edges.len() {
            left[c] = left[c] && edges[c].iter().any(|&d| left[d]);
        }
        if before == left.iter().filter(|l| **l).count() {
            return left;
        }
    }
}

/// The chunks a walk from `start` over `edges` comes to, through chunks
/// of `within` (`start` itself only when the walk comes back to it).
fn reached(edges: &[Vec<usize>], within: &[bool], start: usize) -> Vec<bool> {
    let mut seen = vec![false; edges.len()];
    let mut stack = edges[start].clone();
    while let Some(c) = stack.pop() {
        if within[c] && !seen[c] {
            seen[c] = true;
            stack.extend(edges[c].iter().copied());
        }
    }
    seen
}

/// Per chunk, the chunks that share a cycle of static imports and
/// synchronous requires with one of the chunks it loads: only those can
/// be part-way through their init while the chunk's own init runs. Empty
/// for a chunk on no cycle, one that is only between two cycles included:
/// whatever it imports that is part-way through its init would have to
/// reach back to it.
fn may_be_running(chunks: &[Chunk]) -> Vec<Vec<bool>> {
    let forward: Vec<Vec<usize>> = chunks
        .iter()
        .map(|ch| {
            let sync = ch.lazy.iter().filter(|e| e.sync).map(|e| e.target);
            ch.static_deps.iter().copied().chain(sync).collect()
        })
        .collect();
    let mut backward = vec![Vec::new(); chunks.len()];
    for (c, deps) in forward.iter().enumerate() {
        deps.iter().for_each(|&d| backward[d].push(c));
    }
    let (down, up) = (reaches_a_cycle(&forward), reaches_a_cycle(&backward));
    let near: Vec<bool> = down.iter().zip(up).map(|(d, u)| *d && u).collect();
    (0..chunks.len())
        .map(|c| {
            let loaders = reached(&backward, &near, c);
            if near[c] && loaders[c] {
                loaders
            } else {
                vec![false; chunks.len()]
            }
        })
        .collect()
}

/// Mark every chunk the init of `start` runs.
fn mark_run(calls: &[Vec<usize>], start: usize, run: &mut [bool]) {
    let mut stack = vec![start];
    while let Some(c) = stack.pop() {
        if !run[c] {
            run[c] = true;
            stack.extend(calls[c].iter().copied());
        }
    }
}

/// One chunk's init calls that run a chunk, in import order. A call whose
/// chunk an earlier call already ran does nothing wherever it stands,
/// and the bundler moves it between builds of unchanged code. A call of
/// a chunk in `running` may return early, so it counts for nothing it
/// would run.
fn live_calls(calls: &[Vec<usize>], c: usize, running: &[bool]) -> Vec<usize> {
    let mut run = vec![false; calls.len()];
    run[c] = true;
    let mut live = Vec::new();
    for &d in &calls[c] {
        if !run[d] {
            live.push(d);
            if !running[d] {
                mark_run(calls, d, &mut run);
            }
        }
    }
    live
}

/// Per chunk, the chunks whose init its own init calls, in call order.
pub(super) fn init_calls(chunks: &[Chunk], order: &Order, needs_init: &[bool]) -> Vec<Vec<usize>> {
    let called = |ch: &Chunk| -> Vec<usize> {
        let deps = ch.static_deps.iter().copied();
        deps.filter(|&d| needs_init[d] && !order.eager[d]).collect()
    };
    let calls: Vec<Vec<usize>> = chunks.iter().map(called).collect();
    let running = may_be_running(chunks);
    (0..chunks.len())
        .map(|c| live_calls(&calls, c, &running[c]))
        .collect()
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
