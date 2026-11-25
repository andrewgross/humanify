//! Emission order within each file, aligned to a prior layout — the
//! emission-order half of TS `src/split/stable-split.ts`
//! (`orderByHashSequence`, `alignFileStatements`, `alignmentKey`,
//! `priorEmitSequence`, `alignEmissionOrder`; exp037 Lever B, exp038's
//! load-order constraint, exp050's (hash, name) key).
//!
//! A statement never moves between files; only which of a file's
//! statements occupies each of its slots is aligned, and only as far as
//! the load-time dependencies allow ([`super::load_order`]).

use std::collections::HashMap;

use crate::place::ledger::StableSplitLedger;

use super::load_order::{LoadOrderFacts, order_respecting_load_order};

/// The kill switches this module reads (`--disable <name>`), as config.
#[derive(Clone, Copy, Debug, Default)]
pub struct AlignSwitches {
    /// `emit-align`: no alignment at all (bundle order per file).
    pub emit_align_disabled: bool,
    /// `name-align`: key on the hash alone (the pre-050 behaviour).
    pub name_align_disabled: bool,
}

/// `alignmentKey(hash, name)`: hash + NUL + declared name; a statement
/// with no nameable declaration keys on its hash alone.
pub fn alignment_key(hash: &str, name: Option<&str>) -> String {
    match name {
        Some(n) if !n.is_empty() => format!("{hash}\u{0}{n}"),
        _ => hash.to_string(),
    }
}

/// `orderByHashSequence`: an index claims its prior rank only when
/// `claims` says its key is unambiguous; everything else keeps its place
/// relative to its predecessor. Stable.
pub(super) fn order_by_hash_sequence(
    list: &[usize],
    keys: &[String],
    prior_seq: &[String],
    claims: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let mut rank_of: HashMap<&str, i64> = HashMap::new();
    for (rank, h) in prior_seq.iter().enumerate() {
        rank_of.entry(h.as_str()).or_insert(rank as i64);
    }
    order_by_claimed_ranks(list, |idx| {
        if claims(idx) {
            rank_of.get(keys[idx].as_str()).copied()
        } else {
            None
        }
    })
}

/// Which claims kept their relative order: a longest run of rising
/// prior ranks, in list order (of equal runs, the one that ends
/// earliest, built from the earliest statements). The other claims are
/// statements the bundle moved.
fn unmoved(ranks: &[Option<i64>]) -> Vec<bool> {
    let mut sorted: Vec<i64> = ranks.iter().flatten().copied().collect();
    sorted.sort_unstable();
    sorted.dedup();
    // best[k]: over ranks below the k-th, the longest run and where it
    // ends, as a prefix-maximum tree.
    type Run = (usize, std::cmp::Reverse<usize>);
    let mut best: Vec<Option<Run>> = vec![None; sorted.len() + 1];
    let mut before: Vec<Option<usize>> = vec![None; ranks.len()];
    let mut longest: Option<Run> = None;
    for (i, rank) in ranks.iter().enumerate() {
        let Some(rank) = rank else { continue };
        let at = sorted.partition_point(|r| r < rank);
        let mut prefix: Option<Run> = None;
        let mut k = at;
        while k > 0 {
            prefix = prefix.max(best[k]);
            k -= k.isolate_lowest_one();
        }
        before[i] = prefix.map(|(_, end)| end.0);
        let run = (prefix.map_or(0, |(len, _)| len) + 1, std::cmp::Reverse(i));
        longest = longest.max(Some(run));
        let mut k = at + 1;
        while k < best.len() {
            best[k] = best[k].max(Some(run));
            k += k.isolate_lowest_one();
        }
    }
    let mut kept = vec![false; ranks.len()];
    let mut cur = longest.map(|(_, end)| end.0);
    while let Some(i) = cur {
        kept[i] = true;
        cur = before[i];
    }
    kept
}

/// Order `list` by each index's claimed prior rank; an index with no
/// claim keeps its place after the nearest earlier claim that is an
/// anchor (`anchors` of the claims; `prevRank + 0.5`, doubled here to
/// stay in integers). Stable.
fn order_by_ranks(
    list: &[usize],
    ranks: &[Option<i64>],
    anchors: impl Fn(&[Option<i64>]) -> Vec<bool>,
) -> Vec<usize> {
    let anchors = anchors(ranks);
    let mut prev_rank: i64 = -1;
    let mut keyed: Vec<(i64, usize, usize)> = list
        .iter()
        .enumerate()
        .map(|(pos, &idx)| match ranks[pos] {
            Some(r) => {
                if anchors[pos] {
                    prev_rank = r;
                }
                (2 * r, pos, idx)
            }
            None => (2 * prev_rank + 1, pos, idx),
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    keyed.into_iter().map(|(_, _, idx)| idx).collect()
}

/// [`order_by_ranks`] with every claim an anchor: an index with no claim
/// follows its predecessor, wherever that goes.
fn order_by_claimed_ranks(list: &[usize], rank: impl Fn(usize) -> Option<i64>) -> Vec<usize> {
    let ranks: Vec<Option<i64>> = list.iter().map(|&idx| rank(idx)).collect();
    order_by_ranks(list, &ranks, |ranks| vec![true; ranks.len()])
}

/// The names in an [`alignment_key`] (empty for a hash-only key).
fn key_names(key: &str) -> impl Iterator<Item = &str> {
    key.split_once('\u{0}')
        .map(|(_, names)| names)
        .into_iter()
        .flat_map(|names| names.split(','))
        .filter(|n| !n.is_empty())
}

/// How many entries of `keys` bind each name.
fn name_counts<'k>(keys: impl Iterator<Item = &'k str>) -> HashMap<&'k str, usize> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for key in keys {
        let names: std::collections::BTreeSet<&str> = key_names(key).collect();
        for n in names {
            *counts.entry(n).or_insert(0) += 1;
        }
    }
    counts
}

/// The hash in an [`alignment_key`].
fn key_hash(key: &str) -> &str {
    key.split_once('\u{0}').map_or(key, |(hash, _)| hash)
}

/// A statement whose key claimed nothing (its name changed) claims the
/// prior rank of the one prior statement with its hash, when that hash
/// occurs exactly once on each side. A rank a key claimed stays with
/// that key.
fn hash_claims(
    slots: &[usize],
    keys: &[String],
    prior_seq: &[String],
    key_claimed: &HashMap<usize, i64>,
) -> HashMap<usize, i64> {
    let mut fresh: HashMap<&str, usize> = HashMap::new();
    for &s in slots {
        *fresh.entry(key_hash(&keys[s])).or_insert(0) += 1;
    }
    let mut prior: HashMap<&str, (usize, i64)> = HashMap::new();
    for (rank, key) in prior_seq.iter().enumerate() {
        prior.entry(key_hash(key)).or_insert((0, rank as i64)).0 += 1;
    }
    let taken: std::collections::HashSet<i64> = key_claimed.values().copied().collect();
    slots
        .iter()
        .filter(|s| !key_claimed.contains_key(s))
        .filter_map(|&s| {
            let hash = key_hash(&keys[s]);
            match (fresh.get(hash), prior.get(hash)) {
                (Some(1), Some(&(1, rank))) if !taken.contains(&rank) => Some((s, rank)),
                _ => None,
            }
        })
        .collect()
}

/// exp227's name tier: a statement whose key claimed nothing (an edited
/// statement: its hash changed) claims the prior rank of the one prior
/// statement it shares a name with, when that name is bound by exactly
/// one statement on each side and every such name points at the same
/// prior statement. Ranks already claimed by a key are never taken.
fn name_claims(
    slots: &[usize],
    keys: &[String],
    prior_seq: &[String],
    key_claimed: &HashMap<usize, i64>,
) -> HashMap<usize, i64> {
    let fresh = name_counts(slots.iter().map(|&s| keys[s].as_str()));
    let prior = name_counts(prior_seq.iter().map(String::as_str));
    let mut prior_rank: HashMap<&str, i64> = HashMap::new();
    for (rank, key) in prior_seq.iter().enumerate() {
        for n in key_names(key) {
            prior_rank.entry(n).or_insert(rank as i64);
        }
    }
    let taken: std::collections::HashSet<i64> = key_claimed.values().copied().collect();
    let mut out = HashMap::new();
    for &s in slots.iter().filter(|s| !key_claimed.contains_key(s)) {
        let ranks: std::collections::BTreeSet<i64> = key_names(&keys[s])
            .filter(|n| fresh.get(n) == Some(&1) && prior.get(n) == Some(&1))
            .filter_map(|n| prior_rank.get(n).copied())
            .collect();
        if let (1, Some(&r)) = (ranks.len(), ranks.first())
            && !taken.contains(&r)
        {
            out.insert(s, r);
        }
    }
    out
}

/// `alignFileStatements(slots, keys, priorSeq, facts)`: one file's
/// statement indices ordered to match its prior emission order, within
/// what the load-time dependencies allow. PRECISION gate: only a key with
/// exactly one occurrence on each side may claim a prior position; a
/// statement whose key claims nothing may then claim one through a name
/// ([`name_claims`]).
pub fn align_file_statements(
    slots: &[usize],
    keys: &[String],
    prior_seq: Option<&[String]>,
    facts: &[LoadOrderFacts],
) -> Vec<usize> {
    let Some(prior_seq) = prior_seq.filter(|s| !s.is_empty()) else {
        return slots.to_vec(); // new file: bundle order
    };
    let mut fresh_count: HashMap<&str, usize> = HashMap::new();
    for &s in slots {
        *fresh_count.entry(keys[s].as_str()).or_insert(0) += 1;
    }
    let mut prior_count: HashMap<&str, usize> = HashMap::new();
    let mut rank_of: HashMap<&str, i64> = HashMap::new();
    for (rank, h) in prior_seq.iter().enumerate() {
        *prior_count.entry(h.as_str()).or_insert(0) += 1;
        rank_of.entry(h.as_str()).or_insert(rank as i64);
    }
    let unambiguous = |s: usize| {
        fresh_count.get(keys[s].as_str()) == Some(&1)
            && prior_count.get(keys[s].as_str()) == Some(&1)
    };
    let key_claimed: HashMap<usize, i64> = slots
        .iter()
        .filter(|&&s| unambiguous(s))
        .filter_map(|&s| rank_of.get(keys[s].as_str()).map(|&r| (s, r)))
        .collect();
    if key_claimed.len() < 2 {
        return slots.to_vec();
    }
    let mut claimed = key_claimed;
    let by_hash = hash_claims(slots, keys, prior_seq, &claimed);
    claimed.extend(by_hash);
    let by_name = name_claims(slots, keys, prior_seq, &claimed);
    // A statement with no claim stays with the claims the bundle did not
    // move: one that moved goes back to its prior place alone.
    let ranks: Vec<Option<i64>> = slots
        .iter()
        .map(|s| claimed.get(s).or_else(|| by_name.get(s)).copied())
        .collect();
    let desired = order_by_ranks(slots, &ranks, unmoved);
    order_respecting_load_order(slots, &desired, facts)
}

/// The ledger's emitted-order sequence (`emitHashes ?? hashes`).
fn emitted_sequence(ledger: &StableSplitLedger) -> Option<&Vec<String>> {
    ledger.emit_hashes.as_ref().or(ledger.hashes.as_ref())
}

/// `priorEmitSequence`: the prior's per-slot emitted keys, or None when
/// there is nothing usable (wrong hash version, missing/short sequence).
pub fn prior_emit_sequence(
    prior: Option<&StableSplitLedger>,
    switches: AlignSwitches,
) -> Option<Vec<String>> {
    let prior = prior?;
    if !prior.hashes_current() {
        return None;
    }
    let seq = emitted_sequence(prior)?;
    if seq.len() != prior.order.len() {
        return None;
    }
    let names = if switches.name_align_disabled {
        None
    } else {
        prior.emit_names.as_ref()
    };
    match names {
        Some(names) if names.len() == seq.len() => Some(
            seq.iter()
                .zip(names)
                .map(|(h, n)| alignment_key(h, n.as_deref()))
                .collect(),
        ),
        _ => Some(seq.clone()),
    }
}

/// `alignEmissionOrder(assignment, hashes, facts, prior, names)`: the
/// permutation `perm[slot] = body index emitted at that slot`; identity
/// when there is no usable prior.
pub fn align_emission_order(
    assignment: &[String],
    hashes: &[String],
    facts: &[LoadOrderFacts],
    prior: Option<&StableSplitLedger>,
    names: Option<&[Option<String>]>,
    switches: AlignSwitches,
) -> Vec<usize> {
    let n = assignment.len();
    let prior_layout = prior_emit_sequence(prior, switches);
    let (Some(prior_layout), Some(prior)) = (prior_layout, prior) else {
        return (0..n).collect();
    };
    if switches.emit_align_disabled {
        return (0..n).collect();
    }
    let mut prior_seq_by_file: HashMap<&str, Vec<String>> = HashMap::new();
    for (i, file) in prior.order.iter().enumerate() {
        prior_seq_by_file
            .entry(file.as_str())
            .or_default()
            .push(prior_layout[i].clone());
    }
    // Slots per file, in bundle order; files in first-appearance order
    // (the TS Map's — unobservable here: each file fills only its slots).
    let mut slots_by_file: Vec<(&str, Vec<usize>)> = Vec::new();
    let mut file_index: HashMap<&str, usize> = HashMap::new();
    for (i, file) in assignment.iter().enumerate() {
        let k = *file_index.entry(file.as_str()).or_insert_with(|| {
            slots_by_file.push((file.as_str(), Vec::new()));
            slots_by_file.len() - 1
        });
        slots_by_file[k].1.push(i);
    }
    let prior_has_names = !switches.name_align_disabled
        && prior
            .emit_names
            .as_ref()
            .is_some_and(|names| Some(names.len()) == emitted_sequence(prior).map(Vec::len));
    let keys: Vec<String> = match names {
        Some(names) if names.len() == hashes.len() && prior_has_names => hashes
            .iter()
            .zip(names)
            .map(|(h, n)| alignment_key(h, n.as_deref()))
            .collect(),
        _ => hashes.to_vec(),
    };
    let mut perm = vec![0usize; n];
    for (file, slots) in &slots_by_file {
        let aligned = align_file_statements(
            slots,
            &keys,
            prior_seq_by_file.get(file).map(Vec::as_slice),
            facts,
        );
        for (k, &slot) in slots.iter().enumerate() {
            perm[slot] = aligned[k];
        }
    }
    perm
}

#[cfg(test)]
mod align_test;
