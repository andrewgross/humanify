//! Writing the linked program: Bun's single-file CommonJS layout (the
//! `@bun-cjs` pragma and wrapper), the chunks in the written order, each
//! chunk's imports and exports gone and every name its linked one.

use super::analyse::{Chunk, Declarator, Edit, Imported, Spec, Stmt, StmtKind};
use super::error::LinkError;
use super::graph::{Order, Placement, Target, resolve_export, resolve_sym};
use super::names::Names;
use super::{ChunkSpan, LinkRun};

pub(super) struct Plan<'a> {
    pub chunks: &'a [Chunk],
    pub order: &'a Order,
    pub placement: &'a Placement,
    pub names: &'a Names,
    /// Per chunk, the chunks whose init its init calls (`graph::init_calls`).
    pub init_calls: &'a [Vec<usize>],
    /// Per chunk, the edits that replace its embedded-file uses
    /// (`embedded::resolve`).
    pub embedded: &'a [Vec<Edit>],
}

fn is_ident(s: &str) -> bool {
    let mut ch = s.chars();
    ch.next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && ch.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

// default: legacy, untriaged: a Result error becomes a value here
#[allow(clippy::disallowed_methods)]
fn key(name: &str) -> String {
    if is_ident(name) {
        name.to_string()
    } else {
        serde_json::to_string(name).unwrap_or_default()
    }
}

/// Linked text being written, with the runs copied unedited from a chunk.
#[derive(Default)]
struct Buf {
    text: String,
    /// `(offset in text, offset in the chunk, length, edited)`.
    runs: Vec<(u32, u32, u32, bool)>,
}

impl Buf {
    fn lit(&mut self, s: &str) {
        self.text.push_str(s);
    }

    /// Linker text that stands for the chunk bytes at `src`.
    fn lit_at(&mut self, s: &str, src: usize) {
        if !s.is_empty() {
            self.runs
                .push((self.text.len() as u32, src as u32, s.len() as u32, true));
        }
        self.text.push_str(s);
    }

    fn copy(&mut self, text: &str, from: usize, to: usize) {
        if to > from {
            self.runs.push((
                self.text.len() as u32,
                from as u32,
                (to - from) as u32,
                false,
            ));
            self.text.push_str(&text[from..to]);
        }
    }

    /// `text[range]` with the (sorted, disjoint) edits inside it applied.
    fn src(&mut self, text: &str, range: (usize, usize), edits: &[Edit]) {
        let lo = edits.partition_point(|e| e.start < range.0);
        let mut pos = range.0;
        for e in edits[lo..].iter().take_while(|e| e.start < range.1) {
            self.copy(text, pos, e.start);
            self.lit_at(&e.text, e.start);
            pos = e.end;
        }
        self.copy(text, pos, range.1);
    }

    fn append(&mut self, other: Buf) {
        let base = self.text.len() as u32;
        self.runs
            .extend(other.runs.iter().map(|r| (r.0 + base, r.1, r.2, r.3)));
        self.text.push_str(&other.text);
    }
}

fn render(text: &str, range: (usize, usize), edits: &[Edit]) -> String {
    let mut buf = Buf::default();
    buf.src(text, range, edits);
    buf.text
}

/// The program: pragma, wrapper, the entry's header comments, the
/// `__esm` helper when any chunk has an init, every chunk, the wrapper's
/// close.
pub(super) fn program(
    plan: &Plan<'_>,
) -> Result<(String, Vec<LinkRun>, Vec<ChunkSpan>), LinkError> {
    let entry = &plan.chunks[0];
    let first_line = entry.text.lines().next().unwrap_or("");
    let first_stmt = entry
        .stmts
        .first()
        .map_or(entry.text.len(), |s| s.span.start as usize);
    let mut out = format!(
        "{} @bun-cjs\n(function(exports, require, module, __filename, __dirname) {{",
        first_line.trim_end()
    );
    out.push_str(entry.text[first_line.len()..first_stmt].trim_start_matches('\n'));
    if plan.names.of.keys().any(|t| matches!(t, Target::Ns(_))) {
        out.push_str(&format!(
            "var {}=(target,all)=>{{for(var name in all)Object.defineProperty(target,name,{{get:all[name],enumerable:!0,configurable:!0}})}};\n",
            plan.names.export_helper
        ));
    }
    if !plan.names.init.is_empty() {
        out.push_str(&format!(
            "var {}=(fn,res)=>()=>(fn&&(res=fn(fn=0)),res);\n",
            plan.names.esm_helper
        ));
    }
    let (mut runs, mut spans) = (Vec::new(), Vec::new());
    for &c in &plan.order.chunks {
        let buf = chunk(plan, c)?;
        let base = out.len() as u32;
        runs.extend(buf.runs.iter().map(|r| LinkRun {
            out: r.0 + base,
            chunk: c as u32,
            src: r.1,
            len: r.2,
            edited: r.3,
        }));
        out.push_str(&buf.text);
        spans.push(ChunkSpan {
            out: base,
            end: out.len() as u32,
            chunk: c as u32,
        });
        out.push('\n');
    }
    out.push_str("})\n");
    Ok((out, runs, spans))
}

/// The edits that rename a chunk's bindings to their linked names and
/// replace its lazy loads.
fn edits(plan: &Plan<'_>, c: usize) -> Result<Vec<Edit>, LinkError> {
    let chunk = &plan.chunks[c];
    let removed: Vec<(u32, u32)> = chunk
        .stmts
        .iter()
        .filter(|s| matches!(s.kind, StmtKind::Removed))
        .map(|s| (s.span.start, s.span.end))
        .collect();
    let kept = |start: u32| !removed.iter().any(|r| start >= r.0 && start < r.1);
    let mut out = Vec::new();
    for s in &chunk.syms {
        let target = resolve_sym(plan.chunks, c, s.id)?;
        let new = plan.names.name(target);
        if new == s.name {
            continue;
        }
        let own = target == Target::Sym(c, s.id);
        let sites = own.then_some(&s.decls).into_iter().flatten().chain(&s.refs);
        for site in sites.filter(|site| kept(site.span.start)) {
            let text = if site.shorthand {
                format!("{}: {new}", s.name)
            } else {
                new.to_string()
            };
            out.push(Edit {
                start: site.span.start as usize,
                end: site.span.end as usize,
                text,
            });
        }
    }
    for edge in &chunk.lazy {
        let (start, end) = (edge.span.start as usize, edge.span.end as usize);
        out.retain(|e| e.start < start || e.end > end);
        out.push(Edit {
            start,
            end,
            text: lazy_load(plan, edge.target, edge.sync),
        });
    }
    out.extend(chunk.meta_edits.iter().cloned());
    for e in &plan.embedded[c] {
        // A text read replaces its whole call: the callee's edits go.
        out.retain(|x| x.start < e.start || x.end > e.end);
        out.push(e.clone());
    }
    for &(start, decl) in &chunk.export_prefixes {
        out.push(Edit {
            start: start as usize,
            end: decl as usize,
            text: String::new(),
        });
    }
    out.sort();
    out.dedup();
    let out = drop_require_aliases(plan, c, out);
    if let Some(w) = out.windows(2).find(|w| w[0].end > w[1].start) {
        return Err(LinkError::OverlappingEdits {
            path: chunk.path.clone(),
            at: w[1].start,
        });
    }
    Ok(out)
}

fn is_dropped(plan: &Plan<'_>, c: usize, d: &Declarator) -> bool {
    d.require_alias
        && d.names
            .first()
            .is_some_and(|s| plan.names.require_aliases.contains(&(c, *s)))
}

/// A kept `var` statement with dropped `import.meta.require` aliases is
/// rewritten to its other declarators (or removed when none is left).
/// Moved statements drop theirs in [`statement`].
fn drop_require_aliases(plan: &Plan<'_>, c: usize, mut edits: Vec<Edit>) -> Vec<Edit> {
    let chunk = &plan.chunks[c];
    for (i, st) in chunk.stmts.iter().enumerate() {
        let StmtKind::Var(ds) = &st.kind else {
            continue;
        };
        if plan.placement.moved[c][i] || !ds.iter().any(|d| is_dropped(plan, c, d)) {
            continue;
        }
        let (Some(first), Some(last)) = (ds.first(), ds.last()) else {
            continue;
        };
        let end_of = |d: &Declarator| d.init.map_or(d.pattern.end, |s| s.end) as usize;
        let kept: Vec<String> = ds
            .iter()
            .filter(|d| !is_dropped(plan, c, d))
            .map(|d| render(&chunk.text, (d.pattern.start as usize, end_of(d)), &edits))
            .collect();
        let (lo, hi, text) = if kept.is_empty() {
            (st.span.start as usize, st.span.end as usize, String::new())
        } else {
            (first.pattern.start as usize, end_of(last), kept.join(","))
        };
        edits.retain(|e| e.end <= lo || e.start >= hi);
        edits.push(Edit {
            start: lo,
            end: hi,
            text,
        });
    }
    edits.sort();
    edits
}

/// `import(<chunk>)` → `Promise.resolve().then(() => (init(), ns))`, what
/// Bun writes for an `import()` inside one file; `import.meta.require(
/// <chunk>)` → `(init(), ns)`.
fn lazy_load(plan: &Plan<'_>, target: usize, sync: bool) -> String {
    let ns = plan.names.name(Target::Ns(target));
    let value = match plan.names.init.get(&target) {
        Some(init) => format!("({init}(), {ns})"),
        None => ns.to_string(),
    };
    if sync {
        value
    } else {
        format!("Promise.resolve().then(() => {value})")
    }
}

/// One chunk: its namespace object and kept statements at the top level,
/// then its init: the inits of the lazy chunks it imports, its external
/// loads, its moved statements (ESM: a module's imports are evaluated
/// before its body).
fn chunk(plan: &Plan<'_>, c: usize) -> Result<Buf, LinkError> {
    let ch = &plan.chunks[c];
    let edits = edits(plan, c)?;
    let mut top = Buf::default();
    let mut init = Buf::default();
    for comment in &ch.leading_comments {
        top.lit(comment);
        top.lit("\n");
    }
    let (declared, loads) = externals(plan, c);
    if plan.order.eager[c] {
        top.lit(&eager_loads(&declared, &loads));
    } else {
        let names: Vec<&str> = declared
            .iter()
            .map(String::as_str)
            .filter(|d| !d.is_empty())
            .collect();
        if !names.is_empty() {
            top.lit(&format!("var {};", names.join(",")));
        }
    }
    if let Some(ns) = plan.names.of.get(&Target::Ns(c)) {
        top.lit(&namespace(plan, c, ns)?);
    }
    if plan.names.init.contains_key(&c) {
        for d in &plan.init_calls[c] {
            if let Some(name) = plan.names.init.get(d) {
                init.lit(&format!("{name}();"));
            }
        }
        if !plan.order.eager[c] {
            init.lit(&lazy_loads(&declared, &loads));
        }
    }
    let mut prev_end = ch.stmts.first().map_or(0, |s| s.span.start as usize);
    for (i, st) in ch.stmts.iter().enumerate() {
        let range = (prev_end, st.span.end as usize);
        prev_end = st.span.end as usize;
        let out = Out {
            text: &ch.text,
            edits: &edits,
            range,
        };
        let at = (c, i);
        statement(plan, at, st, &out, (&mut top, &mut init))?;
    }
    if let Some(name) = plan.names.init.get(&c) {
        top.lit(&format!("var {name}={}(()=>{{", plan.names.esm_helper));
        top.append(init);
        top.lit("});");
    }
    Ok(top)
}

struct Out<'a> {
    text: &'a str,
    edits: &'a [Edit],
    /// The statement with the whitespace and comments before it.
    range: (usize, usize),
}

impl Out<'_> {
    fn all(&self, buf: &mut Buf) {
        buf.src(self.text, self.range, self.edits);
    }

    fn span(&self, buf: &mut Buf, s: oxc_span::Span) {
        buf.src(self.text, (s.start as usize, s.end as usize), self.edits);
    }
}

fn statement(
    plan: &Plan<'_>,
    (c, i): (usize, usize),
    st: &Stmt,
    out: &Out<'_>,
    (top, init): (&mut Buf, &mut Buf),
) -> Result<(), LinkError> {
    let moved = plan.placement.moved[c][i];
    let name = |s| plan.names.name(Target::Sym(c, s)).to_string();
    match &st.kind {
        StmtKind::Removed => {}
        StmtKind::DefaultExpr { expr, .. } => {
            let d = plan.names.name(Target::Default(c));
            let value = if moved {
                top.lit(&format!("var {d};"));
                init.lit_at(&format!("{d}="), expr.start as usize);
                init
            } else {
                top.lit_at(&format!("var {d}="), expr.start as usize);
                top
            };
            out.span(value, *expr);
            value.lit(";");
        }
        _ if !moved => out.all(top),
        StmtKind::Var(ds) => {
            let stays = &plan.placement.stays[c][i];
            let (kept, moved): (Vec<_>, Vec<_>) = ds
                .iter()
                .zip(stays)
                .filter(|(d, _)| !is_dropped(plan, c, d))
                .partition(|(_, s)| **s);
            let names: Vec<String> = moved
                .iter()
                .flat_map(|(d, _)| d.names.iter())
                .map(|s| name(*s))
                .collect();
            if !names.is_empty() {
                top.lit(&format!("var {};", names.join(",")));
            }
            for (k, (d, _)) in kept.iter().enumerate() {
                top.lit_at(if k == 0 { "var " } else { "," }, d.pattern.start as usize);
                declarator(d, out, top);
            }
            if !kept.is_empty() {
                top.lit(";");
            }
            let assigned: Vec<_> = moved.iter().filter(|(d, _)| d.init.is_some()).collect();
            for (k, (d, _)) in assigned.iter().enumerate() {
                if k > 0 {
                    init.lit(",");
                }
                assignment(d, out, init);
            }
            if !assigned.is_empty() {
                init.lit(";");
            }
        }
        StmtKind::Class { name: sym, .. } => {
            let n = name(*sym);
            top.lit(&format!("var {n};"));
            init.lit_at(&format!("{n}="), st.span.start as usize);
            out.span(init, st.span);
            init.lit(";");
        }
        _ => out.all(init),
    }
    Ok(())
}

/// A declarator as written (with its edits).
fn declarator(d: &Declarator, out: &Out<'_>, buf: &mut Buf) {
    let end = d.init.map_or(d.pattern.end, |s| s.end);
    out.span(buf, oxc_span::Span::new(d.pattern.start, end));
}

/// A moved declarator as an assignment to its hoisted names.
fn assignment(d: &Declarator, out: &Out<'_>, buf: &mut Buf) {
    let Some(value) = d.init else {
        return;
    };
    if !d.simple {
        buf.lit_at("(", d.pattern.start as usize);
    }
    out.span(buf, d.pattern);
    buf.lit("=");
    out.span(buf, value);
    if !d.simple {
        buf.lit(")");
    }
}

/// A chunk's imports of anything that is not a chunk, as `require`s at
/// its top (what Bun's CommonJS output does with a Node built-in).
///
/// Returns the bindings and, beside each, the expression that loads it
/// (an empty binding: a bare `require(m)` for a side-effect import). An
/// eager chunk declares them at its top; a lazy one hoists the names and
/// loads them in its init.
// default: legacy, untriaged: a Result error becomes a value here
#[allow(clippy::disallowed_methods)]
fn externals(plan: &Plan<'_>, c: usize) -> (Vec<String>, Vec<String>) {
    let (mut declared, mut loads) = (Vec::new(), Vec::new());
    if let Some(init) = to_esm_init(plan, c) {
        declared.push(String::new());
        loads.push(format!("{init}()"));
    }
    for (k, d) in plan.chunks[c].imports.iter().enumerate() {
        let Spec::External(spec) = &d.spec else {
            continue;
        };
        let q = serde_json::to_string(spec).unwrap_or_default();
        let Some(ext) = plan.names.external.get(&(c, k)) else {
            declared.push(String::new());
            loads.push(format!("require({q})"));
            continue;
        };
        if let Some(d) = &ext.default {
            declared.push(d.clone());
            loads.push(match &ext.to_esm {
                Some(h) => format!("{h}(require({q}))"),
                None => format!("{{default:require({q})}}"),
            });
        }
        if let Some(ns) = &ext.ns {
            declared.push(ns.clone());
            loads.push(format!("require({q})"));
        }
    }
    (declared, loads)
}

/// The init a chunk must run before its external loads call `__toESM`:
/// the helper's chunk's, when that chunk is lazy and not one this chunk
/// imports (whose inits run first anyway). ESM never calls the helper
/// here — the linker does, so the linker orders it.
fn to_esm_init<'p>(plan: &'p Plan<'_>, c: usize) -> Option<&'p str> {
    let uses = plan.chunks[c].imports.iter().enumerate().any(|(k, _)| {
        plan.names
            .external
            .get(&(c, k))
            .is_some_and(|e| e.to_esm.is_some())
    });
    let h = plan.names.to_esm_chunk?;
    if !uses || plan.order.eager[h] || plan.chunks[c].static_deps.contains(&h) {
        return None;
    }
    plan.names.init.get(&h).map(String::as_str)
}

/// An eager chunk's external loads, as declarations at its top.
fn eager_loads(declared: &[String], loads: &[String]) -> String {
    declared
        .iter()
        .zip(loads)
        .map(|(d, l)| {
            if d.is_empty() {
                format!("{l};")
            } else {
                format!("var {d}={l};")
            }
        })
        .collect()
}

/// A lazy chunk's external loads, as its init's first statements.
fn lazy_loads(declared: &[String], loads: &[String]) -> String {
    declared
        .iter()
        .zip(loads)
        .map(|(d, l)| {
            if d.is_empty() {
                format!("{l};")
            } else {
                format!("{d}={l};")
            }
        })
        .collect()
}

/// A chunk's namespace object, Bun's way: `var ns={};__export(ns,{name:
/// ()=>binding,…})` — a live getter per export.
fn namespace(plan: &Plan<'_>, c: usize, ns: &str) -> Result<String, LinkError> {
    let mut props = Vec::new();
    for (name, _) in &plan.chunks[c].exports {
        let t = resolve_export(plan.chunks, c, &Imported::Name(name.clone()))?;
        props.push(format!("{}:()=>{}", key(name), plan.names.name(t)));
    }
    Ok(format!(
        "var {ns}={{}};{}({ns},{{{}}});",
        plan.names.export_helper,
        props.join(",")
    ))
}
