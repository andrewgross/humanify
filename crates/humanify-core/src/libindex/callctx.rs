//! Call context (exp153; docs/design/library-names-plan.md step 2): does a
//! bundle unit's OWN calls line up with what the index says the matched
//! print calls?
//!
//! - **The bundle side** is the unit's own references in the statement
//!   reference graph (`graph.out`, never who references it: an INTO edge
//!   describes the app, docs/matching-architecture.md §4), each callee as
//!   its whole print. Runtime helpers and containers are skipped: the
//!   builder records neither (`humanify_index::build::unit_edges`).
//! - **DOWN ONLY — a declared deviation from the cross-version matcher**
//!   (Andrew, 2026-10-26; the row lives in docs/responsibility.md):
//!   library-matching evidence looks only DOWN the graph — a unit's
//!   recursive callee tree, crossing package boundaries (OUT-OF callees
//!   of the row's owner) and below the index's 20-node row floor (the
//!   flat callee-print table, exp155). It never looks UP: callers are app
//!   integration code, INTO stays unread. The cross-version matcher
//!   forbids exactly this asymmetry; the library profile declares it,
//!   and it lives HERE, never in `matching/cascade`.
//! - **The index side** is the INSIDE and OUT-OF callees a `/4` index
//!   records for the rows holding the matched print
//!   ([`ReferenceStore::edges`]), a union over the rows an owner (or its
//!   family) holds.
//! - **The test is containment** ([`Fit`]): tree-shaking drops callers but
//!   never a kept function's callees, so every callee the bundle unit has
//!   should be one the index row records; an index callee the bundle lacks
//!   counts for nothing. The cross-version cascade's callee-hash rung
//!   (`matching::cascade`, `CalleeHashes`) asks the same question with
//!   EQUALITY, because both of its sides are whole programs; here the
//!   bundle is a subset of the build.
//! - **Which unheld callees count** ([`FitScope`], exp155): the plain fit
//!   counts them all — and exp153's census measured where that lies: a
//!   callee the index has never seen (version skew, print drift) refutes
//!   right code, 9.1–9.2 MB of it per bun-dev set. The scoped fit skips
//!   what the index knows nowhere (`Known`) and additionally what the
//!   accepting family holds anywhere — its rows, or flat callee records
//!   under its name (`Family`). A scope other than `All` needs the flat
//!   callee-print table (`edges/2`): an index without it stays silent,
//!   never evidence against.
//! - **The call-tree hash** ([`tree_hashes`]): a unit's print combined
//!   with its callees' tree hashes, bottom-up, a strongly connected
//!   component hashed as one. Callees below the index's row floor are
//!   leaves on both sides (the index records no callee LISTS for a print
//!   with no row — the flat table names its recorders, not what it
//!   calls: a rowless callee stays a leaf by design, exp155).

use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;

use serde::Serialize;

use super::lookup::{LibraryIndex, UnitHit};
use super::table::unit_key;
use super::units::{StatementPrints, UNIT_ROW_MIN_NODES};
use crate::graph::edges::Scope;
use crate::graph::references::ReferenceGraph;
use crate::matching::store::{EdgeRead, ReferenceStore, StoreEdges};

/// One callee of a bundle unit: its whole print's key and node count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Callee {
    pub key: u64,
    pub nodes: u32,
}

/// Per unit of `graph`, its callees ([`Callee`]), sorted by key and once
/// each: the units it references that have a print and are no runtime
/// helper. `prints` is parallel to `graph.units`.
pub fn bundle_callees(
    graph: &ReferenceGraph,
    prints: &[Option<StatementPrints>],
) -> Vec<Vec<Callee>> {
    (0..graph.units.len())
        .map(|u| {
            let mut out: Vec<Callee> = graph.out[u]
                .iter()
                .filter(|&&v| !graph.units[v as usize].runtime)
                .filter_map(|&v| {
                    let w = &prints.get(v as usize)?.as_ref()?.whole;
                    Some(Callee {
                        key: unit_key(&w.hash)?,
                        nodes: w.nodes,
                    })
                })
                .collect();
            out.sort_unstable();
            out.dedup_by_key(|c| c.key);
            out
        })
        .collect()
}

/// How a query's callees sit inside a reference's: `held` of `callees`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Fit {
    pub callees: u32,
    pub held: u32,
}

impl Fit {
    /// The containment of sorted `query` keys in sorted `reference` keys.
    pub fn of(query: impl IntoIterator<Item = u64>, reference: &[u64]) -> Fit {
        let mut fit = Fit::default();
        for k in query {
            fit.callees += 1;
            if reference.binary_search(&k).is_ok() {
                fit.held += 1;
            }
        }
        fit
    }

    /// Every callee held, and at least one callee: the calls line up.
    pub fn confirms(self) -> bool {
        self.callees > 0 && self.held == self.callees
    }

    /// A callee the reference does not record.
    pub fn refutes(self) -> bool {
        self.held < self.callees
    }
}

/// Which of a bundle unit's callees a call-context test counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CalleeFloor {
    /// Every callee with a print.
    #[default]
    All,
    /// Only callees at or over the index's row floor
    /// ([`UNIT_ROW_MIN_NODES`]): a small callee (`var a = {}`, a one-line
    /// constant) is a common print that says little.
    Rowed,
}

impl CalleeFloor {
    /// The config spellings, in this order.
    pub const NAMES: [&'static str; 2] = ["all", "rowed"];

    pub fn name(self) -> &'static str {
        match self {
            CalleeFloor::All => "all",
            CalleeFloor::Rowed => "rowed",
        }
    }

    pub fn parse(text: &str) -> Option<CalleeFloor> {
        match text {
            "all" => Some(CalleeFloor::All),
            "rowed" => Some(CalleeFloor::Rowed),
            _ => None,
        }
    }

    /// The keys of `callees` this floor counts.
    pub fn keys(self, callees: &[Callee]) -> impl Iterator<Item = u64> + '_ {
        callees
            .iter()
            .filter(move |c| self == CalleeFloor::All || c.nodes >= UNIT_ROW_MIN_NODES)
            .map(|c| c.key)
    }
}

/// Which unheld callees a fit may count against a claim (exp155). The
/// plain fit counts them all; the census measured what that costs — a
/// callee the index has never seen refutes right code (9.1–9.2 MB per
/// bun-dev set) because "unknown" and "recorded elsewhere" look alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FitScope {
    /// Today's fit: every floor-passing callee counts, and one the index
    /// does not record is counted as unheld.
    #[default]
    All,
    /// Skip a callee the index knows NOWHERE — no unit row of anyone's,
    /// no flat record: it cannot say which family is right.
    Known,
    /// Additionally skip a callee the accepting family holds anywhere —
    /// a row of its own, or a flat record under its name. Version skew
    /// and print drift inside the family are not evidence against it;
    /// what only another family records still counts.
    Family,
}

impl FitScope {
    /// The config spellings, in this order.
    pub const NAMES: [&'static str; 3] = ["all", "known", "family"];

    pub fn name(self) -> &'static str {
        match self {
            FitScope::All => "all",
            FitScope::Known => "known",
            FitScope::Family => "family",
        }
    }

    pub fn parse(text: &str) -> Option<FitScope> {
        match text {
            "all" => Some(FitScope::All),
            "known" => Some(FitScope::Known),
            "family" => Some(FitScope::Family),
            _ => None,
        }
    }
}

/// A fit with its evidence scoped (exp155): `held` of the `counted`
/// callees, `skipped` the floor-passing callees the scope would not let
/// count — the unknown, and under [`FitScope::Family`] the family-held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ScopedFit {
    pub counted: u32,
    pub held: u32,
    pub skipped: u32,
}

impl ScopedFit {
    /// At least one counted callee, and every counted one held.
    pub fn confirms(self) -> bool {
        self.counted > 0 && self.held == self.counted
    }

    /// A counted callee the reference does not record.
    pub fn refutes(self) -> bool {
        self.held < self.counted
    }
}

impl LibraryIndex {
    /// The callees the index records for `hits` (rows holding one print)
    /// whose owner `accept` takes: the union of their INSIDE and OUT-OF
    /// edges, as sorted print keys. None when the index records no edges
    /// (`/3`) or `accept` takes no hit.
    pub fn hit_callees(&self, hits: &[UnitHit], accept: &dyn Fn(u32) -> bool) -> Option<Vec<u64>> {
        let mut keys: BTreeSet<u64> = BTreeSet::new();
        let mut any = false;
        for &h in hits.iter().filter(|h| accept(h.owner)) {
            for scope in [Scope::Inside, Scope::OutOf] {
                match ReferenceStore::edges(self, h, EdgeRead::Scoped(scope)) {
                    Ok(StoreEdges::Recorded(callees)) => keys.extend(callees.iter().map(|c| c.key)),
                    _ => return None,
                }
            }
            any = true;
        }
        any.then(|| keys.into_iter().collect())
    }

    /// The callees the index records for unit `prints` under the owners
    /// `accept` takes: the rows of its whole print when an accepted owner
    /// holds it, else the rows of each function print at or over the row
    /// floor an accepted owner holds. None: no accepted row, or no edges.
    pub fn print_callees(
        &self,
        prints: &StatementPrints,
        accept: &dyn Fn(u32) -> bool,
    ) -> Option<Vec<u64>> {
        let whole = ReferenceStore::candidates(self, &prints.whole.hash);
        if let Some(keys) = self.hit_callees(&whole, accept) {
            return Some(keys);
        }
        let mut keys: BTreeSet<u64> = BTreeSet::new();
        let mut any = false;
        for f in prints
            .functions
            .iter()
            .filter(|f| f.nodes >= UNIT_ROW_MIN_NODES)
        {
            let hits = ReferenceStore::candidates(self, &f.hash);
            if let Some(k) = self.hit_callees(&hits, accept) {
                keys.extend(k);
                any = true;
            }
        }
        any.then(|| keys.into_iter().collect())
    }

    /// The call-context fit of a bundle unit (`prints`, `callees`) against
    /// the owners `accept` takes; None when no accepted row holds a print
    /// of it (or the index records no edges).
    pub fn call_fit(
        &self,
        prints: &StatementPrints,
        callees: &[Callee],
        floor: CalleeFloor,
        accept: &dyn Fn(u32) -> bool,
    ) -> Option<Fit> {
        let reference = self.print_callees(prints, accept)?;
        Some(Fit::of(floor.keys(callees), &reference))
    }

    /// Does the index know `key` ANYWHERE — a unit row of anyone's, or a
    /// flat callee record? (under [`FitScope::Known`] an unheld callee
    /// this returns false for is skipped, not evidence against).
    fn callee_known(&self, key: u64) -> bool {
        !self.unit_hits(&format!("{key:016x}")).is_empty() || !self.callee_recorders(key).is_empty()
    }

    /// Does the family `accept` takes hold `key` anywhere: a row of its
    /// own, or a flat callee record under its name?
    fn callee_family_held(&self, key: u64, accept: &dyn Fn(u32) -> bool) -> bool {
        self.callee_recorders(key).iter().any(|&o| accept(o))
            || self
                .unit_hits(&format!("{key:016x}"))
                .iter()
                .any(|h| accept(h.owner))
    }

    /// [`LibraryIndex::call_fit`] with its evidence scoped (exp155):
    /// a callee the reference does not record counts only when the scope
    /// lets it — [`FitScope::All`] keeps today's verdict, [`FitScope::Known`]
    /// skips what the index knows nowhere, [`FitScope::Family`] also skips
    /// what the accepting family holds anywhere. A scope other than `All`
    /// reads the flat callee-print table (`edges/2`); an index without it
    /// answers None — silence, never evidence against a claim.
    pub fn call_fit_scoped(
        &self,
        prints: &StatementPrints,
        callees: &[Callee],
        floor: CalleeFloor,
        scope: FitScope,
        accept: &dyn Fn(u32) -> bool,
    ) -> Option<ScopedFit> {
        let reference = self.print_callees(prints, accept)?;
        if scope == FitScope::All {
            let fit = Fit::of(floor.keys(callees), &reference);
            return Some(ScopedFit {
                counted: fit.callees,
                held: fit.held,
                skipped: 0,
            });
        }
        if !self.has_callee_prints() {
            return None;
        }
        let mut fit = ScopedFit::default();
        for k in floor.keys(callees) {
            if reference.binary_search(&k).is_ok() {
                fit.counted += 1;
                fit.held += 1;
            } else if scope == FitScope::Family && self.callee_family_held(k, accept) {
                fit.skipped += 1;
            } else if self.callee_known(k) {
                fit.counted += 1;
            } else {
                fit.skipped += 1;
            }
        }
        Some(fit)
    }
}

/// The call-tree hash of every node reachable from `roots`: a node's own
/// value combined with its successors' tree hashes, computed bottom-up; a
/// strongly connected component is hashed as one (its members' own values
/// and the tree hashes of what it calls outside itself, both sorted), and
/// each member then mixes its own value in. Iterative Tarjan: deep call
/// chains do not grow the stack.
pub fn tree_hashes<N, S, O>(roots: &[N], memo: &mut HashMap<N, u64>, mut succ: S, own: O)
where
    N: Copy + Eq + Hash + Ord,
    S: FnMut(N) -> Vec<N>,
    O: Fn(N) -> u64,
{
    // per visited node still on the stack: (index, lowlink, successors)
    let mut open: HashMap<N, (u32, u32, Vec<N>)> = HashMap::new();
    let mut stack: Vec<N> = Vec::new();
    let mut next = 0u32;
    for &root in roots {
        if memo.contains_key(&root) {
            continue;
        }
        // (node, the next successor to visit)
        let mut work: Vec<(N, usize)> = vec![(root, 0)];
        open.insert(root, (next, next, succ(root)));
        next += 1;
        stack.push(root);
        while let Some(&mut (n, ref mut i)) = work.last_mut() {
            let m = open[&n].2.get(*i).copied();
            if let Some(m) = m {
                *i += 1;
                if memo.contains_key(&m) {
                    continue;
                }
                if let Some(&(mi, _, _)) = open.get(&m) {
                    let e = open.get_mut(&n).expect("open");
                    e.1 = e.1.min(mi);
                } else {
                    let s = succ(m);
                    open.insert(m, (next, next, s));
                    next += 1;
                    stack.push(m);
                    work.push((m, 0));
                }
                continue;
            }
            work.pop();
            let (n_index, n_low, _) = open[&n];
            if let Some(&(p, _)) = work.last() {
                let e = open.get_mut(&p).expect("open");
                e.1 = e.1.min(n_low);
            }
            if n_low != n_index {
                continue;
            }
            // n roots a component: pop it
            let mut members = Vec::new();
            loop {
                let m = stack.pop().expect("component member");
                members.push(m);
                if m == n {
                    break;
                }
            }
            let inside: BTreeSet<N> = members.iter().copied().collect();
            let mut owns: Vec<u64> = members.iter().map(|&m| own(m)).collect();
            owns.sort_unstable();
            let mut outside: Vec<u64> = Vec::new();
            for m in &members {
                let (_, _, ss) = open.remove(m).expect("open member");
                outside.extend(
                    ss.into_iter()
                        .filter(|x| !inside.contains(x))
                        .filter_map(|x| memo.get(&x).copied()),
                );
            }
            outside.sort_unstable();
            outside.dedup();
            let component = mix(&[&owns, &[u64::MAX], &outside]);
            for &m in &members {
                memo.insert(m, mix(&[&[own(m), component]]));
            }
        }
    }
}

/// A 64-bit FNV-1a over the words of `parts` (stable across runs).
fn mix(parts: &[&[u64]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for w in part.iter() {
            for b in w.to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
        }
        h = (h ^ 0xff).wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The bundle side's call-tree hashes: per unit with a print, its tree
/// hash over [`bundle_callees`] (a callee below the row floor is a leaf,
/// as on the index side).
pub fn bundle_tree_hashes(
    prints: &[Option<StatementPrints>],
    callees: &[Vec<Callee>],
) -> HashMap<u32, u64> {
    let key: Vec<Option<(u64, u32)>> = prints
        .iter()
        .map(|p| {
            let w = &p.as_ref()?.whole;
            Some((unit_key(&w.hash)?, w.nodes))
        })
        .collect();
    // callee keys back to units: a key's first unit (equal prints, equal subtrees up to their own callees)
    let mut unit_of: HashMap<u64, u32> = HashMap::new();
    for (u, k) in key.iter().enumerate() {
        if let Some((k, _)) = k {
            unit_of.entry(*k).or_insert(u as u32);
        }
    }
    let roots: Vec<u32> = (0..prints.len() as u32)
        .filter(|&u| key[u as usize].is_some())
        .collect();
    let mut memo = HashMap::new();
    tree_hashes(
        &roots,
        &mut memo,
        |u| {
            let Some((_, nodes)) = key[u as usize] else {
                return Vec::new();
            };
            if nodes < UNIT_ROW_MIN_NODES {
                return Vec::new();
            }
            callees[u as usize]
                .iter()
                .filter_map(|c| unit_of.get(&c.key).copied())
                .filter(|&v| v != u)
                .collect()
        },
        |u| key[u as usize].map_or(0, |k| k.0),
    );
    memo
}

/// The index side's call-tree hashes, computed on demand and memoised:
/// a node is (print key, owner), its successors the union of the callees
/// the owner's rows of that print record (each callee with its own
/// owner); a print the owner holds no row of is a leaf.
pub struct IndexTrees<'i> {
    index: &'i LibraryIndex,
    memo: HashMap<(u64, u32), u64>,
}

impl<'i> IndexTrees<'i> {
    pub fn new(index: &'i LibraryIndex) -> Self {
        IndexTrees {
            index,
            memo: HashMap::new(),
        }
    }

    /// The tree hash of `hash` held by `owner`.
    pub fn of(&mut self, hash: &str, owner: u32) -> Option<u64> {
        let key = unit_key(hash)?;
        let index = self.index;
        let succ = |(k, o): (u64, u32)| -> Vec<(u64, u32)> {
            let hits = index.unit_hits(&format!("{k:016x}"));
            let mut out: BTreeSet<(u64, u32)> = BTreeSet::new();
            for h in hits.into_iter().filter(|h| h.owner == o) {
                if let Some(callees) = index.row_callees(h) {
                    out.extend(callees.iter().map(|c| (c.key, c.owner)));
                }
            }
            out.remove(&(k, o));
            out.into_iter().collect()
        };
        tree_hashes(&[(key, owner)], &mut self.memo, succ, |(k, _)| k);
        self.memo.get(&(key, owner)).copied()
    }
}
