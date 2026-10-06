//! Library imports the PIPELINE names, never the model (2026-10-06,
//! Andrew's decision on the repeated-import collisions).
//!
//! A bundle imports the same library many times into ONE scope: a Bun
//! build of Claude Code has 307 `X = require("path")` bindings in its
//! module scope (2.1.118), each lazy-init wrapper taking its own. Asked
//! of the model, every one of them wanted `path`/`pathModule`; the first
//! won and the rest walked `pathModule → pathUtil → pathLib → …` through
//! collision re-asks into the suffix ladder. In the 2026-10-06 fresh-run
//! investigation repeated library imports were 46% of the barrier's
//! exhausted re-asks; a 2.1.215 run asked 1,337 such bindings in first
//! rounds and 2,553 more times in re-asks (767 re-ask calls asked nothing
//! else).
//!
//! So a module binding whose ONLY write is exactly
//! `require("<package>")` (the declarator's init, or one plain `=`
//! assignment) is named from the specifier — `path` → `pathModule`,
//! `fs/promises` → `fsPromisesModule`, `node:` dropped — numbered
//! `pathModule2`, `pathModule3`, … in source order of the require call,
//! skipping names already held. It is applied before the first wave
//! (after the prior-version transfer), recorded on the trail under
//! [`Tier::LibraryImport`], and the binding is settled so no wave asks
//! for it. Nothing rewrites a model answer.
//!
//! Precision first:
//! - only a BARE specifier (a package or a Node built-in): a relative or
//!   absolute path (`./x.js`, `/$bunfs/…`) is an app module or an asset —
//!   the model names those;
//! - only the call itself: a wrapped require (`interop(require("x"), 1)`)
//!   or a binding written anywhere else is the model's;
//! - `require` must be the program's require (a free name, or a binding
//!   of the binding's own scope chain such as the CJS wrapper's
//!   parameter) — never a local function that happens to be called so;
//! - a binding the transfer already settled keeps its carried name
//!   (cross-version stability; the numbering skips it), and a pending one
//!   whose prior-version suggestion is of the same family (`pathModule7`)
//!   is offered that name first.

use oxc_ast::AstKind;
use oxc_ast::ast::{Argument, AssignmentOperator, AssignmentTarget, Expression};
use oxc_semantic::{Semantic, SymbolId};
use oxc_span::GetSpan;
use oxc_syntax::reference::ReferenceFlags;

use super::PlumbingNames;
use crate::babel_view::unparen;
use crate::graph::UnifiedGraph;
use crate::rename::transfer::lifecycle::Lifecycle;
use crate::rename::validated::scopes::BindingId;
use crate::rename::validated::target::is_valid_rename_target;
use crate::rename::validated::{RejectionReason, RenameRequest, RenameState, TrailSpec};
use crate::trail::{Attempt, Outcome, Tier};

#[cfg(test)]
mod library_imports_test;

/// How far the numbering walks before it gives up on a family (never
/// reached on a real bundle: the largest family is ~300).
const MAX_NUMBER: usize = 10_000;

/// The specifier of `require("<spec>")` — exactly a call of the free name
/// `require` with one string literal argument. The ONE shape owner of a
/// require call (`naming::reconcile`'s require-binding rule reads it too).
pub fn require_specifier<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    let Expression::CallExpression(call) = unparen(expr) else {
        return None;
    };
    let Expression::Identifier(callee) = unparen(&call.callee) else {
        return None;
    };
    if callee.name != "require" || call.optional || call.arguments.len() != 1 {
        return None;
    }
    match &call.arguments[0] {
        Argument::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// The pipeline's name for a library specifier, or None for anything that
/// is not a bare package / built-in specifier: `path` → `pathModule`,
/// `node:fs/promises` → `fsPromisesModule`, `child_process` →
/// `childProcessModule`, `@aws-sdk/client-s3` → `awsSdkClientS3Module`.
pub fn library_module_name(spec: &str) -> Option<String> {
    let spec = spec.strip_prefix("node:").unwrap_or(spec);
    if spec.is_empty() || spec.starts_with('.') || spec.starts_with('/') || spec.contains(':') {
        return None;
    }
    let words: Vec<&str> = spec
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let first = words.first()?;
    if first.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let mut name = String::new();
    for (i, w) in words.iter().enumerate() {
        let mut chars = w.chars();
        let head = chars.next()?;
        if i == 0 {
            name.push(head.to_ascii_lowercase());
        } else {
            name.push(head.to_ascii_uppercase());
        }
        name.extend(chars);
    }
    name.push_str("Module");
    is_valid_rename_target(&name).then_some(name)
}

/// One recognised library import.
struct Import {
    row: usize,
    binding: BindingId,
    /// The require call's position (the numbering's source order).
    at: u32,
    base: String,
}

/// Name every module binding of `graph` whose only write is a library
/// `require`. `suggested` is the per-row prior-version suggestion.
pub fn name_library_imports(
    semantic: &Semantic<'_>,
    graph: &UnifiedGraph,
    state: &mut RenameState,
    binding_state: &mut [Lifecycle],
    suggested: &[Option<String>],
) -> PlumbingNames {
    let mut imports: Vec<Import> = graph
        .module_bindings
        .iter()
        .enumerate()
        .filter(|(row, _)| binding_state[*row].is_pending())
        .filter_map(|(row, b)| {
            let (at, spec) = sole_require_write(semantic, b.symbol, &b.redeclared_spans)?;
            let base = library_module_name(&spec)?;
            let binding = state.view().binding_of_symbol(b.symbol)?;
            Some(Import {
                row,
                binding,
                at,
                base,
            })
        })
        .collect();
    imports.sort_by_key(|i| i.at);
    let mut out = PlumbingNames::default();
    let mut left: Vec<&Import> = Vec::new();
    // A prior-version suggestion of the same family first (stability).
    for import in &imports {
        let wanted = suggested
            .get(import.row)
            .and_then(Option::as_deref)
            .filter(|s| in_family(s, &import.base));
        let current = state.name_of(import.binding).to_string();
        match wanted {
            Some(name) if attempt(state, import.binding, name).is_none() => {
                settle(
                    graph,
                    binding_state,
                    import,
                    &current,
                    name,
                    &mut out,
                    state,
                );
            }
            _ => left.push(import),
        }
    }
    for import in left {
        let current = state.name_of(import.binding).to_string();
        let mut why = "no-free-name";
        let mut landed = None;
        for n in 1..=MAX_NUMBER {
            let name = if n == 1 {
                import.base.clone()
            } else {
                format!("{}{n}", import.base)
            };
            match attempt(state, import.binding, &name) {
                None => {
                    landed = Some(name);
                    break;
                }
                Some(r) if is_name_taken(r) => {}
                Some(r) => {
                    why = r.as_str();
                    break;
                }
            }
        }
        match landed {
            Some(name) => settle(
                graph,
                binding_state,
                import,
                &current,
                &name,
                &mut out,
                state,
            ),
            None => {
                let row = Attempt::new(Tier::LibraryImport, Outcome::Rejected)
                    .reason(why)
                    .proposed(&import.base);
                state.record(import.binding, &current, row, false);
                out.declined.push((current, why.to_string()));
            }
        }
    }
    out
}

fn is_name_taken(r: RejectionReason) -> bool {
    crate::naming::reask::class_of(r) == crate::naming::reask::ReaskClass::NameTaken
}

/// `name` is `base` or `base` + a number.
fn in_family(name: &str, base: &str) -> bool {
    name.strip_prefix(base)
        .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
}

/// Apply `name` through the validated applier: None when it applied (the
/// trail row is recorded by [`settle`]), else why it was refused.
fn attempt(state: &mut RenameState, binding: BindingId, name: &str) -> Option<RejectionReason> {
    let scope = state.scope_of_binding(binding);
    let old = state.name_of(binding).to_string();
    if old == name {
        return None;
    }
    let a = state.attempt_validated_rename(
        RenameRequest {
            scope,
            old_name: &old,
            new_name: name,
            expected: Some(binding),
        },
        TrailSpec::CallerRecords {
            tier: Tier::LibraryImport,
        },
    );
    if a.applied {
        None
    } else {
        Some(a.reason.unwrap_or(RejectionReason::TargetInScope))
    }
}

/// Record the applied name and settle the binding (no wave asks it).
fn settle(
    graph: &UnifiedGraph,
    binding_state: &mut [Lifecycle],
    import: &Import,
    before: &str,
    name: &str,
    out: &mut PlumbingNames,
    state: &mut RenameState,
) {
    let b = &graph.module_bindings[import.row];
    let row = Attempt::new(Tier::LibraryImport, Outcome::Applied).proposed(name);
    state.record(import.binding, before, row, false);
    out.named.push((before.to_string(), name.to_string()));
    binding_state[import.row].mark_skipped("library-import", &b.session_id);
}

/// The binding's single write, when it is exactly a `require("<spec>")`
/// of the program's require: (the call's start, the specifier). Any other
/// write — a second assignment, a compound one, an update, a
/// redeclaration — disqualifies it.
fn sole_require_write(
    semantic: &Semantic<'_>,
    symbol: SymbolId,
    redeclared: &[u32],
) -> Option<(u32, String)> {
    if !redeclared.is_empty() {
        return None;
    }
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut write = None;
    if let AstKind::VariableDeclarator(d) = nodes.kind(scoping.symbol_declaration(symbol))
        && let Some(init) = d.init.as_ref()
    {
        write = Some(init);
    }
    for &rid in scoping.get_resolved_reference_ids(symbol) {
        let r = scoping.get_reference(rid);
        if !r.flags().contains(ReferenceFlags::Write) {
            continue;
        }
        if write.is_some() || r.flags().contains(ReferenceFlags::Read) {
            return None;
        }
        let value = plain_assignment_value(semantic, r.node_id())?;
        write = Some(value);
    }
    let value = write?;
    let spec = require_specifier(value)?;
    program_require(semantic, value, scoping.symbol_scope_id(symbol))
        .then(|| (value.span().start, spec.to_string()))
}

/// The right-hand side of the plain `X = …` whose whole target is the
/// write reference `node`.
fn plain_assignment_value<'s, 'a>(
    semantic: &'s Semantic<'a>,
    node: oxc_semantic::NodeId,
) -> Option<&'s Expression<'a>> {
    let nodes = semantic.nodes();
    let ref_span = nodes.get_node(node).span();
    let mut cur = node;
    loop {
        let parent = nodes.parent_id(cur);
        if parent == cur {
            return None;
        }
        if let AstKind::AssignmentExpression(a) = nodes.kind(parent) {
            let AssignmentTarget::AssignmentTargetIdentifier(id) = &a.left else {
                return None;
            };
            return (a.operator == AssignmentOperator::Assign && id.span == ref_span)
                .then_some(&a.right);
        }
        cur = parent;
    }
}

/// The call's `require` is the program's: unresolved (the free name), or
/// a binding of `scope` or one of its ancestors (the CJS wrapper's
/// parameter) — never a function the code declared closer in.
fn program_require(
    semantic: &Semantic<'_>,
    call: &Expression<'_>,
    scope: oxc_semantic::ScopeId,
) -> bool {
    let Expression::CallExpression(c) = unparen(call) else {
        return false;
    };
    let Expression::Identifier(callee) = unparen(&c.callee) else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym) = callee
        .reference_id
        .get()
        .and_then(|r| scoping.get_reference(r).symbol_id())
    else {
        return true;
    };
    let owner = scoping.symbol_scope_id(sym);
    let mut cur = Some(scope);
    while let Some(s) = cur {
        if s == owner {
            return true;
        }
        cur = scoping.scope_parent_id(s);
    }
    false
}
