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
/// relative to its predecessor (`prevRank + 0.5`, doubled here to stay in
/// integers). Stable.
fn order_by_hash_sequence(
    list: &[usize],
    keys: &[String],
    prior_seq: &[String],
    claims: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let mut rank_of: HashMap<&str, i64> = HashMap::new();
    for (rank, h) in prior_seq.iter().enumerate() {
        rank_of.entry(h.as_str()).or_insert(rank as i64);
    }
    let mut prev_rank: i64 = -1;
    let mut keyed: Vec<(i64, usize, usize)> = list
        .iter()
        .enumerate()
        .map(|(pos, &idx)| {
            let rank = if claims(idx) {
                rank_of.get(keys[idx].as_str()).copied()
            } else {
                None
            };
            match rank {
                Some(r) => {
                    prev_rank = r;
                    (2 * r, pos, idx)
                }
                None => (2 * prev_rank + 1, pos, idx),
            }
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    keyed.into_iter().map(|(_, _, idx)| idx).collect()
}

/// `alignFileStatements(slots, keys, priorSeq, facts)`: one file's
/// statement indices ordered to match its prior emission order, within
/// what the load-time dependencies allow. PRECISION gate: only a key with
/// exactly one occurrence on each side may claim a prior position.
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
    for h in prior_seq {
        *prior_count.entry(h.as_str()).or_insert(0) += 1;
    }
    let unambiguous = |s: usize| {
        fresh_count.get(keys[s].as_str()) == Some(&1)
            && prior_count.get(keys[s].as_str()) == Some(&1)
    };
    if slots.iter().filter(|&&s| unambiguous(s)).count() < 2 {
        return slots.to_vec();
    }
    let desired = order_by_hash_sequence(slots, keys, prior_seq, unambiguous);
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
