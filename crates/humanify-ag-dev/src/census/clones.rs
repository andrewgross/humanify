//! `cargo dev census clones [--loose] [--root D]`: copy-paste twin
//! functions across `crates/` — the successor of `scripts/clone-census.ts`
//! (which scanned the harness TypeScript with Babel), now over the Rust.
//!
//! **What it sees**: every function [`rust_fns`] lists under
//! `<root>/crates` (default root: this repository), test and non-test,
//! each as its identifier-masked token stream (binding names become
//! order-keyed slots; methods, fields, types, free functions and literals
//! stay verbatim).
//!
//! **What it finds** ([`detect`], the ONE detector — `census slop` reads
//! its clone share from it):
//! - IDENTICAL groups: two or more functions of at least
//!   [`STRICT_MIN_TOKENS`] tokens whose masked streams are identical (the
//!   TS census's only kind).
//! - SHARED-RUN pairs: two functions that are not identical but share a
//!   contiguous run of at least [`RUN_MIN_TOKENS`] masked tokens, with
//!   every binding blinded to one `$` (so a renamed local cannot break a
//!   run). This is the near-copy the TS census was blind to: a twin that
//!   grew one extra statement (the `matching.rs` fingerprint builders,
//!   docs/dev-harness.md §6, share a run of 80-99 tokens). A run that more
//!   than [`RUN_IDIOM_CAP`] functions share is an idiom and pairs nobody.
//!   80 was set on the repository: at 100 the fingerprint twin drops out;
//!   at 60 the list doubles with short shared prologues.
//!
//! **Strict mode (the gate's `census:clones`, ADVISORY)**: every finding
//! with at least one NON-TEST member must be on the allowlist
//! (`dev/census/clone-allowlist.json`: an entry per finding, its sorted
//! `path:functionName` members and a one-line justification), or the
//! census exits 1 — the gate prints REVIEW, never FAIL. Act on a finding
//! by unifying the code (CLAUDE.md code style) or allowlisting it with a
//! justification. Findings made only of TEST code are listed apart and
//! never fail: the TS census skipped test files entirely; this one shows
//! test twins so a reviewer can see them, without gating on them. Unlike
//! the TS census a SAME-FILE finding is reviewed too: the one live twin
//! the design names sits in one file.
//!
//! **Loose mode (`--loose`, always exit 0)**: the review sweep, cross-file
//! only: identical groups between [`LOOSE_MIN_TOKENS`] and the strict
//! cutoff, groups whose BLURRED streams are identical while their exact
//! streams differ (same shape, different literals), and shared runs of
//! [`LOOSE_RUN_MIN_TOKENS`].
//!
//! The cutoffs count masked tokens. 60 was tuned by inspection on the
//! repository (the TS census's 300 masked characters played the same
//! role): it keeps a 6-8 line helper and drops the one-line accessors,
//! `Default` bodies and builders that dominate below it and are idiom,
//! not duplication.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};

use super::rust_fns::{self, FnUnit, Kind};
use crate::HostEnv;
use crate::committed::census::{CloneAllowlist, CloneGroup, clone_allowlist};
use crate::settings::Parsed;

/// The strict (gate) cutoff for an identical group, in masked tokens.
pub const STRICT_MIN_TOKENS: usize = 60;
/// The loose (review) cutoff, in masked tokens.
pub const LOOSE_MIN_TOKENS: usize = 30;
/// The shortest shared run that pairs two functions (strict).
pub const RUN_MIN_TOKENS: usize = 80;
/// The shortest shared run the loose sweep lists.
pub const LOOSE_RUN_MIN_TOKENS: usize = 50;
/// A run shared by more functions than this is an idiom.
pub const RUN_IDIOM_CAP: usize = 20;

/// How a finding's members resemble each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    Identical,
    /// A shared run of at least this many masked tokens.
    SharedRun(usize),
}

/// One finding: its members (two or more) and how they match.
#[derive(Clone, Debug)]
pub struct Finding<'a> {
    pub members: Vec<&'a FnUnit>,
    pub how: How,
}

impl Finding<'_> {
    fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.members.iter().map(|u| u.id()).collect();
        ids.sort();
        ids
    }

    fn files(&self) -> usize {
        self.members
            .iter()
            .map(|u| &u.rel)
            .collect::<BTreeSet<_>>()
            .len()
    }

    fn has_non_test(&self) -> bool {
        self.members.iter().any(|u| u.kind == Kind::NonTest)
    }
}

/// The root a census scans: `--root`, or this repository.
pub fn scan_root(p: &Parsed) -> PathBuf {
    p.settings
        .text("dev.root")
        .map_or_else(crate::paths::repo_root, PathBuf::from)
}

/// Run the verb.
pub fn run(p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    census(&scan_root(p), p.settings.flag_on("dev.loose"))
}

/// Groups of >= 2 units sharing `key`, largest first, then by first member.
fn group_by<'a>(units: &[&'a FnUnit], key: fn(&FnUnit) -> &str) -> Vec<Vec<&'a FnUnit>> {
    let mut by: BTreeMap<&str, Vec<&FnUnit>> = BTreeMap::new();
    for u in units {
        by.entry(key(u)).or_default().push(u);
    }
    let mut groups: Vec<Vec<&FnUnit>> = by.into_values().filter(|g| g.len() >= 2).collect();
    groups.sort_by(|a, b| {
        b[0].tokens
            .cmp(&a[0].tokens)
            .then(a[0].id().cmp(&b[0].id()))
    });
    groups
}

fn exact(u: &FnUnit) -> &str {
    &u.exact
}

fn blurred(u: &FnUnit) -> &str {
    &u.blurred
}

fn as_findings(groups: Vec<Vec<&FnUnit>>) -> Vec<Finding<'_>> {
    groups
        .into_iter()
        .map(|members| Finding {
            members,
            how: How::Identical,
        })
        .collect()
}

/// Hashes of every `k`-token window of a unit's binding-blind stream.
fn windows(u: &FnUnit, k: usize) -> Vec<u64> {
    let blind: Vec<&str> = u
        .exact
        .split(' ')
        .map(|t| if t.starts_with('$') { "$" } else { t })
        .collect();
    let mut out: Vec<u64> = blind
        .windows(k)
        .map(|w| {
            let mut h = DefaultHasher::new();
            w.hash(&mut h);
            h.finish()
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Pairs of non-identical units sharing a `k`-token run, most shared first.
fn shared_run_pairs<'a>(units: &[&'a FnUnit], k: usize) -> Vec<Finding<'a>> {
    let eligible: Vec<&FnUnit> = units.iter().copied().filter(|u| u.tokens >= k).collect();
    let mut postings: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for (n, u) in eligible.iter().enumerate() {
        for h in windows(u, k) {
            postings.entry(h).or_default().push(n);
        }
    }
    let mut shared: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for list in postings.values().filter(|l| l.len() <= RUN_IDIOM_CAP) {
        for (i, &a) in list.iter().enumerate() {
            for &b in &list[i + 1..] {
                *shared.entry((a, b)).or_default() += 1;
            }
        }
    }
    let mut pairs: Vec<(usize, Finding)> = shared
        .into_iter()
        .filter(|((a, b), _)| eligible[*a].exact != eligible[*b].exact)
        .map(|((a, b), n)| {
            let members = vec![eligible[a], eligible[b]];
            let how = How::SharedRun(k);
            (n, Finding { members, how })
        })
        .collect();
    pairs.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.ids().cmp(&y.1.ids())));
    pairs.into_iter().map(|(_, f)| f).collect()
}

/// THE detector (strict): identical groups of at least
/// [`STRICT_MIN_TOKENS`] tokens, then shared-run pairs of
/// [`RUN_MIN_TOKENS`], test and same-file findings included.
pub fn detect(units: &[FnUnit]) -> Vec<Finding<'_>> {
    let all: Vec<&FnUnit> = units.iter().collect();
    let big: Vec<&FnUnit> = all
        .iter()
        .copied()
        .filter(|u| u.tokens >= STRICT_MIN_TOKENS)
        .collect();
    let mut out = as_findings(group_by(&big, exact));
    out.extend(shared_run_pairs(&all, RUN_MIN_TOKENS));
    out
}

fn print_finding(out: &mut String, index: usize, f: &Finding) {
    let how = match f.how {
        How::Identical => format!("identical, {} masked tokens", f.members[0].tokens),
        How::SharedRun(k) => format!("shared run of >= {k} masked tokens"),
    };
    let _ = writeln!(
        out,
        "\ngroup {index} — {} members in {} file(s), {how}",
        f.members.len(),
        f.files()
    );
    for u in &f.members {
        let tag = if u.kind == Kind::Test { "  [test]" } else { "" };
        let _ = writeln!(out, "  {}:{}  {}{tag}", u.rel, u.line, u.name);
    }
    let _ = writeln!(out, "  | {}", f.members[0].excerpt);
}

fn print_section(out: &mut String, title: &str, findings: &[Finding]) {
    let _ = writeln!(out, "\n━━━ {title} ━━━");
    if findings.is_empty() {
        let _ = writeln!(out, "(none)");
    }
    for (i, f) in findings.iter().enumerate() {
        print_finding(out, i + 1, f);
    }
}

/// Run the census on the tree at `root`: (exit code, stdout, stderr).
pub fn census(root: &Path, loose: bool) -> (u8, String, String) {
    let allow = match clone_allowlist(root) {
        Ok(a) => a.value,
        Err(e) => return (2, String::new(), format!("census clones: {e}\n")),
    };
    let scan = match rust_fns::worktree_sources(root).and_then(|s| rust_fns::scan(&s)) {
        Ok(s) => s,
        Err(e) => return (2, String::new(), format!("census clones: {e}\n")),
    };
    if loose {
        (0, run_loose(&scan), String::new())
    } else {
        run_strict(&scan, &allow)
    }
}

struct Partition<'a> {
    fresh: Vec<Finding<'a>>,
    allowed: Vec<(Finding<'a>, &'a CloneGroup)>,
    stale: Vec<&'a CloneGroup>,
}

fn partition<'a>(reviewable: Vec<Finding<'a>>, allow: &'a CloneAllowlist) -> Partition<'a> {
    let mut seen = BTreeSet::new();
    let mut p = Partition {
        fresh: Vec::new(),
        allowed: Vec::new(),
        stale: Vec::new(),
    };
    for f in reviewable {
        let ids = f.ids();
        match allow.entries.iter().position(|e| e.members == ids) {
            Some(i) => {
                seen.insert(i);
                p.allowed.push((f, &allow.entries[i]));
            }
            None => p.fresh.push(f),
        }
    }
    p.stale = allow
        .entries
        .iter()
        .enumerate()
        .filter(|(i, _)| !seen.contains(i))
        .map(|(_, e)| e)
        .collect();
    p
}

fn run_strict(scan: &rust_fns::Scan, allow: &CloneAllowlist) -> (u8, String, String) {
    let eligible = scan
        .units
        .iter()
        .filter(|u| u.tokens >= STRICT_MIN_TOKENS)
        .count();
    let (reviewable, test_only): (Vec<_>, Vec<_>) = detect(&scan.units)
        .into_iter()
        .partition(Finding::has_non_test);
    let p = partition(reviewable, allow);
    let mut out = String::new();
    print_section(
        &mut out,
        "POTENTIAL duplication, unreviewed (for Claude to review)",
        &p.fresh,
    );
    let _ = writeln!(out, "\n━━━ reviewed and accepted (allowlisted) ━━━");
    if p.allowed.is_empty() {
        let _ = writeln!(out, "(none)");
    }
    for (i, (f, e)) in p.allowed.iter().enumerate() {
        print_finding(&mut out, i + 1, f);
        let _ = writeln!(out, "  accepted: {}", e.justification);
    }
    print_section(
        &mut out,
        "twins in test code only (informational, never fail)",
        &test_only,
    );
    let _ = writeln!(out, "\n{}", "═".repeat(72));
    let _ = writeln!(
        out,
        "SUMMARY: {} files under crates/, {} functions, {eligible} >= {STRICT_MIN_TOKENS} masked \
         tokens | {} NEW finding(s), {} allowlisted, {} test-only",
        scan.files,
        scan.units.len(),
        p.fresh.len(),
        p.allowed.len(),
        test_only.len()
    );
    verdict(&mut out, &p)
}

fn verdict(out: &mut String, p: &Partition) -> (u8, String, String) {
    if !p.stale.is_empty() {
        let _ = writeln!(
            out,
            "\nstale allowlist entries (matched no finding — informational):"
        );
        for e in &p.stale {
            let _ = writeln!(out, "  {}", e.members.join("  +  "));
        }
    }
    if p.fresh.is_empty() {
        let _ = writeln!(
            out,
            "\nCENSUS GREEN: every potential-duplication group has been reviewed (unified or \
             allowlisted)."
        );
        return (0, std::mem::take(out), String::new());
    }
    let _ = writeln!(
        out,
        "\nCENSUS: {} unreviewed potential-duplication group(s) — ADVISORY, an automated mini \
         code-review for Claude (or an agent) to act on. Shared structure is a reason to look, \
         not proof of duplication — this census cannot tell copy-paste from coincidence or idiom. \
         Act on each group: unify the code (when it really is one question answered twice — see \
         CLAUDE.md code style) or add its sorted \"path:functionName\" members to \
         dev/census/clone-allowlist.json with a one-line justification (when it is idiom or \
         deliberate). In the check gate this prints REVIEW, never FAIL.",
        p.fresh.len()
    );
    (1, std::mem::take(out), String::new())
}

fn cross_file(findings: Vec<Finding>) -> Vec<Finding> {
    findings.into_iter().filter(|f| f.files() > 1).collect()
}

fn run_loose(scan: &rust_fns::Scan) -> String {
    let mut out = format!(
        "LOOSE REVIEW MODE — periodic modularization sweep, NOT the gate (the gate is strict \
         mode). Always exits 0. Cutoff {LOOSE_MIN_TOKENS} masked tokens; cross-file only.\n"
    );
    let all: Vec<&FnUnit> = scan.units.iter().collect();
    let below: Vec<&FnUnit> = all
        .iter()
        .copied()
        .filter(|u| (LOOSE_MIN_TOKENS..STRICT_MIN_TOKENS).contains(&u.tokens))
        .collect();
    let below_strict = cross_file(as_findings(group_by(&below, exact)));
    // A blurred group is interesting only when exact clones do not wholly
    // explain it: >= 2 distinct exact streams means literals really differ.
    let shaped: Vec<&FnUnit> = all
        .iter()
        .copied()
        .filter(|u| u.tokens >= LOOSE_MIN_TOKENS)
        .collect();
    let shape_twins: Vec<Finding> = cross_file(as_findings(group_by(&shaped, blurred)))
        .into_iter()
        .filter(|f| {
            f.members
                .iter()
                .map(|u| &u.exact)
                .collect::<BTreeSet<_>>()
                .len()
                >= 2
        })
        .collect();
    let runs = cross_file(shared_run_pairs(&all, LOOSE_RUN_MIN_TOKENS));
    print_section(
        &mut out,
        &format!(
            "identical below the strict cutoff ({LOOSE_MIN_TOKENS}-{} masked tokens)",
            STRICT_MIN_TOKENS - 1
        ),
        &below_strict,
    );
    print_section(
        &mut out,
        "same shape, different literals (blurred stream identical)",
        &shape_twins,
    );
    print_section(
        &mut out,
        &format!("shared runs of >= {LOOSE_RUN_MIN_TOKENS} masked tokens"),
        &runs,
    );
    let _ = writeln!(out, "\n{}", "═".repeat(72));
    let _ = writeln!(
        out,
        "SUMMARY (loose): {} files under crates/ | {} identical group(s) below the strict cutoff, \
         {} same-shape group(s) with differing literals, {} shared-run pair(s)",
        scan.files,
        below_strict.len(),
        shape_twins.len(),
        runs.len()
    );
    let _ = writeln!(
        out,
        "Review instrument — always exit 0; the gate is strict mode."
    );
    out
}

#[cfg(test)]
#[path = "clones_test.rs"]
mod clones_test;
