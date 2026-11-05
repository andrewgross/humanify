//! A context's TAKEN names (the names the covered scopes' bindings were
//! RENAMED to earlier in this run), held as SHARED per-scope snapshots —
//! the #56 pattern on the renamed-name field.
//!
//! The question the set answers never changed: at prompt and validation
//! time, "is this name already used in a scope I care about" — a TAKEN
//! name is never droppable from an ask's avoid-list, however eligible it
//! looks (the collision fix, 2026-09-28). What changed is the holding:
//! `build_context` used to UNION the scope chain's renamed names into one
//! private `HashSet` per context — a full clone of every name, retained
//! for the whole era, ~590 KB per function pass and ~31 GB of the fresh
//! 2.1.182 run's ~58 GB peak (finding #66's split). Here each layer is
//! the `renamed_layers` map's immutable snapshot of ONE scope's renamed
//! names, shared (`Arc`) by every context built while that scope's table
//! is unchanged; membership over the chain's layers is membership over
//! the union.
//!
//! The snapshot-at-build semantics are the private clone's, exactly: the
//! map REFRESHES by replacing the `Arc` when a scope's table version
//! bumps, never by mutating it, so a context's layers freeze the
//! build-time state the same way the old clone did.

use std::collections::HashSet;
use std::sync::Arc;

/// The taken-name view over a scope chain's snapshots, innermost first.
#[derive(Clone, Debug, Default)]
pub struct TakenNames {
    layers: Vec<Arc<HashSet<String>>>,
}

impl TakenNames {
    pub fn new(layers: Vec<Arc<HashSet<String>>>) -> TakenNames {
        TakenNames { layers }
    }

    /// `taken.has(name)` — membership over the union of the layers.
    pub fn contains(&self, name: &str) -> bool {
        self.layers.iter().any(|l| l.contains(name))
    }

    /// The shared snapshots, innermost first.
    pub fn layers(&self) -> &[Arc<HashSet<String>>] {
        &self.layers
    }

    /// The view's OWN estimated heap bytes for finding #66's gauges
    /// (naming::waves::processor::gauges): the per-context Arc list —
    /// deliberately NOT the shared snapshots' contents (the split counts
    /// each distinct snapshot once, by pointer, across every context
    /// holding it; the `renamed_layers` map's current copies are charged
    /// to `usedSetBytes`).
    pub fn own_bytes(&self) -> u64 {
        self.layers.len() as u64 * std::mem::size_of::<Arc<HashSet<String>>>() as u64
    }
}

#[cfg(test)]
mod taken_test;
