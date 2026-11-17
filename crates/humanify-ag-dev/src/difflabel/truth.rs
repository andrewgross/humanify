//! Which input units are certainly unchanged between two versions.

use std::collections::BTreeMap;

use super::units::{FreeName, InputFile};

/// What the inputs say about one input byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// Inside a unit paired with one unit of the other version; the pair's
    /// number (the outermost such unit's).
    Unchanged(u32),
    /// Its unit's text is in the other version, but not as one certain
    /// pair.
    Duplicate,
    /// Its unit's text is not in the other version.
    Changed,
    OutsideUnits,
    UnknownFile,
}

pub struct Side {
    files: Vec<InputFile>,
    by_name: BTreeMap<String, usize>,
    /// Per file, per unit.
    standing: Vec<Vec<Standing>>,
}

pub struct Truth {
    pub from: Side,
    pub to: Side,
    pub pairs: u32,
}

/// `(file, unit)`.
type UnitRef = (u32, u32);

/// An outside name of one version: `(file, name)`.
type Name = (u32, FreeName);

/// A candidate pair's outside names side by side, each with its vote.
type NamePairs = Vec<(Name, Name, u32)>;

/// A declaration's own name outvotes every reader of it.
const OWN_NAME_VOTE: u32 = 1 << 20;

fn by_text(files: &[InputFile]) -> BTreeMap<u128, Vec<UnitRef>> {
    let mut out: BTreeMap<u128, Vec<UnitRef>> = BTreeMap::new();
    for (f, file) in files.iter().enumerate() {
        for (u, unit) in file.units.iter().enumerate() {
            out.entry(unit.text_key)
                .or_default()
                .push((f as u32, u as u32));
        }
    }
    out
}

/// `None` when the two units read outside names in different places.
fn name_pairs(from: &Side, to: &Side, a: UnitRef, b: UnitRef) -> Option<NamePairs> {
    let (fa, fb) = (&from.files[a.0 as usize], &to.files[b.0 as usize]);
    let (na, nb) = (fa.free_names(a.1), fb.free_names(b.1));
    if na.len() != nb.len() {
        return None;
    }
    let own = |file: &InputFile, unit: u32, name: FreeName| {
        let u = &file.units[unit as usize];
        matches!(name, FreeName::Declared(d) if d >= u.start && d < u.end)
    };
    let mut out = Vec::new();
    for pair in na.into_iter().zip(nb) {
        match pair {
            (None, None) => {}
            (Some(x), Some(y)) => {
                let same_kind = match (x, y) {
                    (FreeName::Global(g), FreeName::Global(h)) => g == h,
                    (FreeName::Declared(_), FreeName::Declared(_)) => true,
                    _ => false,
                };
                if !same_kind {
                    return None;
                }
                let vote = if own(fa, a.1, x) { OWN_NAME_VOTE } else { 1 };
                out.push(((a.0, x), (b.0, y), vote));
            }
            _ => return None,
        }
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

type Votes = BTreeMap<Name, BTreeMap<Name, u32>>;

/// Each name's single most-voted partner.
fn winners(votes: &Votes) -> BTreeMap<Name, Name> {
    let mut out = BTreeMap::new();
    for (name, partners) in votes {
        let best = partners.values().copied().max().unwrap_or(0);
        let mut top = partners.iter().filter(|(_, v)| **v == best);
        if let (Some((partner, _)), None) = (top.next(), top.next()) {
            out.insert(*name, *partner);
        }
    }
    out
}

/// Drop candidates until every declared outside name maps to one name of
/// the other version, both ways.
fn settle(names: &[Option<NamePairs>]) -> Vec<bool> {
    let mut alive: Vec<bool> = names.iter().map(Option::is_some).collect();
    for _ in 0..20 {
        let (mut forward, mut backward) = (Votes::new(), Votes::new());
        for pairs in names
            .iter()
            .zip(&alive)
            .filter_map(|(n, a)| n.as_ref().filter(|_| *a))
        {
            for (x, y, vote) in pairs
                .iter()
                .filter(|p| matches!(p.0.1, FreeName::Declared(_)))
            {
                *forward.entry(*x).or_default().entry(*y).or_default() += vote;
                *backward.entry(*y).or_default().entry(*x).or_default() += vote;
            }
        }
        let (forward, backward) = (winners(&forward), winners(&backward));
        let agrees = |(x, y, _): &(Name, Name, u32)| {
            matches!(x.1, FreeName::Global(_))
                || (forward.get(x) == Some(y) && backward.get(y) == Some(x))
        };
        let mut dropped = false;
        for (i, pairs) in names.iter().enumerate() {
            if alive[i] && !pairs.as_ref().is_some_and(|p| p.iter().all(agrees)) {
                alive[i] = false;
                dropped = true;
            }
        }
        if !dropped {
            break;
        }
    }
    alive
}

impl Side {
    fn new(files: Vec<InputFile>) -> Side {
        Side {
            by_name: files
                .iter()
                .enumerate()
                .map(|(i, f)| (f.name.clone(), i))
                .collect(),
            standing: files
                .iter()
                .map(|f| vec![Standing::Changed; f.units.len()])
                .collect(),
            files,
        }
    }

    fn set(&mut self, unit: UnitRef, standing: Standing) {
        self.standing[unit.0 as usize][unit.1 as usize] = standing;
    }

    /// What the inputs say about the byte: the outermost unchanged unit
    /// holding it, else its smallest unit's standing.
    pub fn at(&self, file: &str, offset: u32) -> Standing {
        let Some(&f) = self.by_name.get(file) else {
            return Standing::UnknownFile;
        };
        let Some(innermost) = self.files[f].innermost(offset) else {
            return Standing::OutsideUnits;
        };
        let mut found = self.standing[f][innermost as usize];
        let mut at = self.files[f].units[innermost as usize].parent;
        while let Some(unit) = at {
            if let unchanged @ Standing::Unchanged(_) = self.standing[f][unit as usize] {
                found = unchanged;
            }
            at = self.files[f].units[unit as usize].parent;
        }
        found
    }
}

impl Truth {
    pub fn compare(from: Vec<InputFile>, to: Vec<InputFile>) -> Truth {
        let (mut from, mut to) = (Side::new(from), Side::new(to));
        let (texts_from, texts_to) = (by_text(&from.files), by_text(&to.files));
        let mut candidates: Vec<(UnitRef, UnitRef)> = Vec::new();
        for (key, a) in &texts_from {
            let Some(b) = texts_to.get(key) else { continue };
            if let ([a], [b]) = (a.as_slice(), b.as_slice()) {
                candidates.push((*a, *b));
            }
            a.iter().for_each(|u| from.set(*u, Standing::Duplicate));
            b.iter().for_each(|u| to.set(*u, Standing::Duplicate));
        }
        let names = humanify_ag_core::par::map_ordered(&candidates, |(a, b)| {
            name_pairs(&from, &to, *a, *b)
        });
        let mut pairs = 0;
        for ((a, b), _) in candidates
            .iter()
            .zip(settle(&names))
            .filter(|(_, alive)| *alive)
        {
            from.set(*a, Standing::Unchanged(pairs));
            to.set(*b, Standing::Unchanged(pairs));
            pairs += 1;
        }
        Truth { from, to, pairs }
    }
}

#[cfg(test)]
#[path = "truth_test.rs"]
mod truth_test;
