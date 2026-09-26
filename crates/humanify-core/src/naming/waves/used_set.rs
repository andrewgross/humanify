//! A function context's `usedIdentifiers` Set (TS `buildContext`), held as
//! SHARED name layers — the one owner of that Set's order and membership.
//!
//! The Set is every binding name from the function's own scope outward,
//! then the program's bindings, then the file's free names, first
//! occurrence kept. The wave processor keeps one per function context for
//! the whole run (the barrier and the retries read it), and on a bundle the
//! outer layers are huge: the module scope of a Bun CJS wrapper holds ~15k
//! names, and a first-version run builds ~20k contexts. Copying those names
//! into every context held ~300M strings — 62 GB peak on the smallest
//! bundle, 175 GB and a crash on 2.1.182 (finding #56). Here a layer is an
//! immutable snapshot of one scope's table, shared (`Arc`) by every context
//! built while that table is unchanged; a context owns only its barrier
//! edits (the renames applied to its own bindings).
//!
//! The ORDER is the build-time order (the TS reads it only before any
//! barrier touches the Set — the prompt's used names, the proximity
//! window); membership sees the barrier's `delete(old); add(new)` edits.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// One scope table's names at snapshot time, first occurrence kept.
#[derive(Debug, Default)]
pub struct NameLayer {
    order: Vec<String>,
    index: HashMap<String, u32>,
}

impl NameLayer {
    pub fn new(names: impl IntoIterator<Item = String>) -> NameLayer {
        let mut layer = NameLayer::default();
        for name in names {
            if !layer.index.contains_key(&name) {
                layer.index.insert(name.clone(), layer.order.len() as u32);
                layer.order.push(name);
            }
        }
        layer
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }
}

/// The Set over its layers (innermost first), plus the context's own
/// barrier edits.
#[derive(Debug)]
pub struct UsedSet {
    layers: Vec<Arc<NameLayer>>,
    /// Per layer: the sorted positions whose name an earlier layer holds
    /// (skipped in the order — the Set keeps the first occurrence).
    shadowed: Vec<Vec<u32>>,
    added: HashSet<String>,
    removed: HashSet<String>,
}

impl UsedSet {
    pub fn new(layers: Vec<Arc<NameLayer>>) -> UsedSet {
        let shadowed = (0..layers.len())
            .map(|i| shadowed_positions(&layers[..i], &layers[i]))
            .collect();
        UsedSet {
            layers,
            shadowed,
            added: HashSet::new(),
            removed: HashSet::new(),
        }
    }

    /// The build-time members in Set (insertion) order.
    pub fn order(&self) -> impl Iterator<Item = &str> {
        self.layers
            .iter()
            .zip(&self.shadowed)
            .flat_map(|(layer, skip)| {
                let mut skip = skip.iter().peekable();
                layer.order.iter().enumerate().filter_map(move |(k, n)| {
                    if skip.peek() == Some(&&(k as u32)) {
                        skip.next();
                        None
                    } else {
                        Some(n.as_str())
                    }
                })
            })
    }

    /// `set.has(name)`, barrier edits included.
    pub fn contains(&self, name: &str) -> bool {
        self.added.contains(name)
            || (!self.removed.contains(name) && self.layers.iter().any(|l| l.contains(name)))
    }

    /// `set.delete(name)`.
    pub fn remove(&mut self, name: &str) {
        self.added.remove(name);
        self.removed.insert(name.to_string());
    }

    /// `set.add(name)`.
    pub fn insert(&mut self, name: &str) {
        self.added.insert(name.to_string());
    }

    /// The shared layers, innermost first.
    pub fn layers(&self) -> &[Arc<NameLayer>] {
        &self.layers
    }

    /// The names this context owns (its barrier edits) — the layers are
    /// shared. The memory bound's observable.
    pub fn owned_names(&self) -> usize {
        self.added.len() + self.removed.len()
    }
}

/// The positions of `layer` whose name one of `earlier` holds, sorted.
/// Each probe walks the smaller side, so a small scope against the
/// module-scope layer costs the small scope's size.
fn shadowed_positions(earlier: &[Arc<NameLayer>], layer: &NameLayer) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for e in earlier {
        if e.len() <= layer.len() {
            out.extend(e.order.iter().filter_map(|n| layer.index.get(n).copied()));
        } else {
            out.extend(
                layer
                    .order
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| e.contains(n))
                    .map(|(k, _)| k as u32),
            );
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod used_set_test;
