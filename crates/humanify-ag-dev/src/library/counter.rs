//! Python's `collections.Counter`, where the scorer's bytes depend on its
//! exact behaviour: insertion order (`most_common` breaks ties by it),
//! `c[k]` reading 0 for a missing key without inserting it, and `+`
//! dropping every non-positive count.

use std::collections::HashMap;
use std::hash::Hash;

/// An insertion-ordered counter.
#[derive(Clone, Debug)]
pub struct Counter<K, V = i64> {
    keys: Vec<K>,
    vals: Vec<V>,
    at: HashMap<K, usize>,
}

impl<K: Eq + Hash + Clone, V: Copy + Default + std::ops::AddAssign + PartialOrd> Default
    for Counter<K, V>
{
    fn default() -> Self {
        Counter {
            keys: Vec::new(),
            vals: Vec::new(),
            at: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone, V: Copy + Default + std::ops::AddAssign + PartialOrd> Counter<K, V> {
    pub fn new() -> Self {
        Self::default()
    }

    /// `c[k] += v` (inserts k at the end when new).
    pub fn add(&mut self, k: K, v: V) {
        match self.at.get(&k) {
            Some(&i) => self.vals[i] += v,
            None => {
                self.at.insert(k.clone(), self.keys.len());
                self.keys.push(k);
                self.vals.push(v);
            }
        }
    }

    /// `c[k] = v` (inserts k at the end when new).
    pub fn set(&mut self, k: K, v: V) {
        match self.at.get(&k) {
            Some(&i) => self.vals[i] = v,
            None => {
                self.at.insert(k.clone(), self.keys.len());
                self.keys.push(k);
                self.vals.push(v);
            }
        }
    }

    /// `c[k]`: 0 when absent.
    pub fn get<Q>(&self, k: &Q) -> V
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.at.get(k).map_or_else(V::default, |&i| self.vals[i])
    }

    pub fn contains<Q>(&self, k: &Q) -> bool
    where
        K: std::borrow::Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        self.at.contains_key(k)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// (key, count) in insertion order.
    pub fn items(&self) -> impl Iterator<Item = (&K, V)> {
        self.keys.iter().zip(self.vals.iter().copied())
    }

    /// `most_common(1)[0][0]`: the first key with the largest count.
    pub fn most_common_first(&self) -> Option<&K> {
        let mut best: Option<(usize, V)> = None;
        for (i, v) in self.vals.iter().copied().enumerate() {
            if best.is_none_or(|(_, b)| v > b) {
                best = Some((i, v));
            }
        }
        best.map(|(i, _)| &self.keys[i])
    }

    /// `most_common()`: by count, descending; ties in insertion order.
    pub fn most_common(&self) -> Vec<(&K, V)> {
        let mut items: Vec<(&K, V)> = self.items().collect();
        items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        items
    }

    /// `self.update(other)`: add every count, zeros included.
    pub fn update(&mut self, other: &Counter<K, V>) {
        for (k, v) in other.items() {
            self.add(k.clone(), v);
        }
    }

    /// `self + other`: the sums, keeping only positive counts (first
    /// appearance order across self then other).
    pub fn plus(&self, other: &Counter<K, V>) -> Counter<K, V> {
        let mut sum = self.clone();
        sum.update(other);
        let mut out = Counter::new();
        for (k, v) in sum.items() {
            if v > V::default() {
                out.add(k.clone(), v);
            }
        }
        out
    }
}
