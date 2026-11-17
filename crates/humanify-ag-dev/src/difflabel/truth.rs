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
    /// Per file, the other version's file holding most of its unchanged
    /// units, when that file's choice is this one.
    partner: Vec<Option<u32>>,
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

/// Per file, the single other file with the best score.
fn favourites<S: Ord + Copy>(scores: impl Iterator<Item = (u32, u32, S)>) -> BTreeMap<u32, u32> {
    let mut by_file: BTreeMap<u32, Vec<(S, u32)>> = BTreeMap::new();
    for (file, other, score) in scores {
        by_file.entry(file).or_default().push((score, other));
    }
    let mut out = BTreeMap::new();
    for (file, mut others) in by_file {
        others.sort_unstable_by(|a, b| b.cmp(a));
        if others.get(1).is_none_or(|second| second.0 < others[0].0) {
            out.insert(file, others[0].1);
        }
    }
    out
}

/// Unit texts and export names, each with its count.
type TextCounts = BTreeMap<(bool, u128), u32>;

fn text_counts(file: &InputFile) -> TextCounts {
    let mut out = BTreeMap::new();
    for unit in &file.units {
        *out.entry((false, unit.text_key)).or_default() += 1;
    }
    for name in &file.exports {
        *out.entry((true, super::units::name_key(name))).or_default() += 1;
    }
    out
}

/// For the files still unpaired: `(from file, to file, (texts and export
/// names both have, minus those only one has))`.
fn shared_texts(from: &Side, to: &Side) -> Vec<(u32, u32, (u32, i64))> {
    let unpaired = |side: &Side| -> Vec<(u32, TextCounts)> {
        let files = side.files.iter().enumerate();
        let open = files.filter(|(i, _)| side.partner[*i].is_none());
        open.map(|(i, f)| (i as u32, text_counts(f))).collect()
    };
    let (open_from, open_to) = (unpaired(from), unpaired(to));
    let mut out = Vec::new();
    for (a, texts_a) in &open_from {
        for (b, texts_b) in &open_to {
            let both: u32 = texts_a
                .iter()
                .map(|(key, n)| (*n).min(texts_b.get(key).copied().unwrap_or(0)))
                .sum();
            let units = |texts: &TextCounts| i64::from(texts.values().sum::<u32>());
            if both > 0 {
                let only_one = units(texts_a) + units(texts_b) - 2 * i64::from(both);
                out.push((*a, *b, (both, -only_one)));
            }
        }
    }
    out
}

/// Pair each file with the file that scores best with it, when that
/// file's best is this one.
fn pair_files<S: Ord + Copy>(from: &mut Side, to: &mut Side, scores: &[(u32, u32, S)]) {
    let forward = favourites(scores.iter().copied());
    let backward = favourites(scores.iter().map(|(a, b, s)| (*b, *a, *s)));
    for (a, b) in &forward {
        if backward.get(b) == Some(a) {
            from.partner[*a as usize] = Some(*b);
            to.partner[*b as usize] = Some(*a);
        }
    }
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
            partner: vec![None; files.len()],
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
        let mut shared: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for ((a, b), _) in candidates
            .iter()
            .zip(settle(&names))
            .filter(|(_, alive)| *alive)
        {
            from.set(*a, Standing::Unchanged(pairs));
            to.set(*b, Standing::Unchanged(pairs));
            *shared.entry((a.0, b.0)).or_default() += 1;
            pairs += 1;
        }
        let shared: Vec<_> = shared.into_iter().map(|((a, b), n)| (a, b, n)).collect();
        pair_files(&mut from, &mut to, &shared);
        let by_texts = shared_texts(&from, &to);
        pair_files(&mut from, &mut to, &by_texts);
        Truth { from, to, pairs }
    }

    /// `(paired, from files, to files)`.
    pub fn file_counts(&self) -> (usize, usize, usize) {
        let paired = self.from.partner.iter().flatten().count();
        (paired, self.from.files.len(), self.to.files.len())
    }

    /// The files with no paired file, each with its unit count.
    pub fn unpaired_files(&self, to_side: bool) -> Vec<(&str, usize)> {
        let side = if to_side { &self.to } else { &self.from };
        let files = side.files.iter().zip(&side.partner);
        let open = files.filter(|(_, partner)| partner.is_none());
        open.map(|(f, _)| (f.name.as_str(), f.units.len()))
            .collect()
    }

    /// The other version's file an input file is paired with.
    pub fn paired_file(&self, to_side: bool, name: &str) -> Option<&str> {
        let (side, other) = if to_side {
            (&self.to, &self.from)
        } else {
            (&self.from, &self.to)
        };
        let partner = side.partner[*side.by_name.get(name)?]?;
        Some(other.files[partner as usize].name.as_str())
    }
}

#[cfg(test)]
#[path = "truth_test.rs"]
mod truth_test;
