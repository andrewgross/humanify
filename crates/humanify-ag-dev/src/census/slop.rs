//! `cargo dev census slop [--base REF] [--root D]`: complexity mass,
//! erosion and clone share per crate, as a DELTA against the merge-base of
//! HEAD with `--base` (default `main`) — docs/dev-harness.md §7. ADVISORY.
//!
//! Definitions (rule `slop/1`, [`super::rust_fns::CC_RULE`]; the CC rule,
//! the function set and SLOC are written down in [`super::rust_fns`]):
//! - per function: `mass = CC x sqrt(SLOC)`;
//! - per crate: `erosion = sum(mass of fns with CC > 10) / sum(mass of
//!   all fns)` (0 for a crate with no functions);
//! - per crate: `clone share = SLOC of the fns that are a member of a
//!   finding of the clone census's detector ([`super::clones::detect`]:
//!   identical groups and shared-run pairs, allowlisted or not — the share
//!   measures duplication, not review) / SLOC of all fns`.
//!
//! NOW is the working tree under `<root>/crates` (uncommitted edits
//! count); the BASE is `git merge-base HEAD <base>` read from git blobs —
//! no worktree, no checkout. NON-TEST code is the headline; test code
//! (see [`super::rust_fns`] for which code is test code) is printed apart
//! and never asks for review. Every `#[allow(clippy::cognitive_complexity)]`
//! site is listed by name: clippy's per-function ceiling is silent about
//! them.
//!
//! **When it asks for review (exit 1; the gate prints REVIEW, never
//! FAIL)**: a crate's NON-TEST erosion or clone share rose by more than
//! [`REVIEW_RISE_PP`] percentage points against the base (a crate absent
//! at the base counts from zero), or a cognitive-complexity allow site
//! exists that the base did not have. Exit 2: the base cannot be resolved
//! or a file does not parse.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use super::clones::{detect, scan_root};
use super::rust_fns::{self, AllowSite, CC_RULE, FnUnit, Kind, Source};
use crate::HostEnv;
use crate::settings::Parsed;

/// A rise larger than this (percentage points) asks for review.
pub const REVIEW_RISE_PP: f64 = 0.5;

/// Above this CC a function's mass counts as eroded.
pub const ERODED_ABOVE_CC: u32 = 10;

/// One crate's sums for one kind of code.
#[derive(Clone, Debug, Default)]
pub struct CrateStats {
    pub fns: usize,
    pub sloc: usize,
    pub mass: f64,
    pub eroded_mass: f64,
    pub cloned_sloc: usize,
}

impl CrateStats {
    /// Eroded mass over all mass.
    pub fn erosion(&self) -> f64 {
        if self.mass > 0.0 {
            self.eroded_mass / self.mass
        } else {
            0.0
        }
    }

    /// Cloned SLOC over all SLOC.
    pub fn clone_share(&self) -> f64 {
        if self.sloc > 0 {
            self.cloned_sloc as f64 / self.sloc as f64
        } else {
            0.0
        }
    }
}

/// A tree's measures: per (crate, kind), and its allow sites.
#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub stats: BTreeMap<(String, Kind), CrateStats>,
    pub allows: Vec<AllowSite>,
}

/// `CC x sqrt(SLOC)`.
pub fn mass(u: &FnUnit) -> f64 {
    f64::from(u.cc) * (u.sloc as f64).sqrt()
}

/// Measure one tree.
pub fn measure(sources: &[Source]) -> Result<Metrics, String> {
    let scan = rust_fns::scan(sources)?;
    let cloned: BTreeSet<String> = detect(&scan.units)
        .iter()
        .flat_map(|f| f.members.iter().map(|u| u.id()))
        .collect();
    let mut stats: BTreeMap<(String, Kind), CrateStats> = BTreeMap::new();
    for u in &scan.units {
        let s = stats.entry((u.krate.clone(), u.kind)).or_default();
        let m = mass(u);
        s.fns += 1;
        s.sloc += u.sloc;
        s.mass += m;
        if u.cc > ERODED_ABOVE_CC {
            s.eroded_mass += m;
        }
        if cloned.contains(&u.id()) {
            s.cloned_sloc += u.sloc;
        }
    }
    Ok(Metrics {
        stats,
        allows: scan.allows,
    })
}

/// Run the verb.
pub fn run(p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    let base = p.settings.text("dev.base").unwrap_or_else(|| "main".into());
    census(&scan_root(p), &base)
}

fn both(root: &Path, base: &str) -> Result<(String, Metrics, Metrics), String> {
    let merge_base = crate::proc::git(root, &["merge-base", "HEAD", base])?;
    let now = measure(&rust_fns::worktree_sources(root)?)?;
    let then = measure(&rust_fns::git_sources(root, &merge_base)?)?;
    Ok((merge_base, now, then))
}

/// Run the census on the repository at `root` against `base`.
pub fn census(root: &Path, base: &str) -> (u8, String, String) {
    let (merge_base, now, then) = match both(root, base) {
        Ok(x) => x,
        Err(e) => return (2, String::new(), format!("census slop: {e}\n")),
    };
    let mut out = format!(
        "census slop ({CC_RULE}) — deltas against the merge-base of HEAD with {base} ({})\n\
         mass = CC x sqrt(SLOC); erosion = mass of fns with CC > {ERODED_ABOVE_CC} / all mass; \
         clone share = SLOC in a clone-census finding / all SLOC\n",
        &merge_base[..merge_base.len().min(12)]
    );
    let mut reasons = table(&mut out, "NON-TEST (headline)", Kind::NonTest, &now, &then);
    table(
        &mut out,
        "TEST CODE (apart, never reviewed)",
        Kind::Test,
        &now,
        &then,
    );
    reasons.extend(allow_sites(&mut out, &now, &then));
    if reasons.is_empty() {
        let _ = writeln!(
            out,
            "\nSLOP CENSUS GREEN: no crate's non-test erosion or clone share rose by more than \
             {REVIEW_RISE_PP}pp, and no new cognitive_complexity allow."
        );
        return (0, out, String::new());
    }
    let _ = writeln!(
        out,
        "\nSLOP CENSUS: REVIEW — ADVISORY, a measure of style, not a correctness verdict. Look at:"
    );
    for r in &reasons {
        let _ = writeln!(out, "  - {r}");
    }
    (1, out, String::new())
}

/// Percentage points, with -0.0 shown as +0.0.
fn pp(x: f64) -> f64 {
    let v = x * 100.0;
    if v.abs() < 0.05 { 0.0 } else { v }
}

fn table(out: &mut String, title: &str, kind: Kind, now: &Metrics, then: &Metrics) -> Vec<String> {
    let crates: BTreeSet<&String> = now
        .stats
        .keys()
        .chain(then.stats.keys())
        .filter(|(_, k)| *k == kind)
        .map(|(c, _)| c)
        .collect();
    let _ = writeln!(out, "\n{title}");
    let _ = writeln!(
        out,
        "  {:<22} {:>6} {:>8}   {:<30} {}",
        "crate", "fns", "sloc", "erosion (base -> now)", "clone share (base -> now)"
    );
    let empty = CrateStats::default();
    let mut reasons = Vec::new();
    for c in crates {
        let key = (c.clone(), kind);
        let (n, b) = (
            now.stats.get(&key).unwrap_or(&empty),
            then.stats.get(&key).unwrap_or(&empty),
        );
        let (de, ds) = (
            pp(n.erosion() - b.erosion()),
            pp(n.clone_share() - b.clone_share()),
        );
        let _ = writeln!(
            out,
            "  {c:<22} {:>6} {:>8}   {:<30} {}",
            n.fns,
            n.sloc,
            format!(
                "{:.1}% -> {:.1}% ({de:+.1}pp)",
                pp(b.erosion()),
                pp(n.erosion())
            ),
            format!(
                "{:.1}% -> {:.1}% ({ds:+.1}pp)",
                pp(b.clone_share()),
                pp(n.clone_share())
            ),
        );
        if de > REVIEW_RISE_PP {
            reasons.push(format!("{c}: erosion rose {de:+.1}pp"));
        }
        if ds > REVIEW_RISE_PP {
            reasons.push(format!("{c}: clone share rose {ds:+.1}pp"));
        }
    }
    reasons
}

fn allow_sites(out: &mut String, now: &Metrics, then: &Metrics) -> Vec<String> {
    let before: BTreeSet<(&str, &str)> = then
        .allows
        .iter()
        .map(|a| (a.rel.as_str(), a.name.as_str()))
        .collect();
    let _ = writeln!(
        out,
        "\n#[allow(clippy::cognitive_complexity)] sites: {} (base {})",
        now.allows.len(),
        then.allows.len()
    );
    let mut reasons = Vec::new();
    for a in &now.allows {
        let new = !before.contains(&(a.rel.as_str(), a.name.as_str()));
        let tag = if new { "  [NEW]" } else { "" };
        let _ = writeln!(out, "  {}:{}  {}{tag}", a.rel, a.line, a.name);
        if new {
            reasons.push(format!(
                "new cognitive_complexity allow: {}:{}",
                a.rel, a.name
            ));
        }
    }
    reasons
}

#[cfg(test)]
#[path = "slop_test.rs"]
mod slop_test;
