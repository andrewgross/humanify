//! The planner (exp184): a spec ([`crate::spec`]) resolved against the
//! registry snapshot into the package selections the build indexes.
//!
//! The resolution is RULE-DRIVEN and reproducible: the package universe
//! comes from the spec's selections (a public list at a pinned version,
//! and the dependency closure of earlier selections), each package's
//! versions from the version rules (a policy, possibly overridden, plus
//! the `dependents` rule: the version each planned version's dependency
//! ranges resolve to as of the dependent's own publish time — what a
//! fresh install of it got), and the refusal rules drop what they match.
//! Every chosen package@version records the rule that chose it, so a
//! resolved plan can be audited and re-derived from the same spec and
//! snapshot.
//!
//! Resolution is deterministic: the universe is a map (first selection
//! wins), the closure walks dependencies in sorted order breadth-first,
//! and every decision reads only the snapshot. Packuments are fetched in
//! waves ([`Loader::prefetch`]) so the real loader can fetch a whole
//! wave's worth in parallel.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::process::Command;

use humanify_core::libindex::format::Semver;

use crate::asof::{Packument, Selected, grace_cutoff, normalize_time, select_versions};
use crate::build::PackageSelection;
use crate::ranges::{Range, dep_spec};
use crate::registry;
use crate::shard::hex_sha256;
use crate::spec::{
    HighImpactList, IndexSpec, RefusalMatch, Source, Strategy, name_matches, parse_high_impact_list,
};

/// The reserved skip keys for entries no refusal rule named: a list
/// entry that is not a package name (the real list's `-` sentinel), and
/// a package the registry does not know (renamed, unpublished or
/// removed since the list was made).
pub const SKIP_NOT_A_NAME: &str = "not-a-package-name";
pub const SKIP_ABSENT: &str = "absent-from-registry";

/// Whether a list's absent members are an outage. The list is a PINNED
/// public artifact, so its members' absence is the one fetch-class
/// signal a broken fetcher cannot fake its way around (a mis-encoding
/// bug 404ing a class of names — scoped ones, say — always reaches the
/// list, which is full of them). But a SMALL absent fraction is the
/// legitimate tail: packages removed or unpublished since the list pin
/// was made (213 of 17,338 on the default spec's list, 1.2% — exp184).
/// The rule: more than 2% of a list's names absent is an outage; at or
/// under it, they are skipped and recorded.
pub(crate) fn list_absence_outage(absent: usize, total: usize) -> Option<String> {
    if absent * 50 < total {
        return None;
    }
    Some(format!(
        "{absent} of {total} list members came back absent — an outage, \
         not a tail; first-check the fetcher (a mis-encoding bug 404s a \
         whole class of names)"
    ))
}

/// The package a dependency entry installs: an `npm:` alias's VALUE
/// names the real target (the entry's KEY is only the local name — e.g.
/// `@mastra/core`'s `"@a2a-js/sdk-v0_3": "npm:@a2a-js/sdk@~0.3.14"`), any
/// other entry is its own key. Returns (name, range).
fn dep_target(dep: &str, range: &str) -> (String, String) {
    match range.strip_prefix("npm:") {
        Some(rest) => match rest.rfind('@') {
            Some(at) if at > 0 => (rest[..at].to_string(), rest[at + 1..].to_string()),
            _ => (rest.to_string(), "*".to_string()),
        },
        None => (dep.to_string(), range.to_string()),
    }
}

/// The packuments and lists a resolution reads.
pub struct Sources<'a> {
    pub loader: &'a mut dyn Loader,
    /// One `npm-high-impact` list: (which list, the package version, the
    /// list file's expected sha256) → the file's text. The real one
    /// fetches the package's tarball, extracts the file, and verifies
    /// the hash.
    pub list: &'a dyn Fn(HighImpactList, &str, &str) -> Result<String, String>,
}

/// Where packuments come from: batched, so a real loader can fetch a
/// whole wave's worth in parallel before the resolution reads any.
pub trait Loader {
    /// Fetch/prepare the packuments of `names` (batched; idempotent). A
    /// transport error or another HTTP status fails (one retry already
    /// ran inside `registry::load`); a 404/tombstone is WRITTEN as an
    /// absent packument — absence is DATA, and the resolution decides
    /// what it means at its own decision point (enter()); the wave
    /// itself records NOTHING, or an offline replay of the same
    /// snapshot would record a different plan (a name the wave fetched
    /// may never be consulted — its edge may not even be a range).
    fn prefetch(&mut self, names: &BTreeSet<String>) -> Result<(), String>;
    /// The memoised packument of `name`, if the loader holds one
    /// (never a fetch).
    fn peek(&self, name: &str) -> Option<Packument>;
    /// The recorded packument of `name` and its file's sha256 (fetches
    /// when the loader can).
    fn get(&mut self, name: &str) -> Result<(Packument, String), String>;
}

/// A resolved plan: what the spec and the snapshot resolve to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedSpec {
    pub selections: Vec<PackageSelection>,
    /// Packages a `names` refusal rule skipped before their packument
    /// was even needed: rule id → names, rule order.
    pub skipped: BTreeMap<String, Vec<String>>,
    /// Every packument the resolution read: name → its snapshot file's
    /// sha256 (the manifest's registry record).
    pub packuments: BTreeMap<String, String>,
    /// The fetch-time range over those packuments.
    pub earliest_fetch: Option<String>,
    pub latest_fetch: Option<String>,
}

/// A package's universe entry.
#[derive(Clone, Debug)]
struct UniversePkg {
    /// The selection that first named it (index into the spec).
    selection: usize,
    /// Rank within that selection: a list source's 1-based position;
    /// closure members count up from 1 in discovery order.
    rank: usize,
    /// Added by a dependencies selection's closure walk.
    closure: bool,
    /// Some dependencies selection closes over this package's chosen
    /// versions (the selection it came from is one of its `of`).
    closure_root: bool,
    /// Why the package is in the plan at all.
    attribution: String,
}

/// A range a planned version puts on a package: who asked, and as of
/// when it resolves.
struct Pending {
    from: String,
    version: String,
    range: String,
    cutoff: String,
}

/// One planned version, mid-resolution.
#[derive(Clone, Debug)]
struct Planned {
    role: String,
    published: String,
    attribution: String,
}

/// The resolution's working state.
struct Resolution<'a, 'b> {
    spec: &'a IndexSpec,
    src: &'a mut Sources<'b>,
    universe: BTreeMap<String, UniversePkg>,
    /// Universe insertion order (list rank first, closure members after).
    order: Vec<String>,
    planned: BTreeMap<String, BTreeMap<String, Planned>>,
    /// version → (reason) per package, in the order the rules refused.
    refused: BTreeMap<String, Vec<(String, String)>>,
    pending: BTreeMap<String, Vec<Pending>>,
    /// The `dependents` rule's tally: package → resolved version → how
    /// many planned versions' ranges resolved to it, and the first edge
    /// that did (its attribution). Decided once the walk ends.
    votes: BTreeMap<String, BTreeMap<String, (usize, String)>>,
    queue: VecDeque<(String, String)>,
    closure_rank: usize,
    skipped: BTreeMap<String, Vec<String>>,
    skipped_seen: BTreeSet<String>,
    /// List-named packages the registry does not know (counted against
    /// the list's total for the outage rule).
    absent_list: Vec<String>,
    /// How many names the list selections carried (after truncation).
    list_total: usize,
    /// Refusal rules that match versions (applied to chosen versions).
    version_refusals: Vec<(String, &'a RefusalMatch)>,
    needs_deps: bool,
    packuments: BTreeMap<String, String>,
    earliest_fetch: Option<String>,
    latest_fetch: Option<String>,
}

impl Resolution<'_, '_> {
    fn note_fetch(&mut self, p: &Packument, sha: String) {
        self.packuments.insert(p.name.clone(), sha);
        if let Some(t) = p.fetched_at.as_deref().and_then(normalize_time) {
            if self.earliest_fetch.as_ref().is_none_or(|e| t < *e) {
                self.earliest_fetch = Some(t.clone());
            }
            if self.latest_fetch.as_ref().is_none_or(|l| t > *l) {
                self.latest_fetch = Some(t);
            }
        }
    }

    /// The packument of `name`, checking the snapshot carries per-version
    /// metadata when the spec's rules need it.
    fn packument(&mut self, name: &str) -> Result<Packument, String> {
        let (p, sha) = self.src.loader.get(name)?;
        if self.needs_deps && !p.has_deps() {
            return Err(format!(
                "{name}: the packument snapshot has no per-version metadata \
                 (fetched with the time map since exp184); re-take the snapshot"
            ));
        }
        self.note_fetch(&p, sha);
        Ok(p)
    }

    /// Records an absentee the walk CONSULTED (enter() saw the absent
    /// packument): skipped, never entered, never fetched again.
    fn note_absent(&mut self, absent: &[String]) {
        if absent.is_empty() {
            return;
        }
        let seen = self.skipped.entry(SKIP_ABSENT.into()).or_default();
        for n in absent {
            if !seen.contains(n) {
                seen.push(n.clone());
            }
            self.skipped_seen.insert(n.clone());
        }
    }

    /// Whether `name` is skipped by a `names` refusal rule, recording the
    /// skip.
    fn skip_if_refused(&mut self, name: &str) -> bool {
        if let Some(r) = self
            .spec
            .refuse
            .iter()
            .find(|r| matches!(&r.when, RefusalMatch::Names(ps) if ps.iter().any(|p| name_matches(p, name))))
        {
            if self.skipped_seen.insert(name.to_string()) {
                self.skipped
                    .entry(r.id.clone())
                    .or_default()
                    .push(name.to_string());
            }
            return true;
        }
        false
    }

    /// A package enters the universe: planned under its policy, its
    /// versions queued, its pending ranges resolved.
    fn enter(
        &mut self,
        name: &str,
        pkg: UniversePkg,
        member_of: Option<&str>,
    ) -> Result<(), String> {
        if self.skip_if_refused(name) || self.skipped_seen.contains(name) {
            return Ok(());
        }
        let (policy, max, override_id) = policy_for(self.spec, name, &pkg);
        let p = self.packument(name)?;
        // a recorded absence (the snapshot holds an absent packument —
        // the offline replay of a 404/tombstone): a CLOSURE member's
        // absence is data (the graph is full of gone packages — a
        // backlink-spam package's hundreds of fake dependency names are
        // mostly unpublished); a LIST member's absence is counted (see
        // list_absence_outage — a small fraction is the legitimate tail,
        // a large one an outage)
        if p.absent {
            if !pkg.closure {
                self.absent_list.push(name.to_string());
            }
            self.note_absent(&[name.to_string()]);
            return Ok(());
        }
        let mut selected = union_of(&p, &self.spec.as_of, policy);
        if let Some(max) = max {
            cap_newest(&mut selected, max);
        }
        let rule = format!(
            "sel={} rule={}{}{}",
            self.spec.select[pkg.selection].id,
            policy.name(),
            max.map(|m| format!(" max={m}")).unwrap_or_default(),
            match override_id {
                Some(id) => format!(" override={id}"),
                None => String::new(),
            }
        );
        let mut versions = Vec::new();
        for s in selected {
            self.planned
                .entry(name.to_string())
                .or_default()
                .entry(s.version.clone())
                .or_insert(Planned {
                    role: s.role.clone(),
                    published: s.published.clone(),
                    attribution: rule.clone(),
                });
            versions.push((s.version, s.published));
        }
        let pkg = UniversePkg {
            attribution: match member_of {
                Some(from) => format!("{} member-of={from}", pkg.attribution),
                None => pkg.attribution,
            },
            ..pkg
        };
        self.order.push(name.to_string());
        self.universe.insert(name.to_string(), pkg);
        // the spec's version-matching refusal rules
        for (version, _) in &versions {
            if let Some(id) = self.version_refusal(&p, name, version) {
                self.planned.get_mut(name).expect("planned").remove(version);
                let reason = self.reason_of(&id);
                self.record_refused(name, version, &reason);
            }
        }
        for (v, _) in &versions {
            if self.planned.get(name).is_some_and(|m| m.contains_key(v)) {
                self.queue.push_back((name.to_string(), v.clone()));
            }
        }
        self.resolve_pending(name)?;
        Ok(())
    }

    fn reason_of(&self, id: &str) -> String {
        self.spec
            .refuse
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.reason.clone())
            .unwrap_or_default()
    }

    fn record_refused(&mut self, name: &str, version: &str, reason: &str) {
        let rs = self.refused.entry(name.to_string()).or_default();
        if !rs.iter().any(|(v, _)| v == version) {
            rs.push((version.to_string(), reason.to_string()));
        }
    }

    /// Resolve every range planned versions have put on `name` — the
    /// `dependents` rule: the version each range resolved to as of the
    /// requester's own publish time is TALLIED (planned, capped, by
    /// `finish_dependents` once the walk ends; a dependent version's own
    /// dependencies are never walked, so the rule cannot cascade).
    fn resolve_pending(&mut self, name: &str) -> Result<(), String> {
        let Some(pendings) = self.pending.remove(name) else {
            return Ok(());
        };
        if self.skipped_seen.contains(name) {
            return Ok(()); // absent or refused: the edges just die
        }
        let p = self.packument(name)?;
        for d in pendings {
            // the installer's rule: the exact cutoff, else the grace one
            let resolved = resolve_range(&p, &d.range, &d.cutoff)
                .or_else(|| grace_cutoff(&d.cutoff).and_then(|g| resolve_range(&p, &d.range, &g)));
            let Some(resolved) = resolved else {
                continue;
            };
            let first = format!("{}@{} range={}", d.from, d.version, d.range);
            self.votes
                .entry(name.to_string())
                .or_default()
                .entry(resolved)
                .or_insert((0, first))
                .0 += 1;
        }
        Ok(())
    }

    /// The `dependents` rule's decision: per package, the `dependents`
    /// most-resolved versions not already planned or refused (ties: the
    /// higher version), each planned as a `dependent`.
    fn finish_dependents(&mut self) -> Result<(), String> {
        let cap = self.spec.versions.dependents;
        let votes = std::mem::take(&mut self.votes);
        for (name, tally) in votes {
            if cap == 0 || !self.universe.contains_key(&name) {
                continue;
            }
            let mut ranked: Vec<(String, usize, String)> = tally
                .into_iter()
                .filter(|(v, _)| {
                    !self.planned.get(&name).is_some_and(|m| m.contains_key(v))
                        && !self
                            .refused
                            .get(&name)
                            .is_some_and(|rs| rs.iter().any(|(r, _)| r == v))
                })
                .map(|(v, (n, first))| (v, n, first))
                .collect();
            ranked.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| humanify_core::libindex::format::compare_versions(&b.0, &a.0))
            });
            ranked.truncate(cap);
            if ranked.is_empty() {
                continue;
            }
            let p = self.packument(&name)?;
            let sel = self.spec.select[self.universe[&name].selection].id.clone();
            for (version, n, first) in ranked {
                if let Some(id) = self.version_refusal(&p, &name, &version) {
                    let reason = self.reason_of(&id);
                    self.record_refused(&name, &version, &reason);
                    continue;
                }
                let published = p.time.get(&version).and_then(|t| normalize_time(t));
                self.planned.entry(name.clone()).or_default().insert(
                    version,
                    Planned {
                        role: "dependent".into(),
                        published: published.unwrap_or_default(),
                        attribution: format!("sel={sel} dependents={n} first={first}"),
                    },
                );
            }
        }
        Ok(())
    }

    /// The first version-matching refusal rule `name@version` meets.
    fn version_refusal(&self, p: &Packument, name: &str, version: &str) -> Option<String> {
        let meta = p.deps.get(version);
        self.version_refusals.iter().find_map(|(id, when)| {
            let matched = match when {
                RefusalMatch::Versions(vs) => vs.iter().any(|v| v == &format!("{name}@{version}")),
                RefusalMatch::Platform => meta.is_some_and(|m| m.platform),
                RefusalMatch::UnpackedSizeOver(over) => meta
                    .and_then(|m| m.unpacked_size)
                    .is_some_and(|s| s > *over),
                RefusalMatch::Names(_) => false,
            };
            matched.then(|| id.clone())
        })
    }

    /// Walk one planned version's dependencies: every range is recorded
    /// for the `dependents` rule, and a closure member's dependencies
    /// join the universe.
    fn process(&mut self, name: &str, version: &str) -> Result<(), String> {
        if self.skipped_seen.contains(name) {
            return Ok(()); // absent or refused: nothing to walk
        }
        let is_closure = self
            .universe
            .get(name)
            .is_some_and(|u| u.closure_root || u.closure);
        let p = self.packument(name)?;
        let Some(meta) = p.deps.get(version) else {
            return Ok(());
        };
        let published = p
            .time
            .get(version)
            .and_then(|t| normalize_time(t))
            .unwrap_or_default();
        for (dep, range_text) in meta.install_deps() {
            let range_text = if range_text.trim().is_empty() {
                "*".to_string()
            } else {
                range_text
            };
            // `npm:<package>@<range>`: the dependency is the alias's
            // TARGET at the target's range — the alias name is never a
            // package of its own. A spec that is not a range (a git or
            // tarball URL, a `file:` path) resolves nothing.
            let (dep, range_text) = dep_target(&dep, &range_text);
            if Range::parse(&range_text).is_none() {
                continue;
            }
            if self.skip_if_refused(&dep) {
                continue;
            }
            let edge = Pending {
                from: name.to_string(),
                version: version.to_string(),
                range: range_text,
                cutoff: published.clone(),
            };
            if self.universe.contains_key(&dep) {
                if self.spec.versions.dependents > 0 {
                    self.pending.entry(dep.clone()).or_default().push(edge);
                    self.resolve_pending(&dep)?;
                }
                continue;
            }
            if !is_closure {
                // not a closure member: the dependency is out of scope —
                // but the range is recorded, so a LATER closure entry of
                // the package resolves it (enter's resolve_pending)
                if self.spec.versions.dependents > 0 {
                    self.pending.entry(dep).or_default().push(edge);
                }
                continue;
            }
            let Some(sel) = self.closure_selection_of(name) else {
                continue;
            };
            self.closure_rank += 1;
            let rank = self.closure_rank;
            let pkg = UniversePkg {
                selection: sel,
                rank,
                closure: true,
                closure_root: false,
                attribution: format!("sel={} rank={rank}", self.spec.select[sel].id),
            };
            // the edge's range rides in too: enter's resolve_pending
            // applies it, so the version the edge resolved to joins the
            // member's policy picks
            if self.spec.versions.dependents > 0 {
                self.pending.entry(dep.clone()).or_default().push(edge);
            }
            self.enter(&dep, pkg, Some(&format!("{name}@{version}")))?;
        }
        Ok(())
    }

    /// The dependencies selection that closes over `name`'s selection
    /// (the first one whose `of` lists it).
    fn closure_selection_of(&self, name: &str) -> Option<usize> {
        let pkg = self.universe.get(name)?;
        self.spec.select.iter().position(|s| match &s.source {
            Source::Dependencies { of } => {
                of.iter().any(|o| *o == self.spec.select[pkg.selection].id)
            }
            _ => false,
        })
    }

    /// Process the queue in waves until it empties: each wave prefetches
    /// the packuments it will need (the queued packages and the
    /// dependencies of their planned versions), then processes the queue
    /// in FIFO order.
    fn drain(&mut self) -> Result<(), String> {
        while !self.queue.is_empty() {
            let mut want: BTreeSet<String> = self.queue.iter().map(|(n, _)| n.clone()).collect();
            for (name, version) in self.queue.iter() {
                if let Some(p) = self.src.loader.peek(name)
                    && let Some(meta) = p.deps.get(version)
                {
                    want.extend(
                        meta.install_deps()
                            .into_iter()
                            .map(|(d, r)| dep_target(&d, &r).0),
                    );
                }
            }
            // the wave records NOTHING: what it fetched may never be
            // consulted (a dep the walk skips at the range guard), and a
            // name the offline replay already holds would otherwise be
            // recorded differently — enter() is the only decision point
            self.src.loader.prefetch(&want)?;
            let wave: Vec<(String, String)> = self.queue.drain(..).collect();
            for (name, version) in wave {
                self.process(&name, &version)?;
            }
        }
        Ok(())
    }

    fn finish(self) -> ResolvedSpec {
        let mut selections = Vec::new();
        for name in &self.order {
            let pkg = self.universe.get(name).expect("ordered universe");
            let mut vs: Vec<crate::build::PlannedVersion> = self
                .planned
                .get(name)
                .map(|versions| {
                    versions
                        .iter()
                        .map(|(version, p)| crate::build::PlannedVersion {
                            version: version.clone(),
                            role: p.role.clone(),
                            published: p.published.clone(),
                            attribution: Some(p.attribution.clone()),
                        })
                        .collect()
                })
                .unwrap_or_default();
            vs.sort_by(|a, b| {
                humanify_core::libindex::format::compare_versions(&a.version, &b.version)
            });
            vs.reverse();
            selections.push(PackageSelection {
                name: name.clone(),
                versions: vs,
                refused: self.refused.get(name).cloned().unwrap_or_default(),
                attribution: Some(pkg.attribution.clone()),
            });
        }
        ResolvedSpec {
            selections,
            skipped: self.skipped,
            packuments: self.packuments,
            earliest_fetch: self.earliest_fetch,
            latest_fetch: self.latest_fetch,
        }
    }
}

/// The highest stable, installable version published by `cutoff` that
/// `range` resolves to (what `npm install --before=cutoff` gets; the
/// caller retries at [`grace_cutoff`] as the installer does). None:
/// an unresolvable spec (a git/file/workspace URL, a tag) or nothing
/// satisfies it.
fn resolve_range(p: &Packument, range_text: &str, cutoff: &str) -> Option<String> {
    let spec = dep_spec(&p.name, range_text)?;
    let mut best: Option<(String, Semver)> = None;
    for v in &p.versions {
        let Some(s) = Semver::parse(v) else {
            continue;
        };
        if s.is_prerelease() {
            continue;
        }
        let published = p.time.get(v).and_then(|t| normalize_time(t));
        let Some(published) = published else {
            continue;
        };
        if published.as_str() > cutoff || !spec.range.satisfies(&s) {
            continue;
        }
        if best.as_ref().is_none_or(|(_, bs)| bs < &s) {
            best = Some((v.clone(), s));
        }
    }
    best.map(|(v, _)| v)
}

/// Which packages a resolution plans, and by which rules.
pub fn resolve(spec: &IndexSpec, src: &mut Sources) -> Result<ResolvedSpec, String> {
    let needs_deps = needs_deps(spec);
    let version_refusals: Vec<(String, &RefusalMatch)> = spec
        .refuse
        .iter()
        .filter(|r| !matches!(r.when, RefusalMatch::Names(_)))
        .map(|r| (r.id.clone(), &r.when))
        .collect();
    let mut res = Resolution {
        spec,
        src,
        universe: BTreeMap::new(),
        order: Vec::new(),
        planned: BTreeMap::new(),
        refused: BTreeMap::new(),
        pending: BTreeMap::new(),
        votes: BTreeMap::new(),
        queue: VecDeque::new(),
        closure_rank: 0,
        skipped: BTreeMap::new(),
        skipped_seen: BTreeSet::new(),
        absent_list: Vec::new(),
        list_total: 0,
        version_refusals,
        needs_deps,
        packuments: BTreeMap::new(),
        earliest_fetch: None,
        latest_fetch: None,
    };
    // seed every list selection, then process (the closure walks from the
    // `of` selections' planned versions during the drain)
    for (si, sel) in spec.select.iter().enumerate() {
        if let Source::NpmHighImpact {
            list,
            version,
            sha256,
            limit,
        } = &sel.source
        {
            let text = (res.src.list)(*list, version, sha256)?;
            let (mut names, rejects) = parse_high_impact_list(&text, *list)?;
            if !rejects.is_empty() {
                let seen = res.skipped.entry(SKIP_NOT_A_NAME.into()).or_default();
                for r in &rejects {
                    if !seen.contains(r) {
                        seen.push(r.clone());
                    }
                }
            }
            if let Some(l) = limit {
                names.truncate(*l);
            }
            let closure_root = is_closure_root(spec, si);
            res.list_total += names.len();
            let want: BTreeSet<String> = names.iter().cloned().collect();
            // the wave's fetches warm the snapshot; the absent names are
            // NOT recorded here — enter() is the decision point (it
            // counts them against the list's total)
            res.src.loader.prefetch(&want)?;
            for (i, name) in names.into_iter().enumerate() {
                let rank = i + 1;
                if res.universe.contains_key(&name) {
                    continue; // an earlier selection named it
                }
                let pkg = UniversePkg {
                    selection: si,
                    rank,
                    closure: false,
                    closure_root,
                    attribution: format!("sel={} rank={rank}", spec.select[si].id),
                };
                res.enter(&name, pkg, None)?;
            }
        }
    }
    if let Some(outage) = list_absence_outage(res.absent_list.len(), res.list_total) {
        let first = res.absent_list.first().map(String::as_str).unwrap_or("?");
        return Err(format!("{outage} first {first}"));
    }
    res.drain()?;
    res.finish_dependents()?;
    Ok(res.finish())
}

/// Whether any dependencies selection closes over the selection `si`.
fn is_closure_root(spec: &IndexSpec, si: usize) -> bool {
    spec.select.iter().any(|s| match &s.source {
        Source::Dependencies { of } => of.iter().any(|o| *o == spec.select[si].id),
        _ => false,
    })
}

/// Whether the spec's rules need per-version metadata at all.
fn needs_deps(spec: &IndexSpec) -> bool {
    spec.versions.dependents > 0
        || spec
            .select
            .iter()
            .any(|s| matches!(s.source, Source::Dependencies { .. }))
        || spec.refuse.iter().any(|r| {
            matches!(
                r.when,
                RefusalMatch::Platform | RefusalMatch::UnpackedSizeOver(_)
            )
        })
}

/// Every version some strategy of `strategy` chose, each once (the first
/// strategy to choose it names its role).
fn union_of(p: &Packument, as_of: &str, strategy: &Strategy) -> Vec<Selected> {
    let mut out: Vec<Selected> = Vec::new();
    for &policy in &strategy.0 {
        for s in select_versions(p, as_of, &[], policy).selected {
            if !out.iter().any(|o| o.version == s.version) {
                out.push(s);
            }
        }
    }
    out
}

/// Keep the `max` highest versions of a policy's choice (a version that
/// does not parse as semver sorts last).
fn cap_newest(selected: &mut Vec<Selected>, max: usize) {
    selected.sort_by(|a, b| Semver::parse(&b.version).cmp(&Semver::parse(&a.version)));
    selected.truncate(max);
}

/// The planner's policy per package: the first override whose match
/// holds, else the default; with its depth cap.
fn policy_for<'a>(
    spec: &'a IndexSpec,
    name: &str,
    pkg: &UniversePkg,
) -> (&'a Strategy, Option<usize>, Option<String>) {
    for o in &spec.versions.overrides {
        let when = &o.when;
        let names_ok = when.names.is_empty() || when.names.iter().any(|p| name_matches(p, name));
        let sel_ok = when
            .selection
            .as_deref()
            .is_none_or(|s| spec.select[pkg.selection].id == s);
        let rank_ok = when.rank_below.is_none_or(|n| pkg.rank < n);
        if names_ok && sel_ok && rank_ok {
            return (&o.policy, o.max, Some(o.id.clone()));
        }
    }
    (&spec.versions.default, spec.versions.max, None)
}

/// The real list fetcher: the `npm-high-impact` package at the pinned
/// version (`npm pack`), the list file extracted with `tar`, and its
/// sha256 verified against the spec's. Cached under `cache` — a pinned
/// npm tarball is immutable, so a cached copy verified by hash replays.
pub fn fetch_high_impact_list(
    list: HighImpactList,
    version: &str,
    sha256: &str,
    cache: &std::path::Path,
) -> Result<String, String> {
    let file = list.file();
    let dir = cache.join(format!("npm-high-impact@{version}"));
    let out = dir.join(format!("{}.list", list.const_name()));
    if let Ok(bytes) = std::fs::read(&out)
        && hex_sha256(&bytes) == sha256
    {
        return Ok(String::from_utf8_lossy(&bytes).into_owned());
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    const DEADLINE: std::time::Duration = std::time::Duration::from_secs(5 * 60);
    let packed = humanify_core::subprocess::run(
        Command::new("npm")
            .arg("pack")
            .arg(format!("npm-high-impact@{version}"))
            .arg("--pack-destination")
            .arg(&dir)
            .arg("--silent"),
        None,
        DEADLINE,
    )
    .map_err(|e| format!("npm pack npm-high-impact@{version}: {e}"))?;
    if !packed.status.success() {
        return Err(format!(
            "npm pack npm-high-impact@{version}: {}",
            String::from_utf8_lossy(&packed.stderr).trim()
        ));
    }
    let tgz = String::from_utf8_lossy(&packed.stdout)
        .lines()
        .next_back()
        .unwrap_or_default()
        .trim()
        .to_string();
    let extracted = dir.join("package").join(file);
    let untar = humanify_core::subprocess::run(
        Command::new("tar")
            .arg("-xzf")
            .arg(dir.join(&tgz))
            .arg("-C")
            .arg(&dir)
            .arg(format!("package/{file}")),
        None,
        DEADLINE,
    )
    .map_err(|e| format!("tar {file}: {e}"))?;
    if !untar.status.success() {
        return Err(format!(
            "tar {file}: {}",
            String::from_utf8_lossy(&untar.stderr).trim()
        ));
    }
    let bytes = std::fs::read(&extracted).map_err(|e| format!("{}: {e}", extracted.display()))?;
    if hex_sha256(&bytes) != sha256 {
        return Err(format!(
            "npm-high-impact@{version} {}: sha256 {} does not match the spec's {sha256} \
             — the spec pins a list this tarball does not contain",
            file,
            hex_sha256(&bytes)
        ));
    }
    std::fs::write(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The snapshot loader: packuments from `<dir>`'s recorded files, fetched
/// (in waves, `jobs` at a time) when missing. `offline` refuses to fetch.
pub struct SnapshotLoader {
    dir: PathBuf,
    offline: bool,
    jobs: usize,
    memo: std::sync::Mutex<BTreeMap<String, Packument>>,
}

impl SnapshotLoader {
    pub fn new(dir: PathBuf, offline: bool, jobs: usize) -> SnapshotLoader {
        SnapshotLoader {
            dir,
            offline,
            jobs: jobs.clamp(1, 16),
            memo: std::sync::Mutex::new(BTreeMap::new()),
        }
    }

    fn memoise(&self, p: Packument) {
        self.memo
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(p.name.clone(), p);
    }
}

impl Loader for SnapshotLoader {
    fn prefetch(&mut self, names: &BTreeSet<String>) -> Result<(), String> {
        let todo: Vec<String> = names
            .iter()
            .filter(|n| !registry::packument_path(&self.dir, n).exists())
            .cloned()
            .collect();
        if todo.is_empty() {
            return Ok(());
        }
        if self.offline {
            return Err(format!(
                "{} packages are missing from the registry snapshot (--offline), \
                 e.g. {}; drop --offline or take the snapshot first",
                todo.len(),
                todo[0]
            ));
        }
        // curl fetches in parallel: the wave's fetch is IO-bound
        // subprocesses (concurrency: the builder's own fetch sweep,
        // bounded, one curl call each, no awaits, no shared state beyond
        // the result list)
        let next = std::sync::Mutex::new(0usize);
        let errors = std::sync::Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..self.jobs {
                scope.spawn(|| {
                    loop {
                        let i = {
                            let mut n = next.lock().expect("lock");
                            *n += 1;
                            *n - 1
                        };
                        let Some(name) = todo.get(i) else { break };
                        // a 404/tombstone is WRITTEN as an absent packument
                        // and Ok — absence is DATA, the resolution's decision
                        // point (enter) decides what it means; anything else
                        // fails loud (one retry already ran in load)
                        if let Err(e) = registry::load(&self.dir, name, false) {
                            errors.lock().expect("lock").push((name.clone(), e));
                        }
                    }
                });
            }
        });
        let errors = errors.into_inner().expect("lock");
        if let Some((_, e)) = errors.first() {
            return Err(e.clone());
        }
        Ok(())
    }

    fn peek(&self, name: &str) -> Option<Packument> {
        self.memo
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(name)
            .cloned()
    }

    fn get(&mut self, name: &str) -> Result<(Packument, String), String> {
        if let Some(p) = self
            .memo
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(name)
            .cloned()
        {
            // the memo holds the PARSED packument; the recorded sha is the
            // FILE's — read the file for it
            let path = registry::packument_path(&self.dir, name);
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            return Ok((p, hex_sha256(&bytes)));
        }
        let (p, sha) = registry::load(&self.dir, name, self.offline)?;
        self.memoise(p.clone());
        Ok((p, sha))
    }
}
