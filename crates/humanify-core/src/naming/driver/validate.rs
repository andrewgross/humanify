//! `outputValid` — TS plugin.ts `checkStructuralInvariant` +
//! `validateGeneratedOutput` (src/output-validation.ts): the generated text
//! must re-parse, keep the free-name set and the total binding count
//! (`compareSemantics`), and keep the rename-invariant structural signature
//! (binding identifiers as order-keyed slots; literals, operators, property
//! keys and free names verbatim). The post-generate passes (reconcile,
//! deferred sweep, family permute) run only on a valid output.
//!
//! The signature is the canonical serializer's token stream over the whole
//! program with literals VERBATIM (TS `hashAndMapPath(path, true,
//! { privateNamesAsSlots: true })`). Its BYTES differ from the TS by design
//! (00-control §3); only its EQUALITY before vs after is read, which is a
//! partition question the two serializers agree on. Private names are
//! blinded rather than slotted — equal for every pure private rename (the
//! statement twins' `#f` → `#A`); a rename that MERGED two privates would
//! pass here and fail the TS (never observed; recorded).
//!
//! The messages (the CLI's `ERROR:` blocks) are WPB.4's; this answers only
//! the gating question.
//!
//! An `export { x } from "m"` specifier local names a binding of ANOTHER
//! module; the serializer reads it verbatim, never as a slot. The TS once
//! resolved it by name, failing a correct rename that gave a local binding
//! the same name (finding #34, fixed TS-first 2026-09-25).

use std::collections::BTreeSet;

use oxc_allocator::Allocator;

use crate::hash::serialize::{LiteralPolicy, SymbolTables, canonical_serialize_privates_blinded};
use crate::ingest::Ingest;
use crate::rename::validated::scopes::BabelScopes;

/// `SemanticBaseline`: captured on the fresh text before any rename.
pub struct Baseline {
    signature: String,
    free_names: BTreeSet<String>,
    binding_count: usize,
}

/// The three measurements of one text, or None when it does not parse.
fn measure(text: &str) -> Option<Baseline> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    if !ingest.errors.is_empty() {
        return None;
    }
    let semantic = ingest.semantic();
    let scopes = BabelScopes::build(semantic);
    let json = crate::ingest::program_estree_json(ingest.program);
    let tables = SymbolTables::build(semantic);
    let signature =
        canonical_serialize_privates_blinded(&json, &tables, LiteralPolicy::Verbatim).hash;
    Some(Baseline {
        signature,
        free_names: scopes.globals.clone(),
        binding_count: scopes.bindings.len(),
    })
}

/// `captureSemanticBaseline` over the fresh text.
pub fn baseline_of(fresh: &str) -> Option<Baseline> {
    measure(fresh)
}

/// `!parseFailure && !semanticFailure` for the generated text.
pub fn output_valid(generated: &str, baseline: &Baseline) -> bool {
    let Some(after) = measure(generated) else {
        return false;
    };
    after.free_names == baseline.free_names
        && after.binding_count == baseline.binding_count
        && after.signature == baseline.signature
}
