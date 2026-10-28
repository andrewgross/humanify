//! The reference-store interface (docs/matching-architecture.md §3.4, step
//! 4): what a matcher looks a fingerprint up in. Two implementations:
//!
//! - **the prior store** — [`FingerprintIndex`], built per run from the
//!   prior release (`prior::match_prior_version`'s prior side; the fresh
//!   side is built the same way and is looked up the same way). Pieces are
//!   index entries, labels are session ids (the prior's names and the
//!   slot mappings hang off them), fingerprints are the match key. One
//!   program: its edges are read whole (cross-version reads every edge).
//! - **the index store** — [`LibraryIndex`], on disk, opened once with ONE
//!   recipe slice (`LibraryIndex::load`: the slice is chosen from the
//!   recipe the run detected once, `libindex::recipe::choose_slice`;
//!   nothing below re-detects). Pieces are unit rows, labels are packages,
//!   fingerprints are the `units` form. It never stores or serves INTO
//!   edges: the index cannot know who calls a library in any given app.
//!   A `/4` index answers INSIDE and OUT-OF from its edge table (a
//!   callee is its owner and its print: the index's callee need not be a
//!   row of its own); a `/3` index records none.
//!
//! Which side a profile enumerates is the profile's choice, not the
//! store's: the cross-version cascade walks the PRIOR's entries in entry
//! order (load-bearing: the match list's insertion order is the rename
//! order) and looks each up in the fresh side; the library profile walks
//! the bundle's units and looks each up in the index.

use std::borrow::Cow;

use crate::graph::edges::Scope;
use crate::hash::forms::{CanonicalForm, LIBRARY_UNITS, MATCH_KEY};
use crate::libindex::lookup::{LibraryIndex, UnitHit};
use crate::libindex::table::{EdgeCallee, RestoreAnswer};
use crate::matching::{FingerprintIndex, IndexKind, IndexNode};

/// Which store answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreId {
    /// A per-run prior side: its entry count (the text it came from is the
    /// run's `--prior-version`).
    Prior { entries: usize },
    /// A library index: its id (sha256 of its manifest) and the one recipe
    /// slice it was loaded with.
    Index { id: String, slice: String },
}

/// What a reference piece is labelled with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Label<'s> {
    /// A prior piece: its session id (the names and file it carries hang
    /// off it).
    Prior { session_id: &'s str },
    /// An index piece: the package that owns it.
    Package { name: &'s str },
}

/// Which edges a rule asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeRead {
    /// Every edge (cross-version: the prior is one program, no regions).
    All,
    /// The edges in one scope relative to the piece's region.
    Scoped(Scope),
}

/// A store's answer to an edge read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreEdges<P> {
    /// The callees the store recorded for the piece.
    Recorded(Vec<P>),
    /// The store records no edges in this scope (absence is not evidence).
    NotRecorded,
}

/// An edge read the store will not answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused(pub String);

/// A store of reference pieces, looked up by fingerprint.
pub trait ReferenceStore {
    /// A reference piece's handle.
    type Piece: Copy;
    /// What an edge read names at the far end: a piece of the prior; for
    /// the index, the callee's owner and print.
    type Callee;

    fn identity(&self) -> StoreId;

    /// The hash form the store's fingerprints are in.
    fn form(&self) -> &'static CanonicalForm;

    /// The pieces holding `fingerprint`, in store order (empty: none):
    /// borrowed from a store held in memory, read for the caller from one
    /// read on demand (the library index's tables).
    fn candidates(&self, fingerprint: &str) -> Cow<'_, [Self::Piece]>;

    fn label(&self, piece: Self::Piece) -> Label<'_>;

    /// The piece's literals, when the store keeps them per piece.
    fn literals(&self, piece: Self::Piece) -> Option<&[String]>;

    /// The piece's recorded callees in `read`'s scope.
    fn edges(
        &self,
        piece: Self::Piece,
        read: EdgeRead,
    ) -> Result<StoreEdges<Self::Callee>, Refused>;

    /// The piece's restored ORIGINAL identifier names (exp159, `names/1`):
    /// what the package's own source declares where the print's
    /// declarations anchor. The index store answers from its names table
    /// (absence, one list, or several — an ambiguous answer, the caller's
    /// to settle); the prior store keeps no names table and answers
    /// absence, never a guess.
    fn restore_names(&self, piece: Self::Piece) -> RestoreAnswer;
}

impl ReferenceStore for FingerprintIndex<'_> {
    /// An index entry.
    type Piece = usize;
    type Callee = usize;

    fn identity(&self) -> StoreId {
        StoreId::Prior {
            entries: self.entries.len(),
        }
    }

    fn form(&self) -> &'static CanonicalForm {
        &MATCH_KEY
    }

    fn candidates(&self, fingerprint: &str) -> Cow<'_, [usize]> {
        Cow::Borrowed(self.bucket(fingerprint).unwrap_or(&[]))
    }

    fn label(&self, piece: usize) -> Label<'_> {
        Label::Prior {
            session_id: &self.entries[piece].session_id,
        }
    }

    /// A function row's string literals (the features table); a binding
    /// keeps none.
    fn literals(&self, piece: usize) -> Option<&[String]> {
        match self.entries[piece].node {
            IndexNode::Function(row) => self.features.get(row).map(|f| &f.string_literals[..]),
            IndexNode::Binding(_) => None,
        }
    }

    /// Every edge only: a prior has no regions, so a scoped read is
    /// refused rather than answered as if every edge were INSIDE.
    fn edges(&self, piece: usize, read: EdgeRead) -> Result<StoreEdges<usize>, Refused> {
        if read != EdgeRead::All {
            return Err(Refused(format!(
                "the prior store has no regions: {read:?} has no meaning on one program"
            )));
        }
        let IndexNode::Function(row) = self.entries[piece].node else {
            return Ok(StoreEdges::Recorded(Vec::new()));
        };
        if self.kind != IndexKind::Functions {
            return Ok(StoreEdges::Recorded(Vec::new()));
        }
        Ok(StoreEdges::Recorded(
            self.graph.functions[row]
                .internal_callees
                .iter()
                .filter_map(|span| self.entry_of_span(*span))
                .collect(),
        ))
    }

    fn restore_names(&self, _piece: usize) -> RestoreAnswer {
        RestoreAnswer::Absent
    }
}

impl ReferenceStore for LibraryIndex {
    /// One indexed occurrence of a unit print.
    type Piece = UnitHit;
    type Callee = EdgeCallee;

    fn identity(&self) -> StoreId {
        StoreId::Index {
            id: self.id.clone(),
            slice: self.slice.clone(),
        }
    }

    fn form(&self) -> &'static CanonicalForm {
        &LIBRARY_UNITS
    }

    fn candidates(&self, fingerprint: &str) -> Cow<'_, [UnitHit]> {
        Cow::Owned(self.unit_hits(fingerprint))
    }

    fn label(&self, piece: UnitHit) -> Label<'_> {
        Label::Package {
            name: self.package_name(piece.owner),
        }
    }

    /// The literal table is per FILE, not per unit: the literal rule reads
    /// the query's text against it (`lookup::literal_claim`).
    fn literals(&self, _piece: UnitHit) -> Option<&[String]> {
        None
    }

    /// INTO (and every edge) is refused: an index never holds how an app
    /// calls a library. INSIDE (callees of the piece's own package) and
    /// OUT-OF (callees of another package) come from a `/4` index's edge
    /// table; a `/3` index has none, so a rule reading them gets
    /// [`StoreEdges::NotRecorded`] — absence, never evidence against.
    fn edges(&self, piece: UnitHit, read: EdgeRead) -> Result<StoreEdges<EdgeCallee>, Refused> {
        match read {
            EdgeRead::Scoped(scope @ (Scope::Inside | Scope::OutOf)) => {
                let Some(callees) = self.row_callees(piece) else {
                    return Ok(StoreEdges::NotRecorded);
                };
                let inside = scope == Scope::Inside;
                Ok(StoreEdges::Recorded(
                    callees
                        .into_iter()
                        .filter(|c| (c.owner == piece.owner) == inside)
                        .collect(),
                ))
            }
            EdgeRead::Scoped(Scope::Into) | EdgeRead::All => Err(Refused(
                "the library index never stores INTO edges: how an app calls a library is \
                 the app's, not the library's"
                    .into(),
            )),
        }
    }

    fn restore_names(&self, piece: UnitHit) -> RestoreAnswer {
        LibraryIndex::restore_names(self, piece)
    }
}

#[cfg(test)]
mod store_test;
