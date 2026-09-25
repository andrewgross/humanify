//! The graph rows' AST handles: for every function row, the oxc function
//! node the babel `FunctionNode.path.node` corresponds to — its params and
//! its babel `body` span (a block's braces, or an arrow's expression with
//! parentheses dropped, as babel's node has none).

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_ast::ast::{Expression, FormalParameters};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use crate::graph::UnifiedGraph;

/// One function row's AST view.
#[derive(Clone, Copy, Debug)]
pub struct FnNode {
    /// The graph row's node (the method node for object/class methods —
    /// babel's ObjectMethod / ClassMethod is one node).
    pub row_node: NodeId,
    /// The Function / ArrowFunctionExpression node itself.
    pub func_node: NodeId,
    /// babel's `node.body` span.
    pub body: Span,
    /// Whether the node is babel's FunctionDeclaration.
    pub is_declaration: bool,
    /// babel's `FunctionExpression` (a non-method, non-declaration function).
    pub is_expression: bool,
    pub is_arrow: bool,
    /// Object / class method (babel ObjectMethod / ClassMethod / ClassPrivateMethod).
    pub is_method: bool,
}

/// Index every function row to its node.
pub fn fn_nodes(semantic: &Semantic<'_>, graph: &UnifiedGraph) -> Vec<Option<FnNode>> {
    let nodes = semantic.nodes();
    let mut by_span: HashMap<(u32, u32), FnNode> = HashMap::new();
    for node in nodes.iter() {
        let (func_node, body, is_arrow, is_declaration) = match node.kind() {
            AstKind::Function(f) => {
                let Some(body) = &f.body else { continue };
                (node.id(), body.span, false, f.is_declaration())
            }
            AstKind::ArrowFunctionExpression(a) => {
                let body = match (a.body.as_function_body(), a.body.as_expression()) {
                    (Some(b), _) => b.span,
                    (None, Some(e)) => unparen_span(e),
                    (None, None) => continue,
                };
                (node.id(), body, true, false)
            }
            _ => continue,
        };
        // A method's row is its parent property / definition.
        let parent = nodes.parent_node(node.id());
        let (row_node, row_span, is_method) = match parent.kind() {
            AstKind::ObjectProperty(p)
                if p.method || p.kind != oxc_ast::ast::PropertyKind::Init =>
            {
                (parent.id(), p.span, true)
            }
            AstKind::MethodDefinition(m) => (parent.id(), m.span, true),
            _ => (node.id(), node.span(), false),
        };
        let entry = FnNode {
            row_node,
            func_node,
            body,
            is_declaration,
            is_expression: !is_arrow && !is_declaration && !is_method,
            is_arrow,
            is_method,
        };
        by_span.insert((row_span.start, row_span.end), entry);
    }
    graph
        .functions
        .iter()
        .map(|f| by_span.get(&(f.span.start, f.span.end)).copied())
        .collect()
}

/// The span babel gives an expression: parentheses are not nodes there.
pub fn unparen_span(expr: &Expression<'_>) -> Span {
    crate::babel_view::unparen(expr).span()
}

/// The function node's params (a Function or an arrow).
pub fn params_of<'a>(
    semantic: &'a Semantic<'_>,
    func_node: NodeId,
) -> Option<&'a FormalParameters<'a>> {
    match semantic.nodes().kind(func_node) {
        AstKind::Function(f) => Some(&f.params),
        AstKind::ArrowFunctionExpression(a) => Some(&a.params),
        _ => None,
    }
}
