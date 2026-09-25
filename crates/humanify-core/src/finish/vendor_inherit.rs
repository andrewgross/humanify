//! Cross-release reuse of vendored library bodies — TS
//! `src/split/vendor-body-inherit.ts` (exp046 Task C).
//!
//! humanify does not name vendored library files, so Bun's minifier reroll
//! of every local reaches the emitted tree unchanged. When the prior
//! release's file at the same path is the SAME PROGRAM, its bytes are
//! written instead, and the file leaves the diff.
//!
//! The key is the whole safety argument: the literal-PRESERVING,
//! rename-invariant structural signature of the whole file (TS
//! `computeStructuralSignature` on the Program — here the canonical
//! serializer under [`LiteralPolicy::Verbatim`], the same family), never
//! the manifest's blurred `structuralHash`. Require paths stay IN the key,
//! so a match is drop-in bytes. A file that does not parse is never
//! inherited.
//!
//! Kill switch: `--disable vendor-inherit` (the caller passes no prior
//! root).

use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;

use crate::hash::serialize::{LiteralPolicy, SymbolTables, canonical_serialize};
use crate::ingest::{Ingest, program_estree_json};

/// The kill-switch name (`VENDOR_INHERIT_SWITCH`).
pub const VENDOR_INHERIT_SWITCH: &str = "vendor-inherit";

/// `VendorInheritStats`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VendorInheritStats {
    /// Files that had a prior counterpart and were compared.
    pub considered: usize,
    /// Files written with the prior release's bytes.
    pub inherited: usize,
}

/// The rename-invariant, literal-preserving signature of a whole file, or
/// None when it does not parse.
fn file_signature(code: &str) -> Option<String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, code);
    if !ingest.errors.is_empty() {
        return None;
    }
    let tables = SymbolTables::build(ingest.semantic());
    let json = program_estree_json(ingest.program);
    Some(canonical_serialize(&json, &tables, LiteralPolicy::Verbatim).hash)
}

/// The text with every identifier-shaped run removed (`skeleton`): a
/// rename rewrites only such runs, so differing skeletons can never be
/// reconciled by one — the cheap necessary condition before two parses.
fn skeleton(code: &str) -> String {
    let bytes = code.as_bytes();
    let mut out = String::with_capacity(code.len());
    let mut i = 0;
    let is_start = |b: u8| b.is_ascii_alphabetic() || b == b'_' || b == b'$';
    let is_part = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';
    let mut run_from = 0;
    while i < bytes.len() {
        if is_start(bytes[i]) {
            out.push_str(&code[run_from..i]);
            while i < bytes.len() && is_part(bytes[i]) {
                i += 1;
            }
            run_from = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&code[run_from..]);
    out
}

/// Whether `prior` may replace `fresh`: identical, or the same program
/// under a renaming of its bindings.
pub fn same_program(prior: &str, fresh: &str) -> bool {
    if prior == fresh {
        return true;
    }
    if skeleton(prior) != skeleton(fresh) {
        return false;
    }
    match file_signature(prior) {
        None => false,
        Some(sig) => file_signature(fresh).is_some_and(|f| f == sig),
    }
}

/// `createVendorBodyInheritor(priorRoot)` — the prior release's TREE ROOT
/// (the directory holding `vendor/`).
pub struct VendorBodyInheritor {
    prior_root: PathBuf,
    stats: VendorInheritStats,
}

impl VendorBodyInheritor {
    pub fn new(prior_root: &Path) -> VendorBodyInheritor {
        VendorBodyInheritor {
            prior_root: prior_root.to_path_buf(),
            stats: VendorInheritStats::default(),
        }
    }

    /// `bytesFor(relPath, fresh)`: the prior bytes when the prior tree holds
    /// the same program at the same path, else `fresh`.
    pub fn bytes_for(&mut self, rel_path: &str, fresh: String) -> String {
        let Ok(bytes) = std::fs::read(self.prior_root.join(rel_path)) else {
            return fresh;
        };
        // readFileSync(…, "utf-8"): invalid sequences become U+FFFD.
        let prior = String::from_utf8_lossy(&bytes).into_owned();
        self.stats.considered += 1;
        if prior == fresh {
            return fresh;
        }
        if same_program(&prior, &fresh) {
            self.stats.inherited += 1;
            return prior;
        }
        fresh
    }

    pub fn stats(&self) -> VendorInheritStats {
        self.stats
    }
}

#[cfg(test)]
mod vendor_inherit_test;
