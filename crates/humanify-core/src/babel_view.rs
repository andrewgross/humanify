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

/// Babel `loc` (1-based line, 0-based UTF-16 column) of a byte offset —
/// the ONE owner of the question (docs/responsibility.md). Babel's line
/// terminators are `\r\n`, `\r`, `\n`, U+2028 and U+2029 (anywhere, raw
/// strings and templates included); `diff` and `split("\n")` count `\n`
/// only, and every TS consumer that joins the two inherits the mismatch —
/// so do the ports (the reconcile's line keys, the permute's contexts).
pub struct BabelLines<'t> {
    text: &'t str,
    starts: Vec<u32>,
}

impl<'t> BabelLines<'t> {
    pub fn new(text: &'t str) -> BabelLines<'t> {
        let b = text.as_bytes();
        let mut starts = vec![0u32];
        let mut i = 0usize;
        while i < b.len() {
            match b[i] {
                b'\n' => starts.push(i as u32 + 1),
                b'\r' => {
                    if b.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                    starts.push(i as u32 + 1);
                }
                // U+2028 / U+2029: E2 80 A8 / E2 80 A9.
                0xE2 if b.get(i + 1) == Some(&0x80)
                    && matches!(b.get(i + 2), Some(0xA8 | 0xA9)) =>
                {
                    i += 2;
                    starts.push(i as u32 + 1);
                }
                _ => {}
            }
            i += 1;
        }
        BabelLines { text, starts }
    }

    /// The 1-based line of byte `pos`.
    pub fn line(&self, pos: u32) -> usize {
        self.starts.partition_point(|s| *s <= pos)
    }

    /// `(line, column)` of byte `pos`.
    pub fn loc(&self, pos: u32) -> (usize, usize) {
        let line = self.line(pos);
        let start = self.starts[line - 1] as usize;
        let col = self.text[start..pos as usize].encode_utf16().count();
        (line, col)
    }
}
