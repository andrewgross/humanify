//! The Babel-equivalent view of oxc expressions (WP1.5 by-product).
//!
//! oxc PRESERVES `ParenthesizedExpression` nodes; Babel does not. Every
//! place the TS does `t.isX(expr)` on a callee or operand, the Rust must
//! first skip the paren wrappers or it sees a shape the TS never produced.
//! This module is the ONE owner of that question (docs/responsibility.md);
//! the hand-rolled loops it replaces lived in ingest.rs, graph.rs,
//! modules/wrapper.rs and modules.rs.

use oxc_ast::ast::Expression;

/// The expression with every paren wrapper skipped — what Babel's AST
/// holds at this position.
pub fn unparen<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut e = expr;
    while let Expression::ParenthesizedExpression(p) = e {
        e = &p.expression;
    }
    e
}
