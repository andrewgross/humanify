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
}
