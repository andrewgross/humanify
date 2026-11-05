//! Set similarity — the ONE owner of the k-shingle windows and the Jaccard
//! / containment ratios every matcher scores with (2026-10-13, matching
//! unification step 1).
//!
//! What a caller shingles, and what its sets hold, stays the CALLER's:
//! the cascade's tagged feature sets (`matching::FingerprintIndex::
//! compute_shingle_set` — edge n-grams + `ext:`/`prop:`/`str:` tokens, no
//! windows), the content shingles (`matching::statement_align::
//! content_shingles` — 4-token windows over the canonical walk's token
//! stream, capped, read by the snap gate, the close tier's corroboration
//! and `twins::role`), the vendor pairing (`modules::vendor_pairing` —
//! 5-token windows over the text lexer's stream, hashed) and the fossil
//! matcher (`place::assign::fossil_match` — token sets, declared-name sets,
//! multisets of statement hashes). This module owns only the arithmetic
//! they shared as copies:
//!
//! - [`shingle_windows`]: a stream no longer than `k` is ONE shingle of the
//!   whole stream (so tiny contents still compare); a longer one yields
//!   every `k`-window in order;
//! - [`jaccard`] over two sets (both empty reads 1 — the TS
//!   `jaccardSimilarity` rule the cascade and the close tier gate on),
//!   [`jaccard_from_counts`] when the caller counted the intersection its
//!   own way (an inverted index, a multiset), [`weighted_jaccard`] over
//!   summed weights (the vendor pairing's IDF literal score);
//! - [`containment_from_counts`]: |A ∩ B| / min(|A|, |B|).
//!
//! The empty-side policy is NOT uniform across callers and is not hidden
//! here: [`jaccard`] answers 1 for two empty sets, the count forms answer 0
//! for an empty union, and the fossil matcher's set scores return 0 early
//! when EITHER side is empty — that early return stays at its caller.

use std::collections::BTreeSet;

/// The k-shingle windows of `tokens`: the whole stream once when it is no
/// longer than `k`, else every `k`-window in order. `k` must be at least 1.
pub fn shingle_windows<T>(tokens: &[T], k: usize) -> impl Iterator<Item = &[T]> {
    let short = tokens.len() <= k;
    let whole = short.then_some(tokens);
    let windows = (!short).then(|| tokens.windows(k));
    whole.into_iter().chain(windows.into_iter().flatten())
}

/// |A ∩ B| / |A ∪ B| over two sets; 1 when both are empty (TS
/// `jaccardSimilarity`).
pub fn jaccard<T: Ord>(a: &BTreeSet<T>, b: &BTreeSet<T>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    jaccard_from_counts(a.intersection(b).count(), a.len(), b.len())
}

/// |A ∩ B| / |A ∪ B| from the sizes and a caller-counted intersection (a
/// multiset's sum of per-element minima reads the same); 0 when the union
/// is empty.
pub fn jaccard_from_counts(intersection: usize, a_len: usize, b_len: usize) -> f64 {
    let union = a_len + b_len - intersection;
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// The weighted Jaccard: shared weight over `(a_total + b_total) - shared`;
/// 0 when that union is not positive.
pub fn weighted_jaccard(shared: f64, a_total: f64, b_total: f64) -> f64 {
    let union = a_total + b_total - shared;
    if union > 0.0 { shared / union } else { 0.0 }
}

/// |A ∩ B| / min(|A|, |B|); 0 when the smaller side is empty.
pub fn containment_from_counts(intersection: usize, a_len: usize, b_len: usize) -> f64 {
    let smaller = a_len.min(b_len);
    if smaller == 0 {
        0.0
    } else {
        intersection as f64 / smaller as f64
    }
}

#[cfg(test)]
#[path = "similarity/similarity_test.rs"]
mod similarity_test;
