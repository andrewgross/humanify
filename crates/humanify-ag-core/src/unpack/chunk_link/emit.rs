//! Writing the linked program: Bun's single-file CommonJS layout (the
//! `@bun-cjs` pragma and wrapper), the chunks in the written order, each
//! chunk's imports and exports gone and every name its linked one.

use super::analyse::{Chunk, Declarator, Edit, Imported, Spec, Stmt, StmtKind};
use super::graph::{Order, Target, resolve_export, resolve_sym};
use super::names::Names;

pub(super) struct Plan<'a> {
    pub chunks: &'a [Chunk],
    pub order: &'a Order,
    pub moved: &'a [Vec<bool>],
    pub names: &'a Names,
}

fn is_ident(s: &str) -> bool {
    let mut ch = s.chars();
    ch.next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && ch.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn key(name: &str) -> String {
    if is_ident(name) {
        name.to_string()
    } else {
        serde_json::to_string(name).unwrap_or_default()
    }
}

/// `text[range]` with the (sorted, disjoint) edits inside it applied.
fn render(text: &str, range: (usize, usize), edits: &[Edit]) -> String {
    let lo = edits.partition_point(|e| e.start < range.0);
    let mut out = String::new();
    let mut pos = range.0;
    for e in edits[lo..].iter().take_while(|e| e.start < range.1) {
        out.push_str(&text[pos..e.start]);
        out.push_str(&e.text);
        pos = e.end;
    }
    out.push_str(&text[pos..range.1]);
    out
}

/// The program: pragma, wrapper, the entry's header comments, the
/// `__esm` helper when any chunk has an init, every chunk, the wrapper's
/// close.
pub(super) fn program(plan: &Plan<'_>) -> Result<String, String> {
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
    for &c in &plan.order.chunks {
        out.push_str(&chunk(plan, c)?);
        out.push('\n');
    }
    out.push_str("})\n");
    Ok(out)
}

/// The edits that rename a chunk's bindings to their linked names and
/// replace its lazy loads.
fn edits(plan: &Plan<'_>, c: usize) -> Result<Vec<Edit>, String> {
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
    for st in &chunk.stmts {
        if let StmtKind::ExportPrefix(decl) = st.kind {
            out.push(Edit {
                start: st.span.start as usize,
                end: decl as usize,
                text: String::new(),
            });
        }
    }
    out.sort();
    out.dedup();
    let out = drop_require_aliases(plan, c, out);
    if let Some(w) = out.windows(2).find(|w| w[0].end > w[1].start) {
        return Err(format!(
            "{}: overlapping edits at {}",
            chunk.path.display(),
            w[1].start
        ));
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
        if plan.moved[c][i] || !ds.iter().any(|d| is_dropped(plan, c, d)) {
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

/// One chunk: its kept statements at the top level, then its init (the
/// moved statements, after the inits of the lazy chunks it imports), then
/// its namespace object.
fn chunk(plan: &Plan<'_>, c: usize) -> Result<String, String> {
    let ch = &plan.chunks[c];
    let edits = edits(plan, c)?;
    let mut top = String::new();
    let mut init = String::new();
    for comment in &ch.leading_comments {
        top.push_str(comment);
        top.push('\n');
    }
    top.push_str(&externals(plan, c));
    if let Some(ns) = plan.names.of.get(&Target::Ns(c)) {
        top.push_str(&namespace(plan, c, ns)?);
    }
    if plan.names.init.contains_key(&c) {
        for d in &ch.static_deps {
            if let Some(name) = plan.names.init.get(d).filter(|_| !plan.order.eager[*d]) {
                init.push_str(&format!("{name}();"));
            }
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
        statement(plan, c, st, plan.moved[c][i], &out, (&mut top, &mut init))?;
    }
    if let Some(name) = plan.names.init.get(&c) {
        top.push_str(&format!(
            "var {name}={}(()=>{{{init}}});",
            plan.names.esm_helper
        ));
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
    fn all(&self) -> String {
        render(self.text, self.range, self.edits)
    }

    fn span(&self, s: oxc_span::Span) -> String {
        render(self.text, (s.start as usize, s.end as usize), self.edits)
    }
}

fn statement(
    plan: &Plan<'_>,
    c: usize,
    st: &Stmt,
    moved: bool,
    out: &Out<'_>,
    (top, init): (&mut String, &mut String),
) -> Result<(), String> {
    let name = |s| plan.names.name(Target::Sym(c, s)).to_string();
    match &st.kind {
        StmtKind::Removed => {}
        StmtKind::DefaultExpr { expr, .. } => {
            let d = plan.names.name(Target::Default(c));
            if moved {
                top.push_str(&format!("var {d};"));
                init.push_str(&format!("{d}={};", out.span(*expr)));
            } else {
                top.push_str(&format!("var {d}={};", out.span(*expr)));
            }
        }
        _ if !moved => top.push_str(&out.all()),
        StmtKind::Var(ds) => {
            let ds: Vec<&Declarator> = ds.iter().filter(|d| !is_dropped(plan, c, d)).collect();
            if ds.is_empty() {
                return Ok(());
            }
            let names: Vec<String> = ds
                .iter()
                .flat_map(|d| d.names.iter())
                .map(|s| name(*s))
                .collect();
            top.push_str(&format!("var {};", names.join(",")));
            let parts: Vec<String> = ds.iter().filter_map(|d| assignment(d, out)).collect();
            if !parts.is_empty() {
                init.push_str(&format!("{};", parts.join(",")));
            }
        }
        StmtKind::Class { name: sym, .. } => {
            let n = name(*sym);
            top.push_str(&format!("var {n};"));
            init.push_str(&format!("{n}={};", out.span(st.span)));
        }
        _ => init.push_str(&out.all()),
    }
    Ok(())
}

/// A moved declarator as an assignment to its hoisted names.
fn assignment(d: &Declarator, out: &Out<'_>) -> Option<String> {
    let value = out.span(d.init?);
    let pattern = out.span(d.pattern);
    Some(if d.simple {
        format!("{pattern}={value}")
    } else {
        format!("({pattern}={value})")
    })
}

/// A chunk's imports of anything that is not a chunk, as `require`s at
/// its top (what Bun's CommonJS output does with a Node built-in).
fn externals(plan: &Plan<'_>, c: usize) -> String {
    let mut out = String::new();
    for (k, d) in plan.chunks[c].imports.iter().enumerate() {
        let Spec::External(spec) = &d.spec else {
            continue;
        };
        let q = serde_json::to_string(spec).unwrap_or_default();
        let Some(ext) = plan.names.external.get(&(c, k)) else {
            out.push_str(&format!("require({q});"));
            continue;
        };
        if let Some(d) = &ext.default {
            match &ext.to_esm {
                Some(h) => out.push_str(&format!("var {d}={h}(require({q}));")),
                None => out.push_str(&format!("var {d}={{default:require({q})}};")),
            }
        }
        if let Some(ns) = &ext.ns {
            out.push_str(&format!("var {ns}=require({q});"));
        }
    }
    out
}

/// A chunk's namespace object, Bun's way: `var ns={};__export(ns,{name:
/// ()=>binding,…})` — a live getter per export.
fn namespace(plan: &Plan<'_>, c: usize, ns: &str) -> Result<String, String> {
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
