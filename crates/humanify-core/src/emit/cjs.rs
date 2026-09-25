//! Runnable CommonJS emission for a stable-split tree (exp026/exp027) —
//! TS `src/split/cjs-emit.ts`.
//!
//! The review tree is byte-exact statement slices; this is the RUNNABLE
//! form. Cross-file references become `const <alias> = require("./x.js")`
//! plus `<alias>.x` at every site (form-aware: a bare callee becomes
//! `(0, <alias>.x)(…)`, `delete x` becomes `false`, a shorthand property
//! `x: <alias>.x`, a cross-file `var x = e` redeclaration the setter
//! assignment); declaring files export live accessors BEFORE their require
//! header; the wrapper's module context routes through
//! `.humanify/_bundle.js`; an `index.js` entry loads every file in
//! first-statement order. Every rewrite is a byte-range splice of the ONE
//! parse's reference nodes — never a re-parse, never name matching.
//!
//! The emitter DECLINES (an `Err` with the TS's reason) where the input
//! cannot be represented faithfully: cross-file function redeclaration,
//! redeclaration through a destructuring declarator, a redeclared wrapper
//! parameter, a load-time require cycle, an unsupported redeclaration
//! position. The caller then ships the review tree.
//!
//! Scope questions go to [`BabelScopes`] (Babel's model, WP3.1): the
//! wrapper scope's bindings in `Object.keys` order, their reference paths
//! and constant violations. Path questions go to [`babel`].

mod babel;

use std::collections::{HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::{
    BindingPattern, Expression, ForStatementInit, ForStatementLeft, FunctionBody, Statement,
    VariableDeclaration, VariableDeclarationKind,
};
use oxc_semantic::{AstNodes, NodeId, Semantic};
use oxc_span::{GetSpan, Span};
use sha2::{Digest, Sha256};

use humanify_model::js::cmp_utf16;

use crate::babel_view::unparen;
use crate::place::layout::METADATA_DIR;
use crate::rename::validated::scopes::{BabelScopes, BindingId, BindingKind, SiteType};
use crate::rename::validated::target::is_valid_rename_target;

use super::align::{AlignSwitches, align_file_statements, alignment_key};
use super::load_order::LoadOrderFacts;
use super::paths::compute_relative_import_path;

use babel::{
    WriteKind, classify_write, delete_of, is_bare_callee, is_load_time_site, is_shorthand_value,
    parent_is_declarator, parent_is_delete, this_belongs_to_wrapper,
};

// ---------------------------------------------------------------------------
// The wrapper
// ---------------------------------------------------------------------------

/// The wrapper function as the emit reads it (`WrapperFunctionResult`).
pub struct WrapperView<'a> {
    /// The Function / ArrowFunctionExpression node (the load-time and
    /// `this` classification boundary).
    pub node: NodeId,
    /// Positional parameters: the name when the param is a plain
    /// Identifier (Babel's param list includes a rest element).
    pub params: Vec<Option<String>>,
    pub body: &'a FunctionBody<'a>,
}

/// Locate the wrapper function node whose span `find_wrapper_function`
/// reported.
pub fn wrapper_view<'a>(semantic: &Semantic<'a>, span: Span) -> Option<WrapperView<'a>> {
    for node in semantic.nodes().iter() {
        match node.kind() {
            AstKind::Function(f) if f.span == span => {
                let mut params: Vec<Option<String>> = f
                    .params
                    .items
                    .iter()
                    .map(|p| match (&p.pattern, &p.initializer) {
                        (BindingPattern::BindingIdentifier(id), None) => Some(id.name.to_string()),
                        _ => None,
                    })
                    .collect();
                if f.params.rest.is_some() {
                    params.push(None);
                }
                return f.body.as_deref().map(|body| WrapperView {
                    node: node.id(),
                    params,
                    body,
                });
            }
            AstKind::ArrowFunctionExpression(a) if a.span == span => {
                let mut params: Vec<Option<String>> = a
                    .params
                    .items
                    .iter()
                    .map(|p| match (&p.pattern, &p.initializer) {
                        (BindingPattern::BindingIdentifier(id), None) => Some(id.name.to_string()),
                        _ => None,
                    })
                    .collect();
                if a.params.rest.is_some() {
                    params.push(None);
                }
                return match &a.body {
                    oxc_ast::ast::ArrowFunctionBody::FunctionBody(body) => Some(WrapperView {
                        node: node.id(),
                        params,
                        body,
                    }),
                    _ => None,
                };
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Inputs and outputs
// ---------------------------------------------------------------------------

/// Everything `emitRunnableCjs(code, ledger, wrapper, prior)` reads.
pub struct RunnableInput<'a, 's> {
    pub code: &'a str,
    pub semantic: &'s Semantic<'a>,
    pub scopes: &'s BabelScopes,
    pub wrapper: &'s WrapperView<'a>,
    /// `ledger.files` (the pipeline's are sorted by UTF-16 code units).
    pub files: &'s [String],
    /// `ledger.order`: each statement's file, bundle order.
    pub order: &'s [String],
    /// `ledger.emitHashes` / `emitNames` — the review split's layout.
    pub emit_hashes: &'s [String],
    pub emit_names: &'s [Option<String>],
    /// `prior.aliases`.
    pub prior_aliases: Option<&'s HashMap<String, String>>,
    /// `statementHash` / `statementAlignName` per bundle statement.
    pub bundle_hashes: &'s [String],
    pub bundle_names: &'s [Option<String>],
    pub facts: &'s [LoadOrderFacts],
    pub switches: AlignSwitches,
}

/// The emitted tree plus what the ledger records from it.
pub struct RunnableTree {
    /// Relative path → content, in the TS Map's insertion order (ledger
    /// files, then the bundle runtime, then the entry).
    pub files: Vec<(String, String)>,
    /// file → alias (`ledger.aliases`), sorted as the TS writes it.
    pub aliases: Vec<(String, String)>,
    /// Per file (first-statement order), the bundle statement indexes in
    /// emitted order (`stmtIdxsByFile`).
    pub layout: Vec<(String, Vec<usize>)>,
    /// The layout per ledger slot (`recordEmittedLayout`).
    pub emit_hashes: Vec<String>,
    pub emit_names: Vec<Option<String>>,
    pub emit_indexes: Vec<usize>,
}

// ---------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------

/// One byte-range replacement inside the bundle text.
#[derive(Clone, Debug)]
struct Edit {
    start: u32,
    end: u32,
    text: String,
}

struct CrossBinding {
    decl_file: usize,
    /// Written from a file other than its own — needs a setter.
    writable: bool,
}

struct BundleContext {
    var_name: String,
    files: HashSet<usize>,
    file_name: String,
}

/// An insertion-ordered `Map<K, Set<V>>`.
#[derive(Default)]
struct OrderedSets {
    keys: Vec<usize>,
    sets: HashMap<usize, (Vec<usize>, HashSet<usize>)>,
}

impl OrderedSets {
    fn add(&mut self, key: usize, value: usize) {
        let entry = self.sets.entry(key).or_insert_with(|| {
            self.keys.push(key);
            (Vec::new(), HashSet::new())
        });
        if entry.1.insert(value) {
            entry.0.push(value);
        }
    }

    fn get(&self, key: usize) -> &[usize] {
        self.sets.get(&key).map_or(&[], |(v, _)| v.as_slice())
    }
}

struct Plan<'a, 's> {
    input: &'s RunnableInput<'a, 's>,
    nodes: &'s AstNodes<'a>,
    statements: &'a [Statement<'a>],
    ranges: Vec<(u32, u32)>,
    /// statement index → owning file id (relocated).
    stmt_file: Vec<usize>,
    /// name → cross-file binding (only bindings that cross).
    cross: HashMap<String, CrossBinding>,
    /// statement index → edits (first one per start offset wins).
    edits: Vec<Vec<Edit>>,
    /// statement index → cross-file var redeclarations (id start, name),
    /// insertion order.
    redecls: Vec<Vec<(u32, String)>>,
    /// declaring file → names exported via accessor.
    exports: HashMap<usize, HashSet<String>>,
    /// reader file → declaring files it requires.
    requires: HashMap<usize, HashSet<usize>>,
    /// file id → namespace variable.
    ns_vars: Vec<String>,
    directives: Vec<String>,
    load_time_edges: OrderedSets,
    bundle_context: Option<BundleContext>,
    /// (start, end) → identifier node (IdentifierReference /
    /// BindingIdentifier): violation targets come back as spans.
    ident_at: HashMap<(u32, u32), NodeId>,
    /// name → the binding `getBinding` resolves from the wrapper scope.
    lookup: HashMap<String, BindingId>,
}

/// Binary-search the top-level statement index containing a position.
fn stmt_index_of(ranges: &[(u32, u32)], pos: u32) -> Option<usize> {
    let (mut lo, mut hi) = (0i64, ranges.len() as i64 - 1);
    while lo <= hi {
        let m = ((lo + hi) >> 1) as usize;
        let (start, end) = ranges[m];
        if pos < start {
            hi = m as i64 - 1;
        } else if pos >= end {
            lo = m as i64 + 1;
        } else {
            return Some(m);
        }
    }
    None
}

impl Plan<'_, '_> {
    fn add_edit(&mut self, stmt: usize, edit: Edit) {
        if !self.edits[stmt].iter().any(|e| e.start == edit.start) {
            self.edits[stmt].push(edit);
        }
    }

    fn file_at(&self, pos: u32) -> Option<(usize, usize)> {
        stmt_index_of(&self.ranges, pos).map(|i| (i, self.stmt_file[i]))
    }

    fn wrapper_scope_bindings(&self) -> &[(String, BindingId)] {
        let sid = self.input.scopes.scope_of_node(self.input.wrapper.node);
        &self.input.scopes.initial_maps[sid.0 as usize]
    }

    /// `scope.getBinding(name)` from the wrapper scope (its own map, then
    /// its ancestors').
    fn get_binding(&self, name: &str) -> Option<BindingId> {
        self.lookup.get(name).copied()
    }

    /// The identifier node behind a violation target span.
    fn ident(&self, span: Span) -> Option<NodeId> {
        self.ident_at.get(&(span.start, span.end)).copied()
    }

    fn slice(&self, start: u32, end: u32) -> &str {
        &self.input.code[start as usize..end as usize]
    }

    // -- edits -------------------------------------------------------------------

    /// `editForTarget`: one cross-file READ, form-aware.
    fn edit_for_target(&self, id: NodeId, span: Span, name: &str, target: &str) -> Edit {
        let text = if is_shorthand_value(self.nodes, id, span) {
            format!("{name}: {target}")
        } else if is_bare_callee(self.nodes, id, span) {
            format!("(0, {target})")
        } else {
            target.to_string()
        };
        Edit {
            start: span.start,
            end: span.end,
            text,
        }
    }

    /// `editForWriteTarget`.
    fn edit_for_write_target(&self, id: NodeId, span: Span, name: &str, target: &str) -> Edit {
        let text = if is_shorthand_value(self.nodes, id, span) {
            format!("{name}: {target}")
        } else {
            target.to_string()
        };
        Edit {
            start: span.start,
            end: span.end,
            text,
        }
    }

    // -- bindings ------------------------------------------------------------------

    /// `recordCrossSite`.
    fn record_cross_site(&mut self, stmt: usize, name: &str, decl_file: usize, site: NodeId) {
        self.exports
            .entry(decl_file)
            .or_default()
            .insert(name.to_string());
        let reader = self.stmt_file[stmt];
        self.requires.entry(reader).or_default().insert(decl_file);
        if is_load_time_site(self.nodes, site, self.input.wrapper.node) {
            self.load_time_edges.add(reader, decl_file);
        }
    }

    /// `planReads`: true when any read crossed.
    fn plan_reads(&mut self, name: &str, binding: BindingId, decl_file: usize, ns: &str) -> bool {
        let refs = self.input.scopes.binding(binding).refs.clone();
        let mut crosses = false;
        for site in refs {
            if site.ty != SiteType::Identifier {
                continue;
            }
            let Some((stmt, file)) = self.file_at(site.span.start) else {
                continue;
            };
            if file == decl_file {
                continue;
            }
            if let Some(unary) = delete_of(self.nodes, site.node, site.span) {
                // Neutralized in place — no namespace reference, no edge.
                self.add_edit(
                    stmt,
                    Edit {
                        start: unary.start,
                        end: unary.end,
                        text: "false".into(),
                    },
                );
                continue;
            }
            let edit = self.edit_for_target(site.node, site.span, name, &format!("{ns}.{name}"));
            self.add_edit(stmt, edit);
            self.record_cross_site(stmt, name, decl_file, site.node);
            crosses = true;
        }
        crosses
    }

    /// `planWriteTarget`: true when it crossed.
    fn plan_write_target(
        &mut self,
        name: &str,
        decl_file: usize,
        ns: &str,
        span: Span,
    ) -> Result<bool, String> {
        let Some((stmt, file)) = self.file_at(span.start) else {
            return Ok(false);
        };
        if file == decl_file {
            return Ok(false);
        }
        let Some(id) = self.ident(span) else {
            return Err(format!("runnable emit: no identifier node at {span:?}"));
        };
        if parent_is_delete(self.nodes, id) {
            // Neutralized by the read rewrite; never a write.
            return Ok(false);
        }
        match classify_write(self.nodes, id, span) {
            WriteKind::FnRedecl => {
                return Err(format!(
                    "runnable emit: cross-file function redeclaration of \"{name}\" cannot preserve hoisting"
                ));
            }
            WriteKind::PatternRedecl => {
                return Err(format!(
                    "runnable emit: cross-file redeclaration of \"{name}\" through a destructuring declarator is not supported"
                ));
            }
            WriteKind::VarRedecl => {
                let list = &mut self.redecls[stmt];
                match list.iter_mut().find(|(s, _)| *s == span.start) {
                    Some(entry) => entry.1 = name.to_string(),
                    None => list.push((span.start, name.to_string())),
                }
            }
            WriteKind::Write => {
                let edit = self.edit_for_write_target(id, span, name, &format!("{ns}.{name}"));
                self.add_edit(stmt, edit);
            }
        }
        self.record_cross_site(stmt, name, decl_file, id);
        Ok(true)
    }

    /// `planBinding`.
    fn plan_binding(&mut self, name: &str, binding: BindingId) -> Result<(), String> {
        let b = self.input.scopes.binding(binding);
        let Some(decl_stmt) = stmt_index_of(&self.ranges, b.id_span.start) else {
            return Ok(());
        };
        let decl_file = self.stmt_file[decl_stmt];
        let ns = self.ns_vars[decl_file].clone();
        let targets = b.violation_targets.clone();
        let read_crosses = self.plan_reads(name, binding, decl_file, &ns);
        let mut writable = false;
        for spans in targets {
            for span in spans {
                if self.plan_write_target(name, decl_file, &ns, span)? {
                    writable = true;
                }
            }
        }
        if read_crosses || writable {
            self.cross.insert(
                name.to_string(),
                CrossBinding {
                    decl_file,
                    writable,
                },
            );
        }
        Ok(())
    }

    // -- the wrapper's module context ------------------------------------------------

    /// `reserveBundleVar`.
    fn reserve_bundle_var(&self) -> String {
        let taken: HashSet<&str> = self.ns_vars.iter().map(String::as_str).collect();
        let mut v = "__bundle".to_string();
        let mut n = 2;
        while taken.contains(v.as_str()) || self.get_binding(&v).is_some() {
            v = format!("__bundle_{n}");
            n += 1;
        }
        v
    }

    /// `planContextBinding` (reads, then writes) for one wrapper param.
    fn plan_context_binding(
        &mut self,
        param: &str,
        target: &str,
        ctx_files: &mut HashSet<usize>,
    ) -> Result<(), String> {
        let Some(binding) = self.get_binding(param) else {
            return Ok(());
        };
        let b = self.input.scopes.binding(binding);
        if b.kind != BindingKind::Param {
            return Ok(());
        }
        let refs = b.refs.clone();
        let targets = b.violation_targets.clone();
        for site in refs {
            if site.ty != SiteType::Identifier {
                continue;
            }
            let Some((stmt, file)) = self.file_at(site.span.start) else {
                continue;
            };
            if let Some(unary) = delete_of(self.nodes, site.node, site.span) {
                self.add_edit(
                    stmt,
                    Edit {
                        start: unary.start,
                        end: unary.end,
                        text: "false".into(),
                    },
                );
                continue;
            }
            let edit = self.edit_for_target(site.node, site.span, param, target);
            self.add_edit(stmt, edit);
            ctx_files.insert(file);
        }
        for spans in targets {
            for span in spans {
                let Some((stmt, file)) = self.file_at(span.start) else {
                    continue;
                };
                let Some(id) = self.ident(span) else {
                    return Err(format!("runnable emit: no identifier node at {span:?}"));
                };
                if parent_is_declarator(self.nodes, id) {
                    return Err(format!(
                        "runnable emit: wrapper parameter \"{param}\" is redeclared"
                    ));
                }
                let edit = self.edit_for_write_target(id, span, param, target);
                self.add_edit(stmt, edit);
                ctx_files.insert(file);
            }
        }
        Ok(())
    }

    /// `planTopLevelThis`.
    fn plan_top_level_this(&mut self, var_name: &str, ctx_files: &mut HashSet<usize>) {
        let target = format!("{var_name}.thisArg");
        let wrapper = self.input.wrapper.node;
        let wrapper_span = self.nodes.get_node(wrapper).span();
        let this_nodes: Vec<(NodeId, Span)> = self
            .nodes
            .iter()
            .filter_map(|n| match n.kind() {
                AstKind::ThisExpression(t)
                    if t.span.start >= wrapper_span.start && t.span.end <= wrapper_span.end =>
                {
                    Some((n.id(), t.span))
                }
                _ => None,
            })
            .collect();
        for (id, span) in this_nodes {
            if !this_belongs_to_wrapper(self.nodes, id, wrapper) {
                continue;
            }
            let Some((stmt, file)) = self.file_at(span.start) else {
                continue;
            };
            let text = if is_bare_callee(self.nodes, id, span) {
                format!("(0, {target})")
            } else {
                target.clone()
            };
            self.add_edit(
                stmt,
                Edit {
                    start: span.start,
                    end: span.end,
                    text,
                },
            );
            ctx_files.insert(file);
        }
    }

    /// `planWrapperContext`.
    fn plan_wrapper_context(&mut self) -> Result<(), String> {
        const CONTEXT_PROPS: [&str; 5] = ["exports", "require", "module", "filename", "dirname"];
        let var_name = self.reserve_bundle_var();
        let mut ctx_files = HashSet::new();
        let params = self.input.wrapper.params.clone();
        for (i, param) in params.iter().enumerate() {
            let (Some(prop), Some(param)) = (CONTEXT_PROPS.get(i), param) else {
                continue;
            };
            self.plan_context_binding(param, &format!("{var_name}.{prop}"), &mut ctx_files)?;
        }
        self.plan_top_level_this(&var_name, &mut ctx_files);
        if !ctx_files.is_empty() {
            self.bundle_context = Some(BundleContext {
                var_name,
                files: ctx_files,
                file_name: String::new(),
            });
        }
        Ok(())
    }

    /// `assertLoadTimeAcyclic`.
    fn assert_load_time_acyclic(&self) -> Result<(), String> {
        #[derive(Clone, Copy, PartialEq)]
        enum Color {
            Gray,
            Black,
        }
        fn visit(
            plan: &Plan<'_, '_>,
            file: usize,
            colors: &mut HashMap<usize, Color>,
            stack: &mut Vec<usize>,
        ) -> Result<(), String> {
            colors.insert(file, Color::Gray);
            stack.push(file);
            for &dep in plan.load_time_edges.get(file) {
                match colors.get(&dep) {
                    Some(Color::Gray) => {
                        let at = stack.iter().position(|&f| f == dep).unwrap_or(0);
                        let mut cycle: Vec<&str> = stack[at..]
                            .iter()
                            .map(|&f| plan.input.files[f].as_str())
                            .collect();
                        cycle.push(plan.input.files[dep].as_str());
                        return Err(format!(
                            "runnable emit: load-time reference cycle: {}",
                            cycle.join(" -> ")
                        ));
                    }
                    Some(Color::Black) => {}
                    None => visit(plan, dep, colors, stack)?,
                }
            }
            stack.pop();
            colors.insert(file, Color::Black);
            Ok(())
        }
        let mut colors = HashMap::new();
        let mut stack = Vec::new();
        for &file in &self.load_time_edges.keys {
            if !colors.contains_key(&file) {
                visit(self, file, &mut colors, &mut stack)?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Aliases: the import scope and the namespace ladder
// ---------------------------------------------------------------------------

/// `camelFromSegments`: `a-b/c-d` → `aBCD`.
fn camel_from_segments(segments: &[&str]) -> String {
    let joined = segments.join("-");
    let mut out = String::new();
    for (i, w) in joined
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .enumerate()
    {
        let mut chars = w.chars();
        let first = chars.next().expect("non-empty word");
        if i == 0 {
            out.push(first.to_ascii_lowercase());
        } else {
            out.push(first.to_ascii_uppercase());
        }
        out.push_str(chars.as_str());
    }
    out
}

/// `nsCandidates(file)`: the basename's camelCase, widening up the path,
/// then the sanitized path, then the path-hashed form (never collides).
fn ns_candidates(file: &str) -> Vec<String> {
    let stem = file.strip_suffix(".js").unwrap_or(file);
    let parts: Vec<&str> = stem.split('/').filter(|p| !p.is_empty()).collect();
    let mut out = Vec::new();
    for take in 1..=parts.len() {
        out.push(camel_from_segments(&parts[parts.len() - take..]));
    }
    // `file.replace(/[^A-Za-z0-9_$]/g, "_")` — per UTF-16 code unit.
    let sanitized: String = file
        .chars()
        .flat_map(|c| {
            let keep = c.is_ascii_alphanumeric() || c == '_' || c == '$';
            let n = if keep { 1 } else { c.len_utf16() };
            std::iter::repeat_n(if keep { c } else { '_' }, n)
        })
        .collect();
    let hash = Sha256::digest(file.as_bytes());
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    out.push(sanitized.clone());
    out.push(format!("{sanitized}_{}", &hex[..8]));
    out
}

/// Where an alias can collide (`ImportScope`): the names each importer
/// file uses in a binding or reference position.
struct ImportScope {
    always_taken: HashSet<String>,
    names_by_file: HashMap<usize, HashSet<String>>,
    importers: HashMap<usize, HashSet<usize>>,
}

impl ImportScope {
    fn is_shadowed(&self, decl_file: usize, name: &str) -> bool {
        if self.always_taken.contains(name) {
            return true;
        }
        self.importers.get(&decl_file).is_some_and(|imps| {
            imps.iter()
                .any(|i| self.names_by_file.get(i).is_some_and(|s| s.contains(name)))
        })
    }
}

/// `nsNameIsFree`: legal to bind (the renamer's own gate), unclaimed, and
/// not shadowed in any importing file.
fn ns_name_is_free(name: &str, claimed: &HashSet<String>, shadowed: bool) -> bool {
    is_valid_rename_target(name) && !claimed.contains(name) && !shadowed
}

/// `buildNsVars`: the prior's still-legal alias first, then the ladder,
/// tier by tier; a name contested at a tier goes to neither file.
fn build_ns_vars(
    files: &[String],
    scope: &ImportScope,
    prior_aliases: Option<&HashMap<String, String>>,
) -> Result<Vec<String>, String> {
    let candidates: Vec<Vec<String>> = files.iter().map(|f| ns_candidates(f)).collect();
    let mut vars: Vec<Option<String>> = vec![None; files.len()];
    let mut claimed: HashSet<String> = HashSet::new();
    let mut pending: Vec<usize> = (0..files.len()).collect();
    if let Some(prior) = prior_aliases {
        let mut wanted: HashMap<&str, usize> = HashMap::new();
        for &f in &pending {
            if let Some(a) = prior.get(&files[f]) {
                *wanted.entry(a.as_str()).or_insert(0) += 1;
            }
        }
        let mut next = Vec::new();
        for &f in &pending {
            let free = prior.get(&files[f]).filter(|a| {
                wanted.get(a.as_str()) == Some(&1)
                    && ns_name_is_free(a, &claimed, scope.is_shadowed(f, a))
            });
            match free {
                Some(a) => {
                    vars[f] = Some(a.clone());
                    claimed.insert(a.clone());
                }
                None => next.push(f),
            }
        }
        pending = next;
    }
    let max_tier = candidates.iter().map(Vec::len).max().unwrap_or(0);
    let mut tier = 0;
    while tier < max_tier && !pending.is_empty() {
        let mut wanted: HashMap<&str, usize> = HashMap::new();
        for &f in &pending {
            if let Some(c) = candidates[f].get(tier) {
                *wanted.entry(c.as_str()).or_insert(0) += 1;
            }
        }
        let mut next = Vec::new();
        for &f in &pending {
            match candidates[f].get(tier) {
                Some(c)
                    if !c.is_empty()
                        && wanted.get(c.as_str()) == Some(&1)
                        && ns_name_is_free(c, &claimed, scope.is_shadowed(f, c)) =>
                {
                    vars[f] = Some(c.clone());
                    claimed.insert(c.clone());
                }
                _ => next.push(f),
            }
        }
        pending = next;
        tier += 1;
    }
    if let Some(&f) = pending.first() {
        return Err(format!(
            "runnable emit: no free namespace variable for {}",
            files[f]
        ));
    }
    Ok(vars.into_iter().map(|v| v.expect("assigned")).collect())
}

/// Babel Identifier-node names per statement file (`identifierNamesByFile`):
/// every identifier in a binding or reference position, minus property
/// positions and the cross-file reads the emit itself rewrites.
fn identifier_names_by_file(
    nodes: &AstNodes<'_>,
    ranges: &[(u32, u32)],
    stmt_file: &[usize],
    rewritten: &HashSet<NodeId>,
) -> HashMap<usize, HashSet<String>> {
    let mut by_file: HashMap<usize, HashSet<String>> = HashMap::new();
    for &f in stmt_file {
        by_file.entry(f).or_default();
    }
    let mut add = |span: Span, name: &str| {
        if let Some(i) = stmt_index_of(ranges, span.start) {
            by_file
                .get_mut(&stmt_file[i])
                .expect("file")
                .insert(name.to_string());
        }
    };
    for node in nodes.iter() {
        match node.kind() {
            AstKind::IdentifierReference(r) => {
                if !rewritten.contains(&node.id()) {
                    add(r.span, &r.name);
                }
            }
            AstKind::BindingIdentifier(b) => add(b.span, &b.name),
            AstKind::LabelIdentifier(l) => add(l.span, &l.name),
            AstKind::PrivateIdentifier(p) => add(p.span, &p.name),
            AstKind::IdentifierName(n) => {
                let property = babel::parent(nodes, node.id()).is_some_and(|p| {
                    matches!(
                        nodes.kind(p),
                        AstKind::StaticMemberExpression(_)
                            | AstKind::ObjectProperty(_)
                            | AstKind::BindingProperty(_)
                            | AstKind::AssignmentTargetPropertyProperty(_)
                            | AstKind::MethodDefinition(_)
                            | AstKind::PropertyDefinition(_)
                    )
                });
                if !property {
                    add(n.span, &n.name);
                }
            }
            // Babel MetaProperty{meta, property}: two Identifier nodes.
            AstKind::ImportMeta(m) => {
                add(m.span, "import");
                add(m.span, "meta");
            }
            AstKind::NewTarget(m) => {
                add(m.span, "new");
                add(m.span, "target");
            }
            _ => {}
        }
    }
    by_file
}

// ---------------------------------------------------------------------------
// Namespace-augmentation relocation (the 2.1.172+ boot fix)
// ---------------------------------------------------------------------------

/// `isCopyPropsHelper(binding)`: `(target, source) => { for (k in source)
/// f(target, k, …) }`, shape-matched.
fn is_copy_props_helper(plan: &Plan<'_, '_>, binding: BindingId) -> bool {
    let nodes = plan.nodes;
    let path = plan.input.scopes.binding(binding).path_node;
    let (params, body_node) = match nodes.kind(path) {
        AstKind::VariableDeclarator(d) => match d.init.as_ref().map(unparen) {
            Some(Expression::ArrowFunctionExpression(a)) => (a.params.as_ref(), a.body.span()),
            Some(Expression::FunctionExpression(f)) => match &f.body {
                Some(b) => (f.params.as_ref(), b.span),
                None => return false,
            },
            _ => return false,
        },
        AstKind::Function(f) if f.is_declaration() => match &f.body {
            Some(b) => (f.params.as_ref(), b.span),
            None => return false,
        },
        _ => return false,
    };
    // Babel's params list counts a rest element; the first two must be
    // plain Identifiers.
    let count = params.items.len() + usize::from(params.rest.is_some());
    if count < 2 {
        return false;
    }
    let ident = |i: usize| match params.items.get(i) {
        Some(p) if p.initializer.is_none() => match &p.pattern {
            BindingPattern::BindingIdentifier(b) => Some(b.name.as_str()),
            _ => None,
        },
        _ => None,
    };
    let (Some(p0), Some(p1)) = (ident(0), ident(1)) else {
        return false;
    };
    let within = |s: Span, outer: Span| s.start >= outer.start && s.end <= outer.end;
    nodes.iter().any(|n| {
        let AstKind::ForInStatement(f) = n.kind() else {
            return false;
        };
        if !within(f.span, body_node) {
            return false;
        }
        if !matches!(unparen(&f.right), Expression::Identifier(r) if r.name == p1) {
            return false;
        }
        let loop_body = f.body.span();
        nodes.iter().any(|c| match c.kind() {
            AstKind::CallExpression(call) => {
                within(call.span, loop_body)
                    && !babel::is_babel_optional_call(call)
                    && call.arguments.len() >= 2
                    && matches!(
                        call.arguments[0].as_expression().map(unparen),
                        Some(Expression::Identifier(a)) if a.name == p0
                    )
            }
            _ => false,
        })
    })
}

/// `relocateNamespaceAugmentations`: each top-level copy-props call on a
/// bare identifier moves to the file that DEFINES its target.
fn relocate_namespace_augmentations(plan: &mut Plan<'_, '_>) {
    let mut helper_cache: HashMap<BindingId, bool> = HashMap::new();
    for idx in 0..plan.statements.len() {
        let Statement::ExpressionStatement(stmt) = &plan.statements[idx] else {
            continue;
        };
        let Expression::CallExpression(call) = unparen(&stmt.expression) else {
            continue;
        };
        if call.arguments.len() < 2 {
            continue;
        }
        let (Expression::Identifier(callee), Some(Expression::Identifier(target))) = (
            unparen(&call.callee),
            call.arguments[0].as_expression().map(unparen),
        ) else {
            continue;
        };
        let Some(helper) = plan.get_binding(&callee.name) else {
            continue;
        };
        let is_helper = match helper_cache.get(&helper) {
            Some(&v) => v,
            None => {
                let v = is_copy_props_helper(plan, helper);
                helper_cache.insert(helper, v);
                v
            }
        };
        if !is_helper {
            continue;
        }
        let Some(tb) = plan.get_binding(&target.name) else {
            continue;
        };
        let b = plan.input.scopes.binding(tb);
        if b.kind == BindingKind::Param {
            continue;
        }
        let Some(decl_idx) = stmt_index_of(&plan.ranges, b.id_span.start) else {
            continue;
        };
        if decl_idx > idx {
            continue;
        }
        plan.stmt_file[idx] = plan.stmt_file[decl_idx];
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// `applyEdits`: non-overlapping splices, right to left.
fn apply_edits(src: &str, base: u32, edits: &[Edit]) -> Result<String, String> {
    let mut sorted: Vec<&Edit> = edits.iter().collect();
    // Stable, descending by start (`(a, b) => b.start - a.start`).
    sorted.sort_by_key(|e| std::cmp::Reverse(e.start));
    let mut out = src.to_string();
    let mut prev_start = u32::MAX;
    for e in sorted {
        if e.end > prev_start {
            return Err("runnable emit: overlapping rewrites (internal)".into());
        }
        prev_start = e.start;
        let (s, t) = ((e.start - base) as usize, (e.end - base) as usize);
        out = format!("{}{}{}", &out[..s], e.text, &out[t..]);
    }
    Ok(out)
}

/// `takeWithin`: remove and return the pool edits inside [start, end).
fn take_within(pool: &mut Vec<Edit>, start: u32, end: u32) -> Vec<Edit> {
    let mut taken = Vec::new();
    let mut i = pool.len();
    while i > 0 {
        i -= 1;
        if pool[i].start >= start && pool[i].end <= end {
            taken.push(pool.remove(i));
        }
    }
    taken
}

fn declaration_kind(kind: VariableDeclarationKind) -> &'static str {
    match kind {
        VariableDeclarationKind::Var => "var",
        VariableDeclarationKind::Let => "let",
        VariableDeclarationKind::Const => "const",
        VariableDeclarationKind::Using => "using",
        VariableDeclarationKind::AwaitUsing => "await using",
    }
}

impl Plan<'_, '_> {
    fn ns_of_cross(&self, name: &str) -> Result<&str, String> {
        let c = self
            .cross
            .get(name)
            .ok_or_else(|| format!("runnable emit: no cross record for {name}"))?;
        Ok(&self.ns_vars[c.decl_file])
    }

    /// `assignmentFor`: `ns.name = <init>` (None for a bare redeclaration).
    fn assignment_for(
        &self,
        name: &str,
        init: Option<&Expression<'_>>,
        pool: &mut Vec<Edit>,
    ) -> Result<Option<String>, String> {
        let Some(init) = init else {
            return Ok(None);
        };
        let s = init.span();
        let taken = take_within(pool, s.start, s.end);
        let text = apply_edits(self.slice(s.start, s.end), s.start, &taken)?;
        Ok(Some(format!("{}.{name} = {text}", self.ns_of_cross(name)?)))
    }

    fn redecl_name(
        redecls: &[(u32, String)],
        d: &oxc_ast::ast::VariableDeclarator<'_>,
    ) -> Option<String> {
        match &d.id {
            BindingPattern::BindingIdentifier(id) => redecls
                .iter()
                .find(|(s, _)| *s == id.span.start)
                .map(|(_, n)| n.clone()),
            _ => None,
        }
    }

    /// `declStatementComposite`.
    fn decl_statement_composite(
        &self,
        decl: &VariableDeclaration<'_>,
        redecls: &[(u32, String)],
        pool: &mut Vec<Edit>,
    ) -> Result<Edit, String> {
        let mut lines = Vec::new();
        for d in &decl.declarations {
            if let Some(name) = Self::redecl_name(redecls, d) {
                if let Some(assign) = self.assignment_for(&name, d.init.as_ref(), pool)? {
                    lines.push(format!("{assign};"));
                }
            } else {
                let taken = take_within(pool, d.span.start, d.span.end);
                let text = apply_edits(self.slice(d.span.start, d.span.end), d.span.start, &taken)?;
                lines.push(format!("{} {text};", declaration_kind(decl.kind)));
            }
        }
        Ok(Edit {
            start: decl.span.start,
            end: decl.span.end,
            text: if lines.is_empty() {
                ";".into()
            } else {
                lines.join("\n")
            },
        })
    }

    /// `forInitComposite`.
    fn for_init_composite(
        &self,
        init: &VariableDeclaration<'_>,
        redecls: &[(u32, String)],
        pool: &mut Vec<Edit>,
    ) -> Result<Edit, String> {
        let mut parts = Vec::new();
        for d in &init.declarations {
            let Some(name) = Self::redecl_name(redecls, d) else {
                return Err("runnable emit: for-init mixes a cross-file var redeclaration with local declarators".into());
            };
            if let Some(assign) = self.assignment_for(&name, d.init.as_ref(), pool)? {
                parts.push(assign);
            }
        }
        Ok(Edit {
            start: init.span.start,
            end: init.span.end,
            text: parts.join(", "),
        })
    }

    /// `withRedeclComposites`.
    fn with_redecl_composites(
        &self,
        stmt: &Statement<'_>,
        redecls: &[(u32, String)],
        mut pool: Vec<Edit>,
    ) -> Result<Vec<Edit>, String> {
        let all_within = |s: Span| {
            redecls
                .iter()
                .all(|(start, _)| *start >= s.start && *start < s.end)
        };
        let composite = match stmt {
            Statement::VariableDeclaration(d) => {
                self.decl_statement_composite(d, redecls, &mut pool)?
            }
            Statement::ForStatement(f) if matches!(&f.init, Some(ForStatementInit::VariableDeclaration(d)) if all_within(d.span)) =>
            {
                let Some(ForStatementInit::VariableDeclaration(d)) = &f.init else {
                    unreachable!()
                };
                self.for_init_composite(d, redecls, &mut pool)?
            }
            Statement::ForOfStatement(_) | Statement::ForInStatement(_) => {
                let left = match stmt {
                    Statement::ForOfStatement(f) => &f.left,
                    Statement::ForInStatement(f) => &f.left,
                    _ => unreachable!(),
                };
                match left {
                    ForStatementLeft::VariableDeclaration(d) if all_within(d.span) => {
                        let name = &redecls[0].1;
                        Edit {
                            start: d.span.start,
                            end: d.span.end,
                            text: format!("{}.{name}", self.ns_of_cross(name)?),
                        }
                    }
                    _ => return Err(self.unsupported_redecl(redecls)),
                }
            }
            _ => return Err(self.unsupported_redecl(redecls)),
        };
        let mut out = vec![composite];
        out.extend(pool);
        Ok(out)
    }

    fn unsupported_redecl(&self, redecls: &[(u32, String)]) -> String {
        let mut names: Vec<&str> = Vec::new();
        for (_, n) in redecls {
            if !names.contains(&n.as_str()) {
                names.push(n);
            }
        }
        format!(
            "runnable emit: cross-file var redeclaration of \"{}\" in an unsupported position",
            names.join(", ")
        )
    }

    /// `stmtText`: byte-exact when untouched, spliced otherwise.
    fn stmt_text(&self, idx: usize) -> Result<String, String> {
        let (start, end) = self.ranges[idx];
        let edits = &self.edits[idx];
        let redecls = &self.redecls[idx];
        if edits.is_empty() && redecls.is_empty() {
            return Ok(self.slice(start, end).to_string());
        }
        let pool = edits.clone();
        let final_edits = if redecls.is_empty() {
            pool
        } else {
            self.with_redecl_composites(&self.statements[idx], redecls, pool)?
        };
        apply_edits(self.slice(start, end), start, &final_edits)
    }

    /// `accessorLine`.
    fn accessor_line(&self, name: &str) -> String {
        let writable = self.cross.get(name).is_some_and(|c| c.writable);
        let body = if writable {
            format!(
                "{{ get: () => {name}, set: v => {{ {name} = v; }}, enumerable: true, configurable: true }}"
            )
        } else {
            format!("{{ get: () => {name}, enumerable: true, configurable: true }}")
        };
        format!("Object.defineProperty(module.exports, \"{name}\", {body});")
    }

    /// `assembleFile`: directives, accessor block, require header, body.
    fn assemble_file(&self, file: usize, stmt_idxs: &[usize]) -> Result<String, String> {
        let path = self.input.files[file].as_str();
        let mut header: Vec<String> = self.directives.clone();
        if let Some(exps) = self.exports.get(&file) {
            let mut names: Vec<&String> = exps.iter().collect();
            names.sort_by(|a, b| cmp_utf16(a, b));
            for name in names {
                header.push(self.accessor_line(name));
            }
        }
        if let Some(bc) = &self.bundle_context
            && bc.files.contains(&file)
        {
            header.push(format!(
                "const {} = require(\"{}\");",
                bc.var_name,
                compute_relative_import_path(path, &bc.file_name)
            ));
        }
        if let Some(reqs) = self.requires.get(&file) {
            let mut decls: Vec<usize> = reqs.iter().copied().collect();
            decls.sort_by(|&a, &b| cmp_utf16(&self.input.files[a], &self.input.files[b]));
            for decl in decls {
                header.push(format!(
                    "const {} = require(\"{}\");",
                    self.ns_vars[decl],
                    compute_relative_import_path(path, &self.input.files[decl])
                ));
            }
        }
        let mut body: Vec<String> = Vec::with_capacity(stmt_idxs.len());
        for &idx in stmt_idxs {
            body.push(self.stmt_text(idx)?);
        }
        if header.is_empty() {
            self.neutralize_leading_string(stmt_idxs, &mut body);
        }
        let sections: Vec<String> = [header, body]
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.join("\n"))
            .collect();
        Ok(format!("{}\n", sections.join("\n\n")))
    }

    /// `neutralizeLeadingString`: a headerless file whose first statement
    /// is a bare string literal would promote it to a directive.
    fn neutralize_leading_string(&self, stmt_idxs: &[usize], body: &mut [String]) {
        let Some(&first) = stmt_idxs.first() else {
            return;
        };
        if let Statement::ExpressionStatement(e) = &self.statements[first]
            && let Expression::StringLiteral(s) = unparen(&e.expression)
        {
            body[0] = format!("({});", self.slice(s.span.start, s.span.end));
        }
    }

    /// `entrySource`.
    fn entry_source(&self, entry: &str) -> String {
        let mut lines: Vec<String> = self.directives.clone();
        lines.push("// Entry for the runnable split tree: loads every module in the".into());
        lines.push("// original bundle's first-statement order.".into());
        if let Some(bc) = &self.bundle_context {
            lines.push(format!(
                "const {} = require(\"{}\");",
                bc.var_name,
                compute_relative_import_path(entry, &bc.file_name)
            ));
            lines.push(format!(
                "{}.init(module, require, __filename, __dirname, this);",
                bc.var_name
            ));
            lines.push(String::new());
        }
        let mut seen = HashSet::new();
        let all: Vec<usize> = self
            .stmt_file
            .iter()
            .copied()
            .chain(0..self.input.files.len())
            .collect();
        for file in all {
            if seen.insert(file) {
                lines.push(format!(
                    "require(\"{}\");",
                    compute_relative_import_path(entry, &self.input.files[file])
                ));
            }
        }
        format!("{}\n", lines.join("\n"))
    }
}

const BUNDLE_RUNTIME: &str = "\"use strict\";

// Shared original-wrapper module context. index.js initializes it with
// the ENTRY module's require/module/__filename/__dirname/this, so every
// split file sees the single context the original bundle had. `exports`
// captures mod.exports at init time — matching the wrapper's `exports`
// parameter, which never retargets when module.exports is reassigned.
module.exports = {
  module: null,
  exports: null,
  require: null,
  filename: \"\",
  dirname: \"\",
  thisArg: null,
  init(mod, req, filename, dirname, thisArg) {
    this.module = mod;
    this.exports = mod.exports;
    this.require = req;
    this.filename = filename;
    this.dirname = dirname;
    this.thisArg = thisArg;
  }
};
";

/// `pickFreeFile`: prefix `_` until the name is free.
fn pick_free_file(name: &str, taken: &HashSet<String>) -> String {
    let mut v = name.to_string();
    while taken.contains(&v) {
        v = format!("_{v}");
    }
    v
}

// ---------------------------------------------------------------------------
// The entry point
// ---------------------------------------------------------------------------

/// Build the plan's scaffolding and the import scope, then the aliases.
fn new_plan<'a, 's>(input: &'s RunnableInput<'a, 's>) -> Result<Plan<'a, 's>, String> {
    let nodes = input.semantic.nodes();
    let statements: &'a [Statement<'a>] = &input.wrapper.body.statements;
    if input.order.len() != statements.len() {
        return Err(format!(
            "runnable emit: ledger.order has {} entries but the wrapper has {} statements",
            input.order.len(),
            statements.len()
        ));
    }
    let file_id: HashMap<&str, usize> = input
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.as_str(), i))
        .collect();
    let mut stmt_file = Vec::with_capacity(input.order.len());
    for f in input.order {
        let id = file_id.get(f.as_str()).ok_or_else(|| {
            format!("runnable emit: ledger.order references \"{f}\" missing from ledger.files")
        })?;
        stmt_file.push(*id);
    }
    let directives = input
        .wrapper
        .body
        .directives
        .iter()
        .map(|d| input.code[d.span.start as usize..d.span.end as usize].to_string())
        .collect();
    let mut ident_at = HashMap::new();
    for n in nodes.iter() {
        match n.kind() {
            AstKind::IdentifierReference(r) => {
                ident_at.insert((r.span.start, r.span.end), n.id());
            }
            AstKind::BindingIdentifier(b) => {
                ident_at.insert((b.span.start, b.span.end), n.id());
            }
            _ => {}
        }
    }
    // `getBinding` from the wrapper scope: the nearest scope wins, so
    // outer maps are laid down first and inner ones overwrite them.
    let mut chain = Vec::new();
    let mut cur = Some(input.scopes.scope_of_node(input.wrapper.node));
    while let Some(sid) = cur {
        chain.push(sid);
        cur = input.scopes.scope(sid).parent;
    }
    let mut lookup = HashMap::new();
    for sid in chain.into_iter().rev() {
        for (name, b) in &input.scopes.initial_maps[sid.0 as usize] {
            lookup.insert(name.clone(), *b);
        }
    }
    Ok(Plan {
        input,
        lookup,
        nodes,
        statements,
        ranges: statements
            .iter()
            .map(|s| (s.span().start, s.span().end))
            .collect(),
        stmt_file,
        cross: HashMap::new(),
        edits: vec![Vec::new(); statements.len()],
        redecls: vec![Vec::new(); statements.len()],
        exports: HashMap::new(),
        requires: HashMap::new(),
        ns_vars: Vec::new(),
        directives,
        load_time_edges: OrderedSets::default(),
        bundle_context: None,
        ident_at,
    })
}

/// `buildImportScope`.
fn build_import_scope(plan: &Plan<'_, '_>) -> ImportScope {
    let scopes = plan.input.scopes;
    let mut rewritten = HashSet::new();
    let mut importers: HashMap<usize, HashSet<usize>> = HashMap::new();
    for (_, bid) in plan.wrapper_scope_bindings() {
        let b = scopes.binding(*bid);
        if b.kind == BindingKind::Param {
            continue;
        }
        let Some(decl_stmt) = stmt_index_of(&plan.ranges, b.id_span.start) else {
            continue;
        };
        let decl_file = plan.stmt_file[decl_stmt];
        for site in &b.refs {
            if let Some((_, f)) = plan.file_at(site.span.start)
                && f != decl_file
            {
                if site.ty == SiteType::Identifier {
                    rewritten.insert(site.node);
                }
                importers.entry(decl_file).or_default().insert(f);
            }
        }
        for site in &b.violations {
            if let Some((_, f)) = plan.file_at(site.span.start)
                && f != decl_file
            {
                importers.entry(decl_file).or_default().insert(f);
            }
        }
    }
    ImportScope {
        always_taken: plan
            .input
            .wrapper
            .params
            .iter()
            .flatten()
            .cloned()
            .collect(),
        names_by_file: identifier_names_by_file(
            plan.nodes,
            &plan.ranges,
            &plan.stmt_file,
            &rewritten,
        ),
        importers,
    }
}

/// `orderedIndexesByFile`: each file's statements (relocated grouping,
/// bundle order) aligned to the ledger's emitted layout.
fn ordered_indexes_by_file(plan: &Plan<'_, '_>) -> Vec<(usize, Vec<usize>)> {
    let input = plan.input;
    let mut by_file: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut pos: HashMap<usize, usize> = HashMap::new();
    for (idx, &f) in plan.stmt_file.iter().enumerate() {
        let k = *pos.entry(f).or_insert_with(|| {
            by_file.push((f, Vec::new()));
            by_file.len() - 1
        });
        by_file[k].1.push(idx);
    }
    // `ledger.emitHashes ?? ledger.hashes`: absent on a ledger that never
    // recorded a layout — then bundle order stands.
    if input.switches.emit_align_disabled || input.emit_hashes.is_empty() {
        return by_file;
    }
    let target = input.emit_hashes;
    let prior_names = !input.switches.name_align_disabled && input.emit_names.len() == target.len();
    let keys: Vec<String> = if prior_names {
        input
            .bundle_hashes
            .iter()
            .zip(input.bundle_names)
            .map(|(h, n)| alignment_key(h, n.as_deref()))
            .collect()
    } else {
        input.bundle_hashes.to_vec()
    };
    let mut seq_by_file: HashMap<&str, Vec<String>> = HashMap::new();
    for (k, file) in input.order.iter().enumerate() {
        let key = if prior_names {
            alignment_key(&target[k], input.emit_names[k].as_deref())
        } else {
            target[k].clone()
        };
        seq_by_file.entry(file.as_str()).or_default().push(key);
    }
    for (f, idxs) in &mut by_file {
        let seq = seq_by_file.get(input.files[*f].as_str()).map(Vec::as_slice);
        *idxs = align_file_statements(idxs, &keys, seq, input.facts);
    }
    by_file
}

/// `recordEmittedLayout`: per ledger slot, the emitted statement.
fn record_emitted_layout(
    plan: &Plan<'_, '_>,
    by_file: &[(usize, Vec<usize>)],
) -> (Vec<String>, Vec<Option<String>>, Vec<usize>) {
    let input = plan.input;
    let queues: HashMap<&str, &[usize]> = by_file
        .iter()
        .map(|(f, idxs)| (input.files[*f].as_str(), idxs.as_slice()))
        .collect();
    let mut cursor: HashMap<&str, usize> = HashMap::new();
    let n = input.order.len();
    let (mut hashes, mut names, mut indexes) = (
        Vec::with_capacity(n),
        Vec::with_capacity(n),
        Vec::with_capacity(n),
    );
    for slot in 0..n {
        let file = input.order[slot].as_str();
        let at = cursor.get(file).copied().unwrap_or(0);
        let pick = match queues.get(file) {
            Some(q) if at < q.len() => {
                cursor.insert(file, at + 1);
                q[at]
            }
            _ => slot,
        };
        hashes.push(input.bundle_hashes[pick].clone());
        names.push(input.bundle_names[pick].clone());
        indexes.push(pick);
    }
    (hashes, names, indexes)
}

/// `emitRunnableCjs`: the runnable tree, or the decline reason.
/// A declined runnable emit (`tryEmitRunnableCjs`' `onDecline`): the
/// reason, and what the TS had ALREADY written onto the ledger the caller
/// persists when the throw came (finding #40) — the aliases are assigned
/// once the plan is built (`ledger.aliases = …` before
/// `planWrapperContext` / `assertLoadTimeAcyclic`), the emitted layout
/// once the tree is being assembled (`recordEmittedLayout` before the
/// per-file assembly).
#[derive(Debug)]
pub struct EmitDecline {
    pub reason: String,
    pub aliases: Option<Vec<(String, String)>>,
    pub layout: Option<Box<EmittedLayout>>,
}

/// `recordEmittedLayout`'s record: the layout per ledger slot, and the
/// per-file emitted statement order the dump's `emit.json` captures.
#[derive(Debug, Clone)]
pub struct EmittedLayout {
    pub emit_hashes: Vec<String>,
    pub emit_names: Vec<Option<String>>,
    pub emit_indexes: Vec<usize>,
    pub by_file: Vec<(String, Vec<usize>)>,
}

impl From<String> for EmitDecline {
    fn from(reason: String) -> Self {
        EmitDecline {
            reason,
            aliases: None,
            layout: None,
        }
    }
}

pub fn emit_runnable_cjs(input: &RunnableInput<'_, '_>) -> Result<RunnableTree, EmitDecline> {
    let mut plan = new_plan(input)?;
    // Runnable-form only: the shipped ledger keeps the original order.
    relocate_namespace_augmentations(&mut plan);
    let scope = build_import_scope(&plan);
    plan.ns_vars = build_ns_vars(input.files, &scope, input.prior_aliases)?;
    let mut aliases: Vec<(String, String)> = input
        .files
        .iter()
        .cloned()
        .zip(plan.ns_vars.iter().cloned())
        .collect();
    // `[...nsVars].sort()`: arrays compare as their "file,alias" strings.
    aliases.sort_by(|a, b| cmp_utf16(&format!("{},{}", a.0, a.1), &format!("{},{}", b.0, b.1)));
    let bindings: Vec<(String, BindingId)> = plan.wrapper_scope_bindings().to_vec();
    for (name, bid) in &bindings {
        if input.scopes.binding(*bid).kind == BindingKind::Param {
            continue;
        }
        plan.plan_binding(name, *bid)?;
    }
    // -- `ledger.aliases` is assigned here (buildPlan has returned) --------
    let with_aliases = |reason: String| EmitDecline {
        reason,
        aliases: Some(aliases.clone()),
        layout: None,
    };
    plan.plan_wrapper_context().map_err(with_aliases)?;
    plan.assert_load_time_acyclic().map_err(with_aliases)?;
    let by_file = ordered_indexes_by_file(&plan);
    let (emit_hashes, emit_names, emit_indexes) = record_emitted_layout(&plan, &by_file);
    let layout = || EmittedLayout {
        emit_hashes: emit_hashes.clone(),
        emit_names: emit_names.clone(),
        emit_indexes: emit_indexes.clone(),
        by_file: by_file
            .iter()
            .map(|(f, v)| (input.files[*f].clone(), v.clone()))
            .collect(),
    };

    let mut taken: HashSet<String> = input.files.iter().cloned().collect();
    if let Some(bc) = &mut plan.bundle_context {
        bc.file_name = pick_free_file(&format!("{METADATA_DIR}/_bundle.js"), &taken);
        taken.insert(bc.file_name.clone());
    }
    let entry = pick_free_file("index.js", &taken);
    let idxs_of: HashMap<usize, &[usize]> =
        by_file.iter().map(|(f, v)| (*f, v.as_slice())).collect();
    let mut files = Vec::with_capacity(input.files.len() + 2);
    for (f, path) in input.files.iter().enumerate() {
        let idxs = idxs_of.get(&f).copied().unwrap_or(&[]);
        // -- the emitted layout is recorded (recordEmittedLayout ran) ------
        let content = plan.assemble_file(f, idxs).map_err(|reason| EmitDecline {
            reason,
            aliases: Some(aliases.clone()),
            layout: Some(Box::new(layout())),
        })?;
        files.push((path.clone(), content));
    }
    if let Some(bc) = &plan.bundle_context {
        files.push((bc.file_name.clone(), BUNDLE_RUNTIME.to_string()));
    }
    files.push((entry.clone(), plan.entry_source(&entry)));
    Ok(RunnableTree {
        files,
        aliases,
        layout: by_file
            .into_iter()
            .map(|(f, v)| (input.files[f].clone(), v))
            .collect(),
        emit_hashes,
        emit_names,
        emit_indexes,
    })
}

#[cfg(test)]
mod cjs_test;
