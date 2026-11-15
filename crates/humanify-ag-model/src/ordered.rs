//! A JSON object whose keys are written in the order they were added: the
//! count maps and metadata records whose reading order is their insertion
//! order (serde_json's own map sorts its keys).

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

/// Entries in insertion order; setting an existing key keeps its position.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderedMap<V>(pub Vec<(String, V)>);

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        OrderedMap(Vec::new())
    }
}

impl<V> OrderedMap<V> {
    pub fn new() -> Self {
        Self::default()
    }

    /// `map[key] = value`: an existing key keeps its position.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<V>) {
        let key = key.into();
        let value = value.into();
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }

    /// Builder form of [`Self::set`].
    pub fn with(mut self, key: impl Into<String>, value: impl Into<V>) -> Self {
        self.set(key, value);
        self
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn entries(&self) -> &[(String, V)] {
        &self.0
    }
}

impl<V: Clone> OrderedMap<V> {
    /// `{ ...self, ...other }`: other's values win, new keys append.
    pub fn spread(&mut self, other: &OrderedMap<V>) {
        for (k, v) in &other.0 {
            self.set(k.clone(), v.clone());
        }
    }
}

impl<V> From<Vec<(String, V)>> for OrderedMap<V> {
    fn from(entries: Vec<(String, V)>) -> Self {
        let mut map = OrderedMap::new();
        for (k, v) in entries {
            map.set(k, v);
        }
        map
    }
}

impl<V: Serialize> Serialize for OrderedMap<V> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}
