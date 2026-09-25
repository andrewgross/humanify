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

/// Which invariant the generated text failed (plugin.ts: `parseFailure`,
/// else `structuralFailure ?? outputSemanticFailure`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Valid,
    /// The output does not parse.
    ParseFailed,
    /// The structural signature differs (a pure rename keeps it).
    Structural,
    /// The free-name set or the binding count moved (capture, left-behind
    /// reference, split/merged declaration): both sides' measures.
    Semantic {
        free_before: Vec<String>,
        free_after: Vec<String>,
        bindings_before: usize,
        bindings_after: usize,
    },
}

/// The verdict on the generated text against the fresh baseline.
pub fn verdict(generated: &str, baseline: &Baseline) -> Verdict {
    let Some(after) = measure(generated) else {
        return Verdict::ParseFailed;
    };
    if after.signature != baseline.signature {
        return Verdict::Structural;
    }
    if after.free_names != baseline.free_names || after.binding_count != baseline.binding_count {
        return Verdict::Semantic {
            free_before: baseline.free_names.iter().cloned().collect(),
            free_after: after.free_names.iter().cloned().collect(),
            bindings_before: baseline.binding_count,
            bindings_after: after.binding_count,
        };
    }
    Verdict::Valid
}

/// Tokens either side of the first divergence (`DIVERGENCE_CONTEXT`).
const DIVERGENCE_CONTEXT: usize = 6;

/// The signature's token stream: the canonical serializer's stream split
/// at its structure (`{}[],:`), a JSON string literal kept whole.
fn signature_tokens(text: &str) -> Option<Vec<String>> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    if !ingest.errors.is_empty() {
        return None;
    }
    let json = crate::ingest::program_estree_json(ingest.program);
    let tables = SymbolTables::build(ingest.semantic());
    let parts = canonical_serialize_privates_blinded(&json, &tables, LiteralPolicy::Verbatim).parts;
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut chars = parts.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                cur.push(c);
                while let Some(d) = chars.next() {
                    cur.push(d);
                    if d == '\\' {
                        if let Some(e) = chars.next() {
                            cur.push(e);
                        }
                    } else if d == '"' {
                        break;
                    }
                }
            }
            '{' | '}' | '[' | ']' | ',' | ':' => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    Some(tokens)
}

/// `describeStructuralDivergence` (output-validation.ts): WHERE the
/// generated text's structural signature leaves the original's — the
/// first differing token, both streams' lengths and a window of context,
/// every line indented (the eval harness keeps an `ERROR:` line's indented
/// block). None when the streams agree. The tokens are the Rust canonical
/// serializer's, so the index and the token texts differ from the TS's by
/// design (the blessed serializer exemption, 00-control §3); the shape and
/// the question answered are the TS's.
pub fn describe_structural_divergence(original: &str, generated: &str) -> Option<String> {
    let Some(before) = signature_tokens(original) else {
        return Some("could not re-parse the original source to localise the divergence".into());
    };
    let Some(after) = signature_tokens(generated) else {
        return Some("could not recover the token streams to localise the divergence".into());
    };
    let n = before.len().min(after.len());
    let first = (0..n)
        .find(|&i| before[i] != after[i])
        .or((before.len() != after.len()).then_some(n))?;
    let window = |toks: &[String]| {
        let lo = first.saturating_sub(DIVERGENCE_CONTEXT);
        let hi = (first + DIVERGENCE_CONTEXT + 1).min(toks.len());
        toks.get(lo..hi).map(|w| w.join(" ")).unwrap_or_default()
    };
    let tok = |toks: &[String]| {
        serde_json::to_string(toks.get(first).map_or("<end>", String::as_str)).expect("a string")
    };
    let lengths = if before.len() == after.len() {
        format!("{} tokens each", before.len())
    } else {
        format!("{} tokens before vs {} after", before.len(), after.len())
    };
    Some(format!(
        "  first divergence at token {first} of {lengths}\n    original: {}\n    output:   {}\n    original context: {}\n    output context:   {}",
        tok(&before),
        tok(&after),
        window(&before),
        window(&after)
    ))
}

/// `!parseFailure && !semanticFailure` for the generated text.
pub fn output_valid(generated: &str, baseline: &Baseline) -> bool {
    verdict(generated, baseline) == Verdict::Valid
}
