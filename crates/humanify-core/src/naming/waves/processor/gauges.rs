//! Finding #66's owner gauges (docs/perf-inventory.md item 1): what the
//! wave `Run` retains at era end, per owner, in estimated deep heap
//! bytes — the instrument that splits a fresh run's ~58 GB between the
//! candidate owners the perf survey enumerated (stored fn contexts,
//! binding-info collections, used-identifier layers, the recorded names,
//! the per-ask maps).
//!
//! Two rules, both load-bearing:
//!
//! - **Observe, never retain** (#65's lesson): a gauge that holds a copy
//!   of what it measures is the bug it was sent to find. Everything here
//!   reads borrowed data; [`super::Run::gauges`] assembles the owners.
//! - **Every heap allocation is counted once, deterministically.** A
//!   string contributes its BUFFER (`len`); the 24-byte header is a slot
//!   in whatever holds it, charged with the holder (a `Vec<String>` pays
//!   `size_of::<String>()` per element, a hash table pays its per-entry
//!   inline size plus a control byte, a B-tree 2 words of node overhead)
//!   — allocator slack and growth capacity are deliberately not counted,
//!   so two identical runs report identical gauges and a delta is real.
//!   A struct's own slot is not charged (its containers' element slots
//!   cover it); everything heap-reachable inside it is.
//!
//! Retention is the criterion for membership: a round's LANES and its
//! collected ENTRIES die at the round's barrier (`mem::take` in
//! `Run::barrier` — measured zero at era end), and the in-flight prompts
//! are #65's separately-gauged bounded window, so none of those are
//! gauge fields. The formulas are pinned case-for-case in `gauges_test`.

use std::collections::{HashMap, HashSet};

use crate::rename::transfer::owned::BindingInfo;
use humanify_model::llm::CalleeSignature;

/// The wave run's retention gauges: estimated deep heap bytes per owner
/// category, computed once at run end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaveGauges {
    /// The stored strategy material (the `strategies` Vec): every
    /// function pass's retained context — its binding infos, callee
    /// signatures, callsites, context vars, taken-name set.
    pub strategy_bytes: u64,
    /// The per-node contexts (the `ctxs` Vec): binding maps, phase
    /// orders, applied-name records, the nodes' reports.
    pub ctx_bytes: u64,
    /// The used-identifier material (the #56 residue): the shared
    /// `NameLayer`s and renamed-name layers, each counted ONCE (they are
    /// shared `Arc`s), plus every context's own barrier-edit sets.
    pub used_set_bytes: u64,
    /// The recorded names (the `names` Vec, one per applied rename).
    pub name_record_bytes: u64,
    /// The run's small maps and sets: the winners, the per-functionId
    /// round counter, the module used-names Set, the graph-era tables,
    /// the fresh-scope index.
    pub bookkeeping_bytes: u64,
}

/// One String's full footprint: its 24-byte header slot plus its buffer —
/// the per-element footprint of a `Vec<String>` (whose slots ARE the
/// headers, so a list is the plain sum of these). The hash-table helpers
/// charge the header inside the entry's inline size and add only the
/// buffer.
pub(crate) fn string_bytes(s: &str) -> u64 {
    std::mem::size_of::<String>() as u64 + s.len() as u64
}

/// A `Vec<String>`: each element's header-as-slot plus its buffer.
pub(crate) fn string_list_bytes(v: &[String]) -> u64 {
    v.iter().map(|s| string_bytes(s)).sum()
}

/// A `HashSet<T>`'s table: each entry's inline size plus one control byte
/// (load-factor slack ignored).
pub(crate) fn set_bytes<T>(s: &HashSet<T>) -> u64 {
    s.len() as u64 * (std::mem::size_of::<T>() + 1) as u64
}

/// A `HashMap<K, V>`'s table: each entry's inline size plus one control
/// byte.
pub(crate) fn map_bytes<K, V>(m: &HashMap<K, V>) -> u64 {
    m.len() as u64 * (std::mem::size_of::<K>() + std::mem::size_of::<V>() + 1) as u64
}

/// A `HashSet<String>`: its table (each entry's String header + a control
/// byte) plus the key buffers.
pub(crate) fn hash_set_of_strings(s: &HashSet<String>) -> u64 {
    set_bytes(s) + s.iter().map(|n| n.len() as u64).sum::<u64>()
}

/// A `HashMap<String, u64>`: its table (each entry's String header + the
/// u64 + a control byte) plus the key buffers.
pub(crate) fn hash_map_strings_to_u64(m: &HashMap<String, u64>) -> u64 {
    map_bytes(m) + m.keys().map(|k| k.len() as u64).sum::<u64>()
}

/// A `HashMap<String, String>`: its table (both headers + a control byte
/// per entry) plus both sides' buffers.
pub(crate) fn hash_map_strings_to_string(m: &HashMap<String, String>) -> u64 {
    map_bytes(m)
        + m.keys().map(|k| k.len() as u64).sum::<u64>()
        + m.values().map(|v| v.len() as u64).sum::<u64>()
}

/// One `BindingInfo`'s deep bytes: the name's BUFFER (the struct's own
/// slot — including the name's header — is charged by the holder).
pub(crate) fn binding_info_bytes(b: &BindingInfo) -> u64 {
    b.name.len() as u64
}

/// A `Vec<BindingInfo>`: its element slots (each holds the struct with
/// the name's header inline) plus every name buffer.
pub(crate) fn binding_infos_bytes(v: &[BindingInfo]) -> u64 {
    v.len() as u64 * std::mem::size_of::<BindingInfo>() as u64
        + v.iter().map(binding_info_bytes).sum::<u64>()
}

/// One `CalleeSignature`'s deep bytes: the name and snippet buffers plus
/// the params list's slots and buffers (the struct's own slot — including
/// the name's and the `Option`'s headers — is charged by the holder).
pub(crate) fn callee_signature_bytes(c: &CalleeSignature) -> u64 {
    c.name.len() as u64
        + string_list_bytes(&c.params)
        + c.snippet.as_ref().map_or(0, |s| s.len() as u64)
}

/// A `Vec<CalleeSignature>`: its element slots plus each signature's deep
/// bytes.
pub(crate) fn callee_signatures_bytes(v: &[CalleeSignature]) -> u64 {
    v.len() as u64 * std::mem::size_of::<CalleeSignature>() as u64
        + v.iter().map(callee_signature_bytes).sum::<u64>()
}
