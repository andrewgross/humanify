//! A JSON object whose keys are written in the order they were added: the
//! count maps and metadata records whose reading order is their insertion
//! order (serde_json's own map sorts its keys), and [`JsRecord`], an object
//! read and written in JavaScript's record order.

use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::js::array_index;

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
        serialize_entries(&self.0, s)
    }
}

/// `record[key] = value` on a JS object: an existing key keeps its slot
/// and takes the new value; a new array-index key slots in ascending
/// ahead of every ordinary key; a new ordinary key appends.
pub fn js_assign<V>(entries: &mut Vec<(String, V)>, key: String, value: V) {
    if let Some(slot) = entries.iter_mut().find(|(k, _)| *k == key) {
        slot.1 = value;
        return;
    }
    let pos = match array_index(&key) {
        Some(index) => entries
            .iter()
            .position(|(k, _)| array_index(k).is_none_or(|other| other > index))
            .unwrap_or(entries.len()),
        None => entries.len(),
    };
    entries.insert(pos, (key, value));
}

/// A JSON object in JS record order: read with [`js_assign`] per entry (so
/// `JSON.parse`'s order and duplicate-key rule), written in entry order.
#[derive(Clone, Debug, PartialEq)]
pub struct JsRecord<V>(pub Vec<(String, V)>);

impl<V> Default for JsRecord<V> {
    fn default() -> Self {
        JsRecord(Vec::new())
    }
}

impl<V> FromIterator<(String, V)> for JsRecord<V> {
    fn from_iter<I: IntoIterator<Item = (String, V)>>(iter: I) -> Self {
        let mut entries = Vec::new();
        for (k, v) in iter {
            js_assign(&mut entries, k, v);
        }
        JsRecord(entries)
    }
}

impl<V: Serialize> Serialize for JsRecord<V> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serialize_entries(&self.0, s)
    }
}

/// Write `(key, value)` pairs as one JSON object, in their order.
pub fn serialize_entries<V: Serialize, S: Serializer>(
    entries: &[(String, V)],
    s: S,
) -> Result<S::Ok, S::Error> {
    let mut map = s.serialize_map(Some(entries.len()))?;
    for (k, v) in entries {
        map.serialize_entry(k, v)?;
    }
    map.end()
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for JsRecord<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<V>(std::marker::PhantomData<V>);
        impl<'de, V: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<V> {
            type Value = JsRecord<V>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some((k, v)) = map.next_entry::<String, V>()? {
                    js_assign(&mut entries, k, v);
                }
                Ok(JsRecord(entries))
            }
        }
        deserializer.deserialize_map(Visitor(std::marker::PhantomData))
    }
}
