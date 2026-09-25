//! GRADED shape tokens for one fossil module — TS `moduleTokens`
//! (fossil-assign.ts, exp078): tree-shape triples `n:<grand>><parent>><type>`
//! over BABEL node types ([`crate::place::babel_walk`], @babel/types
//! `VISITOR_KEYS` order), plus the rename-proof literal values
//! (`s:<first 40 UTF-16 units>`, `m:<JS number>`), each hashed to 8 hex of
//! sha1; a Set in first-seen order. NO identifier-derived tokens.

use serde_json::Value;
use sha1::{Digest, Sha1};

use crate::place::babel_walk::walk;
use crate::twins::fossil::FossilModule;

/// 8 hex of sha1 over the UTF-8 bytes (`shortHash`).
fn short_hash(s: &str) -> String {
    let digest = Sha1::digest(s.as_bytes());
    digest[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// A JS `Set<string>` in first-seen order.
#[derive(Default)]
struct TokenSet {
    order: Vec<String>,
    seen: std::collections::HashSet<String>,
}

impl TokenSet {
    fn add(&mut self, token: String) {
        if self.seen.insert(token.clone()) {
            self.order.push(token);
        }
    }
}

/// `moduleTokens(module, body)`.
pub fn module_tokens(module: &FossilModule, body: &[Value]) -> Vec<String> {
    let mut tokens = TokenSet::default();
    for &i in &module.statements {
        walk(&body[i], |v| {
            tokens.add(short_hash(&format!(
                "n:{}>{}>{}",
                v.grand, v.parent, v.babel_type
            )));
            if let Some(lit) = v.literal {
                tokens.add(short_hash(lit));
            }
        });
    }
    tokens.order
}

#[cfg(test)]
mod tokens_test;
