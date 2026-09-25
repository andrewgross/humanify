//! Match fossil modules across releases — TS `src/split/fossil-match.ts`,
//! the OWNER of that question (docs/responsibility.md).
//!
//! Identity comes from WRITE-SET SHAPE (the segment's sorted rename-blind
//! hash multiset) and IMPORT-EDGE CONTEXT mapped through already-made
//! matches — never from position. Tiers, in the TS's order: unique
//! signature; edge-corroborated (iterated to fixpoint); stem-corroborated;
//! export set; export containment; graded content; graph position.
//!
//! ORDER is part of the output (lesson 4): the TS returns `freshToPrior`, a
//! `Map` in `record()` order, and `assignFossil` claims the inherited paths
//! in that order (which fixes the `used` set's order, which fixes the mint
//! namer's sibling lists). [`FossilMatchResult::matches`] is therefore a
//! `Vec` in record order. Every JS `Map`/`Set` iterated here is iterated in
//! its insertion order; the float scores are the TS's divisions, operand
//! for operand.

use std::collections::{HashMap, HashSet};

/// `FossilSignature`: one module as the matcher sees it.
#[derive(Clone, Debug, Default)]
pub struct FossilSignature {
    /// Sorted rename-blind statement hashes of the segment.
    pub hashes: Vec<String>,
    /// Module indexes (same side) of import edges.
    pub imports: Vec<usize>,
    pub stem: Option<String>,
    pub tokens: Option<Vec<String>>,
    pub declared: Option<Vec<String>>,
}

/// `FossilMatchResult`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FossilMatchResult {
    /// (fresh index, prior index) in record order — the TS `freshToPrior`
    /// Map's iteration order.
    pub matches: Vec<(usize, usize)>,
    /// tier → pairs, in first-recorded order.
    pub tiers: Vec<(String, usize)>,
    /// fresh index → the tier that matched it.
    pub pair_tiers: HashMap<usize, &'static str>,
}

struct MatchState<'a> {
    prior: &'a [FossilSignature],
    fresh: &'a [FossilSignature],
    prior_to_fresh: HashMap<usize, usize>,
    fresh_to_prior: HashMap<usize, usize>,
    p_importers: Vec<Vec<usize>>,
    f_importers: Vec<Vec<usize>>,
    out: FossilMatchResult,
}

/// A JS `new Set(list)`: first occurrences, in order.
fn js_set(list: &[String]) -> Vec<&str> {
    let mut seen: HashSet<&str> = HashSet::new();
    list.iter()
        .map(String::as_str)
        .filter(|s| seen.insert(s))
        .collect()
}

/// |A ∩ B| over the two declared SETS.
fn declared_intersection(a: &[&str], b: &[&str]) -> usize {
    let sb: HashSet<&str> = b.iter().copied().collect();
    a.iter().filter(|n| sb.contains(*n)).count()
}

/// `declaredOverlap`: Jaccard over declared-name sets; 0 when either side
/// has none.
fn declared_overlap(a: &FossilSignature, b: &FossilSignature) -> f64 {
    let sa = js_set(a.declared.as_deref().unwrap_or(&[]));
    let sb = js_set(b.declared.as_deref().unwrap_or(&[]));
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = declared_intersection(&sa, &sb);
    inter as f64 / (sa.len() + sb.len() - inter) as f64
}

/// `declaredContainment`: |∩| / |smaller set|, 0 when either side
/// declares fewer than two names.
fn declared_containment(a: &FossilSignature, b: &FossilSignature) -> f64 {
    let sa = js_set(a.declared.as_deref().unwrap_or(&[]));
    let sb = js_set(b.declared.as_deref().unwrap_or(&[]));
    if sa.len() < 2 || sb.len() < 2 {
        return 0.0;
    }
    declared_intersection(&sa, &sb) as f64 / sa.len().min(sb.len()) as f64
}

const EXPORT_SET_FLOOR: f64 = 0.6;
const CONTAINMENT_FLOOR: f64 = 0.75;
const CONTAINMENT_JACCARD_FLOOR: f64 = 0.4;
const STEM_OVERLAP_FLOOR: f64 = 0.7;
const GRADED_FLOOR: f64 = 0.5;
const GRADED_MARGIN: f64 = 1.5;

fn containment_eligible(a: &FossilSignature, b: &FossilSignature) -> bool {
    declared_containment(a, b) >= CONTAINMENT_FLOOR
        && declared_overlap(a, b) >= CONTAINMENT_JACCARD_FLOOR
}

/// `overlap`: multiset Jaccard over the hash lists.
fn overlap(a: &FossilSignature, b: &FossilSignature) -> f64 {
    let mut cb: HashMap<&str, usize> = HashMap::new();
    for h in &b.hashes {
        *cb.entry(h.as_str()).or_default() += 1;
    }
    let mut ca: HashMap<&str, usize> = HashMap::new();
    for h in &a.hashes {
        *ca.entry(h.as_str()).or_default() += 1;
    }
    // Sum of per-hash minima: order-free, so iterating the hash map is not a
    // decision input — walk `a`'s distinct hashes in list order anyway.
    let mut inter = 0usize;
    let mut done: HashSet<&str> = HashSet::new();
    for h in &a.hashes {
        if done.insert(h.as_str()) {
            inter += ca[h.as_str()].min(cb.get(h.as_str()).copied().unwrap_or(0));
        }
    }
    let union = a.hashes.len() + b.hashes.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

/// `importersOf`: module → the modules importing it (duplicates kept, in
/// module order — the TS pushes once per import edge).
fn importers_of(mods: &[FossilSignature]) -> Vec<Vec<usize>> {
    let mut rev = vec![Vec::new(); mods.len()];
    for (i, m) in mods.iter().enumerate() {
        for &imp in &m.imports {
            if let Some(list) = rev.get_mut(imp) {
                list.push(i);
            }
        }
    }
    rev
}

impl MatchState<'_> {
    fn record(&mut self, pi: usize, fi: usize, tier: &'static str) {
        self.prior_to_fresh.insert(pi, fi);
        self.fresh_to_prior.insert(fi, pi);
        self.out.matches.push((fi, pi));
        match self.out.tiers.iter_mut().find(|(t, _)| t == tier) {
            Some((_, n)) => *n += 1,
            None => self.out.tiers.push((tier.to_string(), 1)),
        }
        self.out.pair_tiers.insert(fi, tier);
    }

    /// `edgeAgreement`: imports and importers that map onto each other
    /// through already-made matches.
    fn edge_agreement(&self, pi: usize, fi: usize) -> usize {
        let fresh_imports: HashSet<usize> = self.fresh[fi].imports.iter().copied().collect();
        let mut agree = self.prior[pi]
            .imports
            .iter()
            .filter(|imp| {
                self.prior_to_fresh
                    .get(imp)
                    .is_some_and(|m| fresh_imports.contains(m))
            })
            .count();
        let fresh_importers: HashSet<usize> = self.f_importers[fi].iter().copied().collect();
        agree += self.p_importers[pi]
            .iter()
            .filter(|imp| {
                self.prior_to_fresh
                    .get(imp)
                    .is_some_and(|m| fresh_importers.contains(m))
            })
            .count();
        agree
    }

    /// `bestPriorByExports`.
    fn best_prior_by_exports(&self, fi: usize) -> (isize, f64) {
        let mut best = (-1isize, 0.0f64);
        for pi in 0..self.prior.len() {
            if self.prior_to_fresh.contains_key(&pi) {
                continue;
            }
            let score = declared_overlap(&self.prior[pi], &self.fresh[fi]);
            if score > best.1 {
                best = (pi as isize, score);
            }
        }
        best
    }

    /// `bestFreshByExports`.
    fn best_fresh_by_exports(&self, pi: usize) -> (isize, f64) {
        let mut best = (-1isize, 0.0f64);
        for fi in 0..self.fresh.len() {
            if self.fresh_to_prior.contains_key(&fi) {
                continue;
            }
            let score = declared_overlap(&self.prior[pi], &self.fresh[fi]);
            if score > best.1 {
                best = (fi as isize, score);
            }
        }
        best
    }

    /// `bestContainedPrior`.
    fn best_contained_prior(&self, fi: usize) -> (isize, f64) {
        let mut best = (-1isize, 0.0f64);
        for pi in 0..self.prior.len() {
            if self.prior_to_fresh.contains_key(&pi)
                || !containment_eligible(&self.prior[pi], &self.fresh[fi])
            {
                continue;
            }
            let score = declared_overlap(&self.prior[pi], &self.fresh[fi]);
            if score > best.1 {
                best = (pi as isize, score);
            }
        }
        best
    }

    /// `bestContainedFresh`.
    fn best_contained_fresh(&self, pi: usize) -> (isize, f64) {
        let mut best = (-1isize, 0.0f64);
        for fi in 0..self.fresh.len() {
            if self.fresh_to_prior.contains_key(&fi)
                || !containment_eligible(&self.prior[pi], &self.fresh[fi])
            {
                continue;
            }
            let score = declared_overlap(&self.prior[pi], &self.fresh[fi]);
            if score > best.1 {
                best = (fi as isize, score);
            }
        }
        best
    }

    /// `exportHeirVeto`.
    fn export_heir_veto(&self, pi: usize, fi: usize) -> bool {
        let pd = self.prior[pi].declared.as_deref().unwrap_or(&[]);
        let fd = self.fresh[fi].declared.as_deref().unwrap_or(&[]);
        if pd.is_empty() || fd.is_empty() {
            return false;
        }
        if declared_overlap(&self.prior[pi], &self.fresh[fi]) > 0.0 {
            return false;
        }
        let (best_fi, score) = self.best_fresh_by_exports(pi);
        best_fi != fi as isize && score >= EXPORT_SET_FLOOR
    }
}

/// `bySignature`: signature → module indexes, keys in first-seen order.
fn by_signature(mods: &[FossilSignature]) -> Vec<(String, Vec<usize>)> {
    let mut index: Vec<(String, Vec<usize>)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for (i, m) in mods.iter().enumerate() {
        let k = m.hashes.join("|");
        match at.get(&k) {
            Some(&slot) => index[slot].1.push(i),
            None => {
                at.insert(k.clone(), index.len());
                index.push((k, vec![i]));
            }
        }
    }
    index
}

fn tier_unique_signature(state: &mut MatchState) {
    let by_sig_p = by_signature(state.prior);
    let by_sig_f: HashMap<String, Vec<usize>> = by_signature(state.fresh).into_iter().collect();
    for (k, ps) in &by_sig_p {
        if let Some(fs) = by_sig_f.get(k)
            && ps.len() == 1
            && fs.len() == 1
        {
            state.record(ps[0], fs[0], "unique-signature");
        }
    }
}

struct Cand {
    fi: usize,
    ov: f64,
    agree: usize,
}

/// `tryEdgeMatch`.
fn try_edge_match(state: &mut MatchState, pi: usize, unmatched_f: &[usize]) -> bool {
    let mut cands: Vec<Cand> = Vec::new();
    for &fi in unmatched_f {
        if state.fresh_to_prior.contains_key(&fi) {
            continue;
        }
        let ov = overlap(&state.prior[pi], &state.fresh[fi]);
        if ov < 0.5 || state.export_heir_veto(pi, fi) {
            continue;
        }
        cands.push(Cand {
            fi,
            ov,
            agree: state.edge_agreement(pi, fi),
        });
    }
    if cands.is_empty() {
        return false;
    }
    // `b.agree - a.agree || b.ov - a.ov` — a stable sort, like V8's.
    cands.sort_by(|a, b| {
        b.agree
            .cmp(&a.agree)
            .then(b.ov.partial_cmp(&a.ov).unwrap_or(std::cmp::Ordering::Equal))
    });
    let best = &cands[0];
    let unique_best = match cands.get(1) {
        None => true,
        Some(second) => best.agree > second.agree || best.ov > second.ov + 1e-9,
    };
    let licensed = (best.agree >= 1 && unique_best) || (cands.len() == 1 && best.ov >= 0.8);
    if !licensed {
        return false;
    }
    let tier = if best.agree >= 1 {
        "edge-corroborated"
    } else {
        "high-overlap-unique"
    };
    let fi = best.fi;
    state.record(pi, fi, tier);
    true
}

/// `uniqueStems`: stem → the single unmatched module holding it, in the
/// stems' first-seen order.
fn unique_stems(
    mods: &[FossilSignature],
    is_matched: impl Fn(usize) -> bool,
) -> Vec<(String, usize)> {
    let mut counts: Vec<(String, Vec<usize>)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for (i, m) in mods.iter().enumerate() {
        let Some(stem) = m.stem.as_ref().filter(|s| !s.is_empty()) else {
            continue;
        };
        if is_matched(i) {
            continue;
        }
        match at.get(stem) {
            Some(&slot) => counts[slot].1.push(i),
            None => {
                at.insert(stem.clone(), counts.len());
                counts.push((stem.clone(), vec![i]));
            }
        }
    }
    counts
        .into_iter()
        .filter(|(_, idx)| idx.len() == 1)
        .map(|(stem, idx)| (stem, idx[0]))
        .collect()
}

fn tier_stem_corroborated(state: &mut MatchState) {
    let prior_stems = unique_stems(state.prior, |i| state.prior_to_fresh.contains_key(&i));
    let fresh_stems: HashMap<String, usize> =
        unique_stems(state.fresh, |i| state.fresh_to_prior.contains_key(&i))
            .into_iter()
            .collect();
    for (stem, pi) in prior_stems {
        let Some(&fi) = fresh_stems.get(&stem) else {
            continue;
        };
        if overlap(&state.prior[pi], &state.fresh[fi]) < STEM_OVERLAP_FLOOR {
            continue;
        }
        state.record(pi, fi, "stem-corroborated");
    }
}

fn tier_export_set(state: &mut MatchState) {
    let mut best_for_fresh: Vec<(usize, usize, f64)> = Vec::new();
    for fi in 0..state.fresh.len() {
        if state.fresh_to_prior.contains_key(&fi) {
            continue;
        }
        let (pi, score) = state.best_prior_by_exports(fi);
        if score >= EXPORT_SET_FLOOR {
            best_for_fresh.push((fi, pi as usize, score));
        }
    }
    for (fi, pi, score) in best_for_fresh {
        if state.fresh_to_prior.contains_key(&fi) || state.prior_to_fresh.contains_key(&pi) {
            continue;
        }
        let (back_fi, back_score) = state.best_fresh_by_exports(pi);
        if back_fi == fi as isize && back_score == score {
            state.record(pi, fi, "export-set");
        }
    }
}

fn tier_export_containment(state: &mut MatchState) {
    let mut best_for_fresh: Vec<(usize, usize, f64)> = Vec::new();
    for fi in 0..state.fresh.len() {
        if state.fresh_to_prior.contains_key(&fi) {
            continue;
        }
        let (pi, score) = state.best_contained_prior(fi);
        if pi >= 0 {
            best_for_fresh.push((fi, pi as usize, score));
        }
    }
    for (fi, pi, score) in best_for_fresh {
        if state.fresh_to_prior.contains_key(&fi) || state.prior_to_fresh.contains_key(&pi) {
            continue;
        }
        let (back_fi, back_score) = state.best_contained_fresh(pi);
        if back_fi == fi as isize && back_score == score {
            state.record(pi, fi, "export-containment");
        }
    }
}

/// `jaccard` over two token SETS.
fn jaccard(a: &HashSet<&str>, b: &HashSet<&str>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (small, big) = if a.len() < b.len() { (a, b) } else { (b, a) };
    let inter = small.iter().filter(|x| big.contains(*x)).count();
    inter as f64 / (a.len() + b.len() - inter) as f64
}

/// `bestGradedCandidate`.
fn best_graded_candidate(
    state: &MatchState,
    ft: &HashSet<&str>,
    unmatched_p: &[usize],
    prior_tokens: &HashMap<usize, HashSet<&str>>,
) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    let mut second_score = 0.0f64;
    for &pi in unmatched_p {
        if state.prior_to_fresh.contains_key(&pi) {
            continue;
        }
        let s = jaccard(ft, &prior_tokens[&pi]);
        match best {
            Some((_, bs)) if s <= bs => {
                if s > second_score {
                    second_score = s;
                }
            }
            _ => {
                second_score = best.map_or(second_score, |(_, bs)| bs);
                best = Some((pi, s));
            }
        }
    }
    let (pi, s) = best?;
    if s < GRADED_FLOOR || (second_score > 0.0 && s < second_score * GRADED_MARGIN) {
        return None;
    }
    Some(pi)
}

fn tier_graded_content(state: &mut MatchState) {
    let unmatched_p: Vec<usize> = (0..state.prior.len())
        .filter(|pi| {
            !state.prior_to_fresh.contains_key(pi)
                && state.prior[*pi]
                    .tokens
                    .as_ref()
                    .is_some_and(|t| !t.is_empty())
        })
        .collect();
    if unmatched_p.is_empty() {
        return;
    }
    let prior = state.prior;
    let prior_tokens: HashMap<usize, HashSet<&str>> = unmatched_p
        .iter()
        .map(|&pi| {
            let set = prior[pi]
                .tokens
                .iter()
                .flatten()
                .map(String::as_str)
                .collect();
            (pi, set)
        })
        .collect();
    let fresh = state.fresh;
    for (fi, fresh_sig) in fresh.iter().enumerate() {
        if state.fresh_to_prior.contains_key(&fi) {
            continue;
        }
        let Some(tok) = fresh_sig.tokens.as_ref().filter(|t| !t.is_empty()) else {
            continue;
        };
        let ft: HashSet<&str> = tok.iter().map(String::as_str).collect();
        if let Some(pick) = best_graded_candidate(state, &ft, &unmatched_p, &prior_tokens)
            && !state.export_heir_veto(pick, fi)
        {
            state.record(pick, fi, "graded-content");
        }
    }
}

/// `bestOf`: the uniquely-best counterpart by agreement, or `None` when
/// the best is tied or has no positive evidence.
fn best_of(others: &[usize], agree: impl Fn(usize) -> usize) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut best_score = 0usize;
    let mut tied = false;
    for &other in others {
        let score = agree(other);
        if score == 0 {
            continue;
        }
        if best.is_none() || score > best_score {
            best = Some(other);
            best_score = score;
            tied = false;
        } else if score == best_score {
            tied = true;
        }
    }
    if tied { None } else { best }
}

fn tier_graph_position(state: &mut MatchState) {
    let unmatched_p: Vec<usize> = (0..state.prior.len())
        .filter(|pi| !state.prior_to_fresh.contains_key(pi))
        .collect();
    let unmatched_f: Vec<usize> = (0..state.fresh.len())
        .filter(|fi| !state.fresh_to_prior.contains_key(fi))
        .collect();
    if unmatched_p.is_empty() || unmatched_f.is_empty() {
        return;
    }
    for &fi in &unmatched_f {
        if state.fresh_to_prior.contains_key(&fi) {
            continue;
        }
        let best_p = best_of(&unmatched_p, |pi| {
            if state.prior_to_fresh.contains_key(&pi) {
                0
            } else {
                state.edge_agreement(pi, fi)
            }
        });
        let Some(best_p) = best_p else { continue };
        let best_f = best_of(&unmatched_f, |other| {
            if state.fresh_to_prior.contains_key(&other) {
                0
            } else {
                state.edge_agreement(best_p, other)
            }
        });
        if best_f != Some(fi) || state.export_heir_veto(best_p, fi) {
            continue;
        }
        state.record(best_p, fi, "graph-position");
    }
}

/// `matchFossilModules`.
pub fn match_fossil_modules(
    prior: &[FossilSignature],
    fresh: &[FossilSignature],
) -> FossilMatchResult {
    let mut state = MatchState {
        prior,
        fresh,
        prior_to_fresh: HashMap::new(),
        fresh_to_prior: HashMap::new(),
        p_importers: importers_of(prior),
        f_importers: importers_of(fresh),
        out: FossilMatchResult::default(),
    };
    tier_unique_signature(&mut state);
    loop {
        let unmatched_f: Vec<usize> = (0..fresh.len())
            .filter(|i| !state.fresh_to_prior.contains_key(i))
            .collect();
        let mut made = 0;
        for pi in 0..prior.len() {
            if state.prior_to_fresh.contains_key(&pi) {
                continue;
            }
            if try_edge_match(&mut state, pi, &unmatched_f) {
                made += 1;
            }
        }
        if made == 0 {
            break;
        }
    }
    tier_stem_corroborated(&mut state);
    tier_export_set(&mut state);
    tier_export_containment(&mut state);
    tier_graded_content(&mut state);
    tier_graph_position(&mut state);
    state.out
}

#[cfg(test)]
mod fossil_match_test;
