//! The fresh graph's rows as the transfer tiers address them: the babel
//! FUNCTION node of each function row (`fn.path.node`), its own scope
//! (`fn.path.scope`), and the lookups the TS builds as Maps
//! (`nodeToFunction`, `scopeToFunction`, `currentFunctionMap`, the graph's
//! `module:<name>` keys). Built once over the crawl-time Babel scope view.

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_ast::ast::FunctionType;
use oxc_semantic::{NodeId, Semantic, SymbolId};
use oxc_span::Span;

use crate::graph::UnifiedGraph;
use crate::rename::validated::scopes::{BScopeId, BabelScopes, BindingId, BindingKind};

/// The shape of a function row's babel node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FnKind {
    Declaration,
    Expression,
    Arrow,
    Method,
}

/// One function row.
#[derive(Debug, Clone)]
pub struct FnRow {
    pub session_id: String,
    /// The babel function node (`fn.path.node`): the Function / arrow, or
    /// the METHOD node for class and object methods (babel has one node).
    pub node: NodeId,
    pub span: Span,
    /// `fn.path.scope` — the function's own scope.
    pub scope: BScopeId,
    pub kind: FnKind,
    /// The declaration / named expression's own id symbol.
    pub id_symbol: Option<SymbolId>,
}

/// One module-binding row (TS `ModuleBindingNode`, the fields the
/// transfer tiers read).
#[derive(Debug, Clone)]
pub struct ModuleRow {
    /// The ORIGINAL (minified) name — never updated by a rename.
    pub name: String,
    /// `node.identifier`'s span.
    pub id_span: Span,
    /// `node.scope` — the scope the binding is declared in.
    pub scope: BScopeId,
}

/// The fresh side's row views and lookups.
pub struct Rows {
    pub fns: Vec<FnRow>,
    pub modules: Vec<ModuleRow>,
    /// `nodeToFunction`: babel function node → fn row.
    pub fn_by_node: HashMap<NodeId, usize>,
    /// `scopeToFunction`: a function's own scope → fn row.
    pub fn_by_scope: HashMap<BScopeId, usize>,
    /// `currentFunctionMap`: session id → fn row.
    pub fn_by_session: HashMap<String, usize>,
    /// `graph.nodes.get("module:" + name)`: original name → module row.
    pub module_by_name: HashMap<String, usize>,
}

impl Rows {
    /// Build over the graph and the Babel scope view of the same text.
    pub fn build(graph: &UnifiedGraph, semantic: &Semantic<'_>, view: &BabelScopes) -> Rows {
        let nodes = semantic.nodes();
        let row_ids = crate::matching::row_node_ids(&graph.functions, nodes);
        let mut fns = Vec::with_capacity(graph.functions.len());
        for f in &graph.functions {
            let (node, kind) = row_ids
                .get(&(f.span.start, f.span.end))
                .copied()
                .unwrap_or_else(|| panic!("function row {} has no node", f.session_id));
            let (fn_kind, id_symbol) = match kind {
                AstKind::Function(func) => {
                    let id = func.id.as_ref().and_then(|id| id.symbol_id.get());
                    if func.r#type == FunctionType::FunctionExpression {
                        (FnKind::Expression, id)
                    } else {
                        (FnKind::Declaration, id)
                    }
                }
                AstKind::ArrowFunctionExpression(_) => (FnKind::Arrow, None),
                _ => (FnKind::Method, None),
            };
            fns.push(FnRow {
                session_id: f.session_id.clone(),
                node,
                span: f.span,
                scope: view.scope_of_node(node),
                kind: fn_kind,
                id_symbol,
            });
        }
        let modules: Vec<ModuleRow> = graph
            .module_bindings
            .iter()
            .map(|b| {
                let binding = view
                    .binding_of_symbol(b.symbol)
                    .unwrap_or_else(|| panic!("module binding {} has no babel binding", b.name));
                ModuleRow {
                    name: b.name.clone(),
                    id_span: b.span,
                    scope: view.binding(binding).owner,
                }
            })
            .collect();
        let mut fn_by_node = HashMap::new();
        let mut fn_by_scope = HashMap::new();
        let mut fn_by_session = HashMap::new();
        for (i, f) in fns.iter().enumerate() {
            fn_by_node.insert(f.node, i);
            fn_by_scope.insert(f.scope, i);
            fn_by_session.insert(f.session_id.clone(), i);
        }
        let mut module_by_name = HashMap::new();
        for (i, m) in modules.iter().enumerate() {
            module_by_name.insert(m.name.clone(), i);
        }
        Rows {
            fns,
            modules,
            fn_by_node,
            fn_by_scope,
            fn_by_session,
            module_by_name,
        }
    }

    /// TS `registerTransferredWithOwner`'s owner lookup: the scope's own
    /// function when its block is a function, else the nearest function
    /// PARENT above the block (`getFunctionParent` excludes the path
    /// itself and stops at a StaticBlock too, which is no graph row).
    pub fn owner_fn_of_scope(&self, view: &BabelScopes, scope: BScopeId) -> Option<usize> {
        let s = if view.scope(scope).ty.is_function() {
            scope
        } else {
            let mut cur = view.scope(scope).parent;
            loop {
                let id = cur?;
                if view.scope(id).ty.is_function_parent() {
                    break id;
                }
                cur = view.scope(id).parent;
            }
        };
        self.fn_by_node.get(&view.scope(s).node).copied()
    }

    /// `binding.path.isFunctionDeclaration()`: Babel registers exactly the
    /// function declarations as `hoisted` (a redeclaration keeps the first
    /// declaration's path and kind), so the kind answers it.
    pub fn binding_is_function_declaration(view: &BabelScopes, binding: BindingId) -> bool {
        view.binding(binding).kind == BindingKind::Hoisted
    }
}
