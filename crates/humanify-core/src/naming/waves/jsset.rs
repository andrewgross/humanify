//! JS `Set<string>` and `Record<string, string>` with their ORDER
//! semantics — the wave processor's shared name sets are read in insertion
//! order (the prompt's first-50 used names, the proximity window) and
//! mutated by `delete(old); add(new)` pairs, which move a name to the END
//! (15-porting-lessons §4: order is a decision input).

use std::collections::{BTreeMap, HashMap};

/// A `Set<string>`: insertion order; re-adding a present member keeps its
/// position; delete + add moves it to the end.
#[derive(Clone, Debug, Default)]
pub struct JsSet {
    seq: u64,
    pos: HashMap<String, u64>,
    order: BTreeMap<u64, String>,
}

impl JsSet {
    pub fn new() -> JsSet {
        JsSet::default()
    }

    /// `set.add(name)`.
    pub fn add(&mut self, name: impl Into<String>) {
        let name = name.into();
        if self.pos.contains_key(&name) {
            return;
        }
        self.order.insert(self.seq, name.clone());
        self.pos.insert(name, self.seq);
        self.seq += 1;
    }

    /// `set.delete(name)`.
    pub fn delete(&mut self, name: &str) {
        if let Some(p) = self.pos.remove(name) {
            self.order.remove(&p);
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.pos.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.pos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }

    /// Members in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.order.values()
    }

    pub fn to_vec(&self) -> Vec<String> {
        self.iter().cloned().collect()
    }

    /// The Set's estimated deep heap bytes for finding #66's gauges
    /// (naming::waves::processor::gauges): every member is stored TWICE —
    /// once as the `pos` HashMap's key, once as the `order` BTreeMap's
    /// value — so both sides are counted. Deterministic lower-bound
    /// arithmetic, pinned in gauges_test.
    pub fn deep_bytes(&self) -> u64 {
        // pos: the HashMap<String, u64>'s table (each entry's String
        // header + the u64 + a control byte) plus the name buffers.
        let pos = self.pos.len() as u64
            * (std::mem::size_of::<String>() as u64 + std::mem::size_of::<u64>() as u64 + 1)
            + self.pos.keys().map(|n| n.len() as u64).sum::<u64>();
        // order: (entry inline size + ~two words of B-tree node overhead)
        // per member, plus the stored String buffers.
        let order = self.order.len() as u64
            * (std::mem::size_of::<u64>() as u64 + std::mem::size_of::<String>() as u64 + 16)
            + self.order.values().map(|n| n.len() as u64).sum::<u64>();
        pos + order
    }
}

/// A `Record<string, string>` built by assignment: a new key appends, an
/// existing key keeps its position (identifiers are never array indexes,
/// so JS's integer-key-first rule never applies to these records).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsRecord(pub Vec<(String, String)>);

impl JsRecord {
    pub fn set(&mut self, key: &str, value: &str) {
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some(entry) => entry.1 = value.to_string(),
            None => self.0.push((key.to_string(), value.to_string())),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `{ ...self, ...other }`.
    pub fn spread(&mut self, other: &JsRecord) {
        for (k, v) in &other.0 {
            self.set(k, v);
        }
    }

    /// The record's estimated deep heap bytes for finding #66's gauges:
    /// the pair Vec's element slots plus every key and value buffer.
    /// Deterministic lower-bound arithmetic, pinned in gauges_test.
    pub fn deep_bytes(&self) -> u64 {
        self.0.len() as u64 * std::mem::size_of::<(String, String)>() as u64
            + self
                .0
                .iter()
                .map(|(k, v)| k.len() as u64 + v.len() as u64)
                .sum::<u64>()
    }
}
