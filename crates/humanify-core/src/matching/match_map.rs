//! `MatchResult.matches` with the TS `Map`'s ITERATION ORDER (WP3.2).
//!
//! The cascade only ever looked `matches` up, counted it, or sorted it
//! before output, so a hash map was enough through phase 2. The transfer
//! stage is the first consumer whose DECISIONS read the insertion order:
//! `deriveBindingRenames` walks the binding cascade's matches and
//! `collectFunctionVarNameTransfers` the function cascade's, and the
//! resulting rename list is APPLIED in that order — which rename claims a
//! contested name first decides who is rejected (`target-in-scope`) and who
//! lands through the retry pass (15-porting-lessons §4: order is a decision
//! input, reproduced by construction).
//!
//! JS `Map` semantics, exactly: `set` on a NEW key appends; `set` on an
//! existing key replaces the value IN PLACE (the position is kept); `delete`
//! removes the key, so a later `set` of it appends at the end. Reads go
//! through `Deref` to the lookup table (every existing `&HashMap` consumer
//! keeps working); there is deliberately no `DerefMut` — a write that
//! bypassed the order index would silently corrupt it.

use std::collections::{BTreeMap, HashMap};
use std::ops::Deref;

/// An insertion-ordered `String → String` map with JS `Map` semantics.
#[derive(Debug, Clone, Default)]
pub struct MatchMap {
    lookup: HashMap<String, String>,
    /// key → its insertion sequence number.
    seq: HashMap<String, u64>,
    /// sequence number → key, in insertion order.
    order: BTreeMap<u64, String>,
    next: u64,
}

impl MatchMap {
    /// TS `new Map()`.
    pub fn new() -> MatchMap {
        MatchMap::default()
    }

    /// TS `Map.set`: appends a new key; an existing key keeps its position.
    pub fn insert(&mut self, key: String, value: String) -> Option<String> {
        if !self.seq.contains_key(&key) {
            self.seq.insert(key.clone(), self.next);
            self.order.insert(self.next, key.clone());
            self.next += 1;
        }
        self.lookup.insert(key, value)
    }

    /// TS `Map.delete`.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        if let Some(seq) = self.seq.remove(key) {
            self.order.remove(&seq);
        }
        self.lookup.remove(key)
    }

    /// TS `for (const [k, v] of map)` — insertion order.
    pub fn iter_ordered(&self) -> impl Iterator<Item = (&String, &String)> {
        self.order
            .values()
            .map(|k| (k, self.lookup.get(k).expect("order and lookup agree")))
    }

    /// A plain lookup copy (for consumers that take an owned hash map).
    pub fn to_hash_map(&self) -> HashMap<String, String> {
        self.lookup.clone()
    }
}

impl Deref for MatchMap {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &HashMap<String, String> {
        &self.lookup
    }
}

impl FromIterator<(String, String)> for MatchMap {
    /// Builds by `set`ting each pair in iteration order.
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> MatchMap {
        let mut map = MatchMap::new();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}

impl Extend<(String, String)> for MatchMap {
    fn extend<I: IntoIterator<Item = (String, String)>>(&mut self, iter: I) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

#[cfg(test)]
mod match_map_test;
