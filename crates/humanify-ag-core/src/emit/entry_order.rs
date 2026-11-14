//! The runnable tree's ENTRY order (exp227): which order `index.js`
//! requires every split file in.
//!
//! The entry used to list the files in the bundle's first-statement order.
//! That order moves whenever the bundler moves a module, so between two
//! versions a few hundred entry lines changed place with no source change
//! behind them. The entry now keeps the PRIOR tree's order for every file
//! that is still there, and a new file goes right after its predecessor in
//! bundle order.
//!
//! What may move is limited by what runs at load. Loading a file runs its
//! header requires, then its body; an EFFECT file is one whose body does
//! something observable at load (a load-time effect, or a write to another
//! file's binding). The entry order is accepted only when Node, loading it,
//! runs every effect file's body in the same order as it would for the
//! bundle-order entry ([`effect_run_order`]). Where the two disagree, the
//! entry lines that loaded the first disagreeing effect file are PINNED to
//! their bundle-order places, and the order is rebuilt; this repeats until
//! the effect order agrees, or bundle order is kept
//! ([`pinned_entry_order`]). The caller still runs the load-order hazard
//! check on the result and keeps bundle order when it finds one.

use std::collections::HashSet;

use super::align::order_by_hash_sequence;

/// The comment line every emitted entry starts its body with.
pub const ENTRY_MARKER: &str = "// Entry for the runnable split tree";

/// The files an entry requires, in order (paths relative to the tree
/// root), or None when `text` is not an emitted entry.
pub fn parse_entry_modules(text: &str) -> Option<Vec<String>> {
    if !text.lines().any(|l| l.starts_with(ENTRY_MARKER)) {
        return None;
    }
    let modules = text
        .lines()
        .filter_map(|l| l.strip_prefix("require(\"./")?.strip_suffix("\");"))
        .filter(|m| !m.contains('"'))
        .map(str::to_string)
        .collect();
    Some(modules)
}

/// The prior tree's entry order, read through `read` (path relative to the
/// tree root → text). The entry is `index.js`, or `_index.js` … when a
/// split file took that name.
pub fn read_prior_entry_order(read: impl Fn(&str) -> Option<String>) -> Option<Vec<String>> {
    let mut name = String::from("index.js");
    for _ in 0..8 {
        let text = read(&name)?;
        if let Some(modules) = parse_entry_modules(&text) {
            return Some(modules);
        }
        name.insert(0, '_');
    }
    None
}

/// The entry order: `bundle` (file ids, bundle first-statement order)
/// re-ranked by `prior` (paths) where `pinned[f]` is false; the pinned
/// files fill their slots in bundle order. A file missing from the prior
/// follows its bundle-order predecessor.
pub fn stable_entry_order(
    bundle: &[usize],
    paths: &[String],
    prior: &[String],
    pinned: &[bool],
) -> Vec<usize> {
    let in_prior: HashSet<&str> = prior.iter().map(String::as_str).collect();
    let desired = order_by_hash_sequence(bundle, paths, prior, |f| {
        !pinned[f] && in_prior.contains(paths[f].as_str())
    });
    let mut fixed = bundle.iter().copied().filter(|&f| pinned[f]);
    desired
        .into_iter()
        .map(|f| {
            if pinned[f] {
                fixed.next().unwrap_or(f)
            } else {
                f
            }
        })
        .collect()
}

/// The effect files' bodies in the order Node runs them for `entry`:
/// (effect file, the entry line whose load ran it). A file loads its
/// `headers[f]` (header requires, in order) before its body; a file
/// already loading or loaded is not loaded again.
pub fn effect_run_order(
    entry: &[usize],
    headers: &[Vec<usize>],
    effect: &[bool],
) -> Vec<(usize, usize)> {
    fn load(
        f: usize,
        root: usize,
        headers: &[Vec<usize>],
        effect: &[bool],
        seen: &mut [bool],
        out: &mut Vec<(usize, usize)>,
    ) {
        seen[f] = true;
        for &d in &headers[f] {
            if !seen[d] {
                load(d, root, headers, effect, seen, out);
            }
        }
        if effect[f] {
            out.push((f, root));
        }
    }
    let mut seen = vec![false; headers.len()];
    let mut out = Vec::new();
    for &root in entry {
        if !seen[root] {
            load(root, root, headers, effect, &mut seen, &mut out);
        }
    }
    out
}

/// Pinning rounds before bundle order is kept.
const MAX_PIN_ROUNDS: usize = 512;

/// The entry order that keeps the prior's order as far as the effect
/// order allows ([`stable_entry_order`] with the pinned lines held), or
/// None when that is bundle order itself or no agreeing order was found.
pub fn pinned_entry_order(
    bundle: &[usize],
    paths: &[String],
    prior: &[String],
    headers: &[Vec<usize>],
    effect: &[bool],
) -> Option<Vec<usize>> {
    let expected = effect_run_order(bundle, headers, effect);
    let mut pinned = vec![false; paths.len()];
    for _ in 0..MAX_PIN_ROUNDS {
        let order = stable_entry_order(bundle, paths, prior, &pinned);
        let got = effect_run_order(&order, headers, effect);
        let Some(k) = (0..expected.len().max(got.len()))
            .find(|&k| expected.get(k).map(|e| e.0) != got.get(k).map(|g| g.0))
        else {
            return (order != bundle).then_some(order);
        };
        let roots = [expected.get(k), got.get(k)];
        let mut progressed = false;
        for &(_, root) in roots.into_iter().flatten() {
            progressed |= !std::mem::replace(&mut pinned[root], true);
        }
        if !progressed {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod entry_order_test;
