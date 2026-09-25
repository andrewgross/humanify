//! The babel text the naming waves read, under the CURRENT names — what
//! `generate(node)` prints in the TS once the rename overlay's renames
//! have landed on the AST: a function's code, its body and non-simple
//! params (callee signatures), a callee's display name, a parent-scope
//! declaration.
//!
//! [`Occurrences`] maps every identifier occurrence a rename writes (the
//! binding's declaration identifier, its identifier references, the
//! identifiers its constant violations write — `fastRenameBinding`'s set)
//! to its binding; [`FnPrinter::edits`] turns the overlay into
//! [`Replacement`]s for a span, including babel's two name-dependent
//! ObjectProperty forms (a shorthand whose value no longer equals its key
//! prints `key: value`; `{ key: v = d }` whose `v` became `key` collapses to
//! `key = d`).

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_ast::ast::{
    AssignmentTarget, AssignmentTargetMaybeDefault, BindingPattern, Expression, PropertyKey,
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use super::generate::{Replacement, TextView};
use super::nodes::{FnNode, fn_nodes, params_of};
use crate::graph::UnifiedGraph;
use crate::naming::context::{CalleeView, DeclView, ParamView};
use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::{BindingId, SiteType};

/// How an occurrence prints when its binding is renamed.
#[derive(Clone, Debug, PartialEq, Eq)]
enum OccForm {
    /// The identifier alone.
    Plain,
    /// The VALUE of a shorthand property whose key is `key`.
    Shorthand { key: String },
}

/// Every rename-written identifier occurrence, by start.
pub struct Occurrences {
    /// (start, end, binding, form), sorted by start.
    occ: Vec<(u32, u32, BindingId, OccForm)>,
    /// `{ key: v = d }` pattern properties: (key span start, left ident
    /// span, key name) — collapse candidates.
    collapses: Vec<(u32, Span, String)>,
}

impl Occurrences {
    pub fn build(semantic: &Semantic<'_>, state: &RenameState) -> Occurrences {
        let view = state.view();
        let mut by_start: HashMap<u32, (u32, BindingId)> = HashMap::new();
        for (i, b) in view.bindings.iter().enumerate() {
            let id = BindingId(i as u32);
            by_start.insert(b.id_span.start, (b.id_span.end, id));
            for r in &b.refs {
                if r.ty == SiteType::Identifier {
                    by_start.insert(r.span.start, (r.span.end, id));
                }
            }
            for targets in &b.violation_targets {
                for t in targets {
                    by_start.insert(t.start, (t.end, id));
                }
            }
        }
        let mut shorthand: HashMap<u32, String> = HashMap::new();
        let mut collapses = Vec::new();
        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ObjectProperty(p) if p.shorthand => {
                    if let (PropertyKey::StaticIdentifier(k), Expression::Identifier(v)) =
                        (&p.key, &p.value)
                    {
                        shorthand.insert(v.span.start, k.name.to_string());
                    }
                }
                AstKind::BindingProperty(p) => {
                    let PropertyKey::StaticIdentifier(k) = &p.key else {
                        continue;
                    };
                    let left = match &p.value {
                        BindingPattern::BindingIdentifier(b) if p.shorthand => {
                            shorthand.insert(b.span.start, k.name.to_string());
                            continue;
                        }
                        BindingPattern::AssignmentPattern(a) => match &a.left {
                            BindingPattern::BindingIdentifier(b) => b,
                            _ => continue,
                        },
                        _ => continue,
                    };
                    if p.shorthand {
                        shorthand.insert(left.span.start, k.name.to_string());
                    } else {
                        collapses.push((k.span.start, left.span, k.name.to_string()));
                    }
                }
                AstKind::AssignmentTargetPropertyIdentifier(p) => {
                    shorthand.insert(p.binding.span.start, p.binding.name.to_string());
                }
                AstKind::AssignmentTargetPropertyProperty(p) => {
                    let PropertyKey::StaticIdentifier(k) = &p.name else {
                        continue;
                    };
                    if let AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) = &p.binding
                        && let AssignmentTarget::AssignmentTargetIdentifier(left) = &d.binding
                    {
                        collapses.push((k.span.start, left.span, k.name.to_string()));
                    }
                }
                _ => {}
            }
        }
        let mut occ: Vec<(u32, u32, BindingId, OccForm)> = by_start
            .into_iter()
            .map(|(start, (end, b))| {
                let form = match shorthand.remove(&start) {
                    Some(key) => OccForm::Shorthand { key },
                    None => OccForm::Plain,
                };
                (start, end, b, form)
            })
            .collect();
        occ.sort_by_key(|o| o.0);
        collapses.sort_by_key(|c| c.0);
        Occurrences { occ, collapses }
    }
}

/// The printer for one fresh text under a rename state.
pub struct FnPrinter<'a, 's> {
    pub semantic: &'a Semantic<'s>,
    pub view: &'a TextView<'s>,
    pub graph: &'a UnifiedGraph,
    pub state: &'a RenameState,
    pub occ: &'a Occurrences,
    /// Each function row's node handles ([`fn_nodes`], built once).
    pub fns: &'a [Option<FnNode>],
}

impl<'a, 's> FnPrinter<'a, 's> {
    /// The function rows' node handles, for [`FnPrinter::fns`].
    pub fn nodes(semantic: &Semantic<'_>, graph: &UnifiedGraph) -> Vec<Option<FnNode>> {
        fn_nodes(semantic, graph)
    }

    /// The overlay's edits inside `span`.
    pub fn edits(&self, span: Span) -> Vec<Replacement> {
        let text = self.view.text;
        let occ = &self.occ.occ;
        let mut out = Vec::new();
        let lo = occ.partition_point(|o| o.0 < span.start);
        for (start, end, b, form) in &occ[lo..] {
            if *start >= span.end {
                break;
            }
            if *end > span.end {
                continue;
            }
            let current = self.state.name_of(*b);
            let original = &text[*start as usize..*end as usize];
            let rendered = match form {
                OccForm::Shorthand { key } if current != key => format!("{key}: {current}"),
                _ => current.to_string(),
            };
            if rendered != original {
                out.push(Replacement {
                    span: Span::new(*start, *end),
                    text: rendered,
                });
            }
        }
        let clo = self.occ.collapses.partition_point(|c| c.0 < span.start);
        for (key_start, left, key) in &self.occ.collapses[clo..] {
            if *key_start >= span.end {
                break;
            }
            if left.end > span.end {
                continue;
            }
            let original = &text[left.start as usize..left.end as usize];
            let current = self.current_at(left.start).unwrap_or(original);
            if current == key && original != key {
                // `key: v = d` prints as `key = d`: drop `key: ` — the left
                // identifier (renamed to the key) carries the name.
                out.push(Replacement {
                    span: Span::new(*key_start, left.start),
                    text: String::new(),
                });
            }
        }
        out
    }

    /// The current name of the occurrence starting at `start`.
    fn current_at(&self, start: u32) -> Option<&str> {
        let occ = &self.occ.occ;
        let i = occ.partition_point(|o| o.0 < start);
        occ.get(i)
            .filter(|o| o.0 == start)
            .map(|o| self.state.name_of(o.2))
    }

    /// `generate(node).code` of an arbitrary span under the overlay.
    pub fn pretty(&self, span: Span, comments: bool) -> String {
        self.view.pretty(span, &self.edits(span), comments)
    }

    fn node(&self, f: usize) -> FnNode {
        self.fns[f].unwrap_or_else(|| panic!("function row {f} has no node"))
    }

    /// `generate(fn.path.node).code` — the function's code.
    pub fn function_code(&self, f: usize) -> String {
        self.pretty(self.graph.functions[f].span, true)
    }

    /// `generate(node.body, { compact: false, comments: false })`.
    pub fn body_code(&self, f: usize) -> String {
        let n = self.node(f);
        let mut edits = self.edits(n.body);
        if n.is_arrow {
            edits.extend(self.view.leading_object_paren_drop(n.body));
        }
        self.view.pretty(n.body, &edits, false)
    }

    /// The params as `getCalleeSignatures` classifies them.
    pub fn params(&self, f: usize) -> Vec<ParamView> {
        let n = self.node(f);
        let Some(params) = params_of(self.semantic, n.func_node) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for item in &params.items {
            let simple = match (&item.pattern, &item.initializer) {
                (BindingPattern::BindingIdentifier(b), None) => Some(ParamView::Identifier {
                    name: self.name_at(b.span.start, &b.name),
                }),
                (BindingPattern::BindingIdentifier(b), Some(_)) => Some(ParamView::Assign {
                    name: self.name_at(b.span.start, &b.name),
                }),
                (BindingPattern::AssignmentPattern(a), None) => match &a.left {
                    BindingPattern::BindingIdentifier(b) => Some(ParamView::Assign {
                        name: self.name_at(b.span.start, &b.name),
                    }),
                    _ => None,
                },
                _ => None,
            };
            out.push(simple.unwrap_or_else(|| ParamView::Other {
                code: self.pretty(item.span, false),
            }));
        }
        if let Some(rest) = &params.rest {
            out.push(match &rest.rest.argument {
                BindingPattern::BindingIdentifier(b) => ParamView::Rest {
                    name: self.name_at(b.span.start, &b.name),
                },
                _ => ParamView::Other {
                    code: self.pretty(rest.rest.span, false),
                },
            });
        }
        out
    }

    /// The current name of the identifier at `start` (its original name
    /// when no binding claims the occurrence).
    fn name_at(&self, start: u32, original: &str) -> String {
        self.current_at(start).unwrap_or(original).to_string()
    }

    /// The callee view `getCalleeSignatures` reads.
    pub fn callee_view(&self, f: usize) -> CalleeView {
        let n = self.node(f);
        let nodes = self.semantic.nodes();
        let node_type = if n.is_method {
            "ObjectMethod"
        } else if n.is_arrow {
            "ArrowFunctionExpression"
        } else if n.is_declaration {
            "FunctionDeclaration"
        } else {
            "FunctionExpression"
        };
        let id = match nodes.kind(n.func_node) {
            AstKind::Function(func) if !n.is_method => func
                .id
                .as_ref()
                .map(|id| self.name_at(id.span.start, &id.name)),
            _ => None,
        };
        let declarator_id = self.declarator_parent_name(n.row_node);
        CalleeView {
            node_type: node_type.to_string(),
            id,
            declarator_id,
            params: self.params(f),
            body_code: self.body_code(f),
        }
    }

    /// `callee.path.parent` when it is a VariableDeclarator with an
    /// identifier id — its current name (babel has no paren nodes).
    fn declarator_parent_name(&self, row_node: NodeId) -> Option<String> {
        let nodes = self.semantic.nodes();
        let mut parent = nodes.parent_id(row_node);
        while matches!(nodes.kind(parent), AstKind::ParenthesizedExpression(_)) {
            parent = nodes.parent_id(parent);
        }
        match nodes.kind(parent) {
            AstKind::VariableDeclarator(d) => match &d.id {
                BindingPattern::BindingIdentifier(b) => Some(self.name_at(b.span.start, &b.name)),
                _ => None,
            },
            _ => None,
        }
    }

    /// `getBindingDeclCode`'s view of a binding.
    pub fn decl_view(&self, binding: BindingId) -> DeclView {
        let nodes = self.semantic.nodes();
        let path = self.state.view().binding(binding).path_node;
        match nodes.kind(path) {
            AstKind::Function(f) if f.is_declaration() => DeclView::FunctionOrClass,
            AstKind::Class(c) if c.is_declaration() => DeclView::FunctionOrClass,
            AstKind::VariableDeclarator(_) => {
                let parent = nodes.parent_id(path);
                DeclView::Declarator {
                    code: self.pretty(nodes.get_node(parent).span(), true),
                }
            }
            _ => DeclView::Other {
                code: self.pretty(nodes.get_node(path).span(), true),
            },
        }
    }
}
