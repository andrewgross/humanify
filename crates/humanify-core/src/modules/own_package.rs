//! What is this bundle's OWN package? (exp183; exp172's rules moved here.)
//!
//! A bundle can be a published package itself: a CLI the corpus installs
//! and bundles from `node_modules/<cli>/bin/…`, an app the index also
//! knows because some other indexed package depends on it (exp177: the
//! `agents` SDK depends on `just-bash`, so the index records just-bash's
//! code as `just-bash`, and the just-bash CLI's own code was claimed as a
//! library). The app's own package is never a library of the app. This
//! module is the ONE owner of the question; its two askers each hand it
//! their own view of the bundle's modules:
//!
//! - library discovery (`libdetect::discovery`, every bundler): the
//!   reference graph's module groups and the claims, once the fixpoint
//!   has run. Claims of an own package are set apart (`selfClaims`), never
//!   claims: the code is app code, named like app code.
//! - the CommonJS app-module decision (`modules::app_factories`, exp172):
//!   the extracted factories, which it keeps in the app.
//!
//! The evidence, each read from what the run sees (never a file name):
//!
//! - **The entry** (exp183): the modules the bundle RUNS (its trailing
//!   glue calls them: `X();`, `module.exports = X();`, `export default
//!   X();`). When the index claims MOST of every entry module's bytes as
//!   one package P, the bundle is P — a library is never the program
//!   itself. All entries must agree: an app whose last statements run a
//!   library's register module and its own main names nothing.
//! - **The front's manifest** (exp172): the entry's unattributed modules
//!   require P's inlined `package.json` (a CLI reading its own version).
//! - **Mutual require** (exp172): P's modules and the entry's
//!   unattributed modules require each other (the app calls into P and P
//!   calls back; a library never calls app code).
//!
//! The front rules are refused when any module of another package
//! requires one of P's (a library depends on P, so P is a dependency).
//! The entry rule is not: the entry is the app by construction.
//!
//! Name-blind on purpose: a package is own only through the bundle's
//! structure, never because its name resembles the app's, so a bundle
//! that vendors a library sharing its name with one of its dependencies
//! still matches that library.

use std::collections::BTreeSet;

/// What a run knows about one module of the bundle, gathered before any
/// name is chosen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleEvidence {
    /// The packages library evidence names: the index's claims on the
    /// module's statements (and, for an extracted factory, its names-mode
    /// placement path and its declared identity's package).
    pub packages: BTreeSet<String>,
    /// The package claiming more than half of the module's bytes.
    pub majority: Option<String>,
    /// The package an inlined `package.json` module declares.
    pub manifest_of: Option<String>,
    /// The module's original source path, when the bundler kept it.
    pub source_path: Option<String>,
    /// Never the app's: an app text asset, or a factory whose references
    /// cannot be planned (exp172).
    pub excluded: bool,
    /// The modules this module's code requires (indices into the same
    /// list).
    pub requires: Vec<usize>,
}

pub(crate) fn is_node_modules_path(path: &str) -> bool {
    path.split(['/', '\\'])
        .any(|segment| segment == "node_modules")
}

/// Library evidence for a module, `own` the app's own packages.
pub(crate) fn library_evidence(e: &ModuleEvidence, own: &BTreeSet<String>) -> bool {
    e.packages.iter().any(|p| !own.contains(p))
        || e.source_path.as_deref().is_some_and(is_node_modules_path)
}

/// Every module reachable from `seeds` through modules `pass` admits (a
/// seed it refuses is not entered).
pub(crate) fn reach(
    evidence: &[ModuleEvidence],
    seeds: &[usize],
    pass: impl Fn(usize) -> bool,
) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<usize> = seeds.to_vec();
    while let Some(i) = stack.pop() {
        if i >= evidence.len() || seen.contains(&i) || !pass(i) {
            continue;
        }
        seen.insert(i);
        stack.extend(evidence[i].requires.iter().copied());
    }
    seen
}

/// The package owning more than half of a module of `module_bytes` bytes,
/// from its claimed `(package, bytes)` (bytes each claim OWNS: a lazy
/// wrapper's own overhead, not its body's).
pub fn majority<'a>(
    owned: impl IntoIterator<Item = (&'a str, u64)>,
    module_bytes: u64,
) -> Option<String> {
    let mut by_package: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for (package, bytes) in owned {
        *by_package.entry(package).or_default() += bytes;
    }
    by_package
        .into_iter()
        .find(|&(_, bytes)| bytes * 2 > module_bytes)
        .map(|(package, _)| package.to_string())
}

/// Claim spans `(package, start, end)` as the bytes each OWNS: spans nest
/// (a wrapper's claim holds its body's), and a byte counts for the
/// INNERMOST claim holding it. For a view that has spans only (the
/// unpack's claim rows); a byte no inner claim holds counts for the outer
/// one.
pub fn innermost_bytes<'a>(
    claims: impl IntoIterator<Item = (&'a str, u32, u32)>,
) -> Vec<(&'a str, u64)> {
    let mut rows: Vec<(u32, u32, &str)> = claims.into_iter().map(|(p, s, e)| (s, e, p)).collect();
    rows.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    let mut owned: Vec<(&str, u64)> = Vec::new();
    let mut credit = |package: &'a str, from: u32, to: u32| {
        if to > from {
            owned.push((package, u64::from(to - from)));
        }
    };
    let mut pos = 0u32;
    let mut open: Vec<(u32, &str)> = Vec::new();
    for (start, end, package) in rows {
        while let Some(&(top_end, top)) = open.last() {
            if top_end > start {
                break;
            }
            credit(top, pos, top_end);
            pos = pos.max(top_end);
            open.pop();
        }
        if let Some(&(_, top)) = open.last() {
            credit(top, pos, start);
        }
        pos = start;
        open.push((end, package));
    }
    while let Some((top_end, top)) = open.pop() {
        credit(top, pos, top_end);
        pos = pos.max(top_end);
    }
    owned
}

/// Per module, the modules that require it.
pub(crate) fn required_by(evidence: &[ModuleEvidence]) -> Vec<Vec<usize>> {
    let n = evidence.len();
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, e) in evidence.iter().enumerate() {
        for &j in &e.requires {
            if j < n && j != i {
                out[j].push(i);
            }
        }
    }
    out
}

/// The bundle's own packages (see the module doc). `entries`: the modules
/// the bundle runs (empty: it runs more than glue, and nothing is own).
pub fn own_packages(evidence: &[ModuleEvidence], entries: &[usize]) -> BTreeSet<String> {
    let entries: Vec<usize> = entries
        .iter()
        .copied()
        .filter(|&i| i < evidence.len() && !evidence[i].excluded)
        .collect();
    let mut own = front_packages(evidence, &entries);
    own.extend(entry_package(evidence, &entries));
    own
}

/// The entry rule: the one package claiming most of every entry module.
fn entry_package(evidence: &[ModuleEvidence], entries: &[usize]) -> Option<String> {
    let mut agreed: Option<&String> = None;
    for &i in entries {
        let p = evidence[i].majority.as_ref()?;
        if agreed.is_some_and(|a| a != p) {
            return None;
        }
        agreed = Some(p);
    }
    agreed.cloned()
}

/// The front rules (exp172): manifest and mutual require, refused when
/// another package depends on the candidate.
fn front_packages(evidence: &[ModuleEvidence], entries: &[usize]) -> BTreeSet<String> {
    let required_by = required_by(evidence);
    let none = BTreeSet::new();
    // The entry's unattributed modules.
    let front = reach(evidence, entries, |i| {
        let e = &evidence[i];
        !e.excluded && e.packages.is_empty() && !library_evidence(e, &none)
    });
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    for &u in &front {
        for &j in &evidence[u].requires {
            let Some(e) = evidence.get(j) else {
                continue;
            };
            if let Some(p) = &e.manifest_of {
                candidates.insert(p.clone());
            }
            // Mutual: P requires the front module that requires P.
            for p in &e.packages {
                let calls_back = required_by[u]
                    .iter()
                    .any(|&r| evidence[r].packages.contains(p));
                if calls_back {
                    candidates.insert(p.clone());
                }
            }
        }
    }
    candidates
        .into_iter()
        .filter(|p| {
            // Refused when another package's module requires one of P's.
            !evidence.iter().enumerate().any(|(i, e)| {
                e.packages.contains(p)
                    && required_by[i]
                        .iter()
                        .any(|&r| evidence[r].packages.iter().any(|q| q != p))
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "own_package_test.rs"]
mod own_package_test;
