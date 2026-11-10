//! The names-mode placement planner (library-names plan step 4, exp156):
//! a PURE turn of the crank from library claims to vendor-placement
//! decisions — which module regions MOVE to `vendor/<package>/<file>` and
//! which STAY where the split would otherwise put them, each with a reason.
//!
//! It owns ONE question (see docs/responsibility.md): *given* the discovery
//! claims (per unit, in bundle order) and the index's whole-module answers,
//! where may library code land? It never reads the bundle, the index or the
//! clock, and it never sees a hash-map iteration order — the caller passes
//! plain vectors and a sorted map, and the ONLY ordering it consults is the
//! region's content-derived sort key. That is what makes a layout
//! cross-version stable: two bundles that contain the same claimed groups
//! get the same plan whichever order the groups appear in, so an unchanged
//! library lands at an identical vendor path in every version.
//!
//! Conservatism is the policy, not a fallback. A region moves when a
//! single package claims every one of its units, or — exp157 — when it
//! carries no claim at all but its whole-module key has a CLEAN index
//! answer (one owner, one file: the bundler scatters a package between
//! versions and strips a module's claims, and the content key is the
//! evidence that survives the reshuffle); an unclaimed unit interleaved
//! with claimed ones stays (`partial` — moving would split the module
//! group the units belong to), and so do two packages in one group
//! (`contested`) and a whole-module key the index attributes to someone
//! else (`index-disagrees`). A file path two regions would share moves the
//! first region by sort key only; the loser re-paths to its own synthetic
//! in-package name (D1, plan step 7) — a package's regions coalesce under
//! its folder, never one file beside a stayed region.
//!
//! Synthetic names are the index's misses: a claimed region with no clean
//! file answer takes `module-<hash8>.js`, where hash8 is the first 8 hex
//! of the region's own content-derived sort key — for the naming era, the
//! module's whole statement set hashed together (exp158; the head
//! statement's hash alone collided across different modules and renumbered
//! a package's synthetic regions whenever a version's composition
//! changed) — so neither the bundle's order nor the package's other
//! regions can rename it. A tie in the sort key itself falls back to
//! arrival order as `-2`, `-3`.

use std::collections::BTreeMap;

use crate::libindex::families::PackageFamilies;
use crate::modules::vendor_names::{sanitize_fs_path, strip_js_extension};

/// One region (module group) as the planner sees it: the claiming package
/// per unit, in bundle order (`None` = unclaimed), the vendored module key
/// the region is if it was extracted whole, and the content-derived sort
/// key that (only) orders synthetic names and duplicate races.
#[derive(Clone, Debug)]
pub struct RegionInput {
    pub claims: Vec<Option<String>>,
    pub module: Option<String>,
    pub sort_key: String,
    /// The package the PRIOR release filed this exact content under
    /// (exp211), when it filed every copy under one package; `None`
    /// without a prior or when the prior did not place it.
    pub carried: Option<String>,
}

/// The fold marker for one slot whose own contents disagree — a wrapper
/// the GLUE rule claimed for one package over a body another claims. It
/// is written only by [`fold_claims`] (the group exporter folds a head's
/// wrapper-and-body claims into one row); no npm name can look like it,
/// and the planner refuses any region carrying it, whatever else agrees.
pub const INNER_CONTESTED: &str = "<contested>";

/// Fold the claims over one slot's own contents into that slot's single
/// claim: all-agree passes the package through, a disagreement folds to
/// [`INNER_CONTESTED`], silence stays `None`.
pub fn fold_claims<'a, I>(claims: I) -> Option<String>
where
    I: IntoIterator<Item = Option<&'a str>>,
{
    let mut package: Option<&str> = None;
    for claim in claims.into_iter().flatten() {
        match package {
            None => package = Some(claim),
            Some(seen) if seen == claim => {}
            Some(_) => return Some(INNER_CONTESTED.to_string()),
        }
    }
    package.map(str::to_string)
}

/// The index's answer for a whole vendored module: every owning package
/// with ITS source files (sorted by the caller — `LibraryIndex` rows
/// arrive sorted). A key two packages hold keeps both owners' files
/// (exp211): which owner the region is, is the planner's question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleAnswer {
    pub owners: BTreeMap<String, Vec<String>>,
}

/// The planner's whole input; `module_answers` is keyed by module key.
/// `families` says which packages are one family (the carry chooses only
/// within the claim's family, exp211).
pub struct LayoutInput<'a> {
    pub regions: Vec<RegionInput>,
    pub module_answers: &'a BTreeMap<String, ModuleAnswer>,
    pub families: &'a PackageFamilies,
}

/// One region's decision: the exact vendor path to move to, or the
/// reason it stays put.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Placement {
    Move { path: String },
    Leave { reason: &'static str },
}

impl Placement {
    pub fn move_path(&self) -> Option<&str> {
        match self {
            Placement::Move { path } => Some(path),
            Placement::Leave { .. } => None,
        }
    }

    pub fn leave_reason(&self) -> Option<&'static str> {
        match self {
            Placement::Leave { reason } => Some(reason),
            Placement::Move { .. } => None,
        }
    }
}

/// The planner's output: one decision per input region, in input order —
/// the plan is positional, so the caller pairs `regions[i]` with input
/// region i by index.
pub struct LayoutPlan {
    pub regions: Vec<Placement>,
}

/// What a region wants before synthetic ordinals and duplicate races are
/// settled: a package plus either the index's file or a synthetic slot.
enum Want {
    Move {
        package: String,
        file: Option<String>,
    },
    Leave {
        reason: &'static str,
    },
}

/// Plan every region. Pure: the same input yields the identical output.
pub fn plan(input: &LayoutInput) -> LayoutPlan {
    let evidence = bundle_evidence(input);
    let choice = Choice {
        answers: input.module_answers,
        families: input.families,
        evidence: &evidence,
    };
    let wants: Vec<Want> = input
        .regions
        .iter()
        .map(|region| region_want(region, &choice))
        .collect();
    let paths = resolve_paths(input, &wants);
    LayoutPlan {
        regions: settle(&wants, &paths),
    }
}

/// One region's claim verdict: the single claiming package and the file it
/// wants, else the reason it cannot move.
fn region_want(region: &RegionInput, choice: &Choice) -> Want {
    let answer = answer_for(region, choice.answers);
    match claim_verdict(region) {
        Ok(claim) => {
            let package = shared_key_choice(claim, region.carried.as_deref(), answer, choice);
            match answer {
                // A whole-module key the index gives only to OTHER
                // packages: the claim and the index cannot both be right,
                // so nothing moves. A key the claim's package holds beside
                // others is no disagreement (exp211): the region takes
                // that package's own files.
                Some(answer) if !answer.owners.contains_key(package) => Want::Leave {
                    reason: "index-disagrees",
                },
                // An agreed answer with exactly one file is the real npm
                // spelling; no answer (or no single file) falls to a
                // synthetic name below.
                _ => Want::Move {
                    package: package.to_string(),
                    file: answer
                        .and_then(|a| a.owners.get(package))
                        .and_then(|files| chosen_file(files)),
                },
            }
        }
        // No claim on any unit — the index may still answer cleanly (exp157):
        // a bundler reshuffle can strip a package's claims from a module it
        // carried in the previous version, and the whole-module key is the
        // evidence that survives it (one owner, one file: this exact body is
        // that package's that file). Claims that disagree stay leaves — a
        // conflict is evidence, silence is not.
        Err(reason) => match reason {
            "unclaimed" => match answer {
                Some(answer) if answer.owners.len() == 1 => {
                    let (owner, files) = answer.owners.iter().next().expect("one owner");
                    Want::Move {
                        package: owner.clone(),
                        file: chosen_file(files),
                    }
                }
                _ => Want::Leave { reason },
            },
            _ => Want::Leave { reason },
        },
    }
}

/// What the shared-key choice reads besides the region (exp211).
struct Choice<'a> {
    answers: &'a BTreeMap<String, ModuleAnswer>,
    families: &'a PackageFamilies,
    /// Per package, how many regions move on a whole-module key ONLY that
    /// package holds — the bundle's own evidence of which packages it
    /// carries ([`bundle_evidence`]).
    evidence: &'a BTreeMap<String, usize>,
}

/// The bundle's dependency evidence (exp211): per package, the regions
/// whose whole-module key the index gives to that package ALONE and that
/// move on it (the claim agrees, or no claim at all — exp157's clean
/// answer). Order-free: a count over the input, whatever the bundle order.
fn bundle_evidence(input: &LayoutInput) -> BTreeMap<String, usize> {
    let mut evidence: BTreeMap<String, usize> = BTreeMap::new();
    for region in &input.regions {
        let Some(answer) = answer_for(region, input.module_answers) else {
            continue;
        };
        let mut owners = answer.owners.keys();
        let (Some(owner), None) = (owners.next(), owners.next()) else {
            continue;
        };
        let agrees = match claim_verdict(region) {
            Ok(claim) => claim == owner,
            Err(reason) => reason == "unclaimed",
        };
        if agrees {
            *evidence.entry(owner.clone()).or_default() += 1;
        }
    }
    evidence
}

/// The package a claimed region is filed under when the same code is held
/// by more than one package (exp211), else the claim:
///
/// 1. the PRIOR release's package for this exact content, when it is one
///    family with the claim (lodash and lodash-es ship the same bodies,
///    and which twin discovery claims depends on which neighbour reached
///    the module first) and — when the index answers the whole-module key
///    — the index records it as an owner too;
/// 2. when the index gives the key to the claim AND other packages, the
///    owner the bundle evidently carries: strictly the most regions placed
///    on keys only it holds ([`bundle_evidence`]; ajv over its fork
///    `@redocly/ajv`, aws-sdk's `nested-clients` over the client it
///    re-ships). A tie keeps the claim.
///
/// Every choice is a package that holds the code by the evidence already
/// in hand; the choice never makes a region move.
fn shared_key_choice<'r>(
    claim: &'r str,
    carried: Option<&'r str>,
    answer: Option<&'r ModuleAnswer>,
    choice: &Choice,
) -> &'r str {
    if let Some(prior) = carried
        && prior != claim
        && choice.families.same(prior, claim)
        && answer.is_none_or(|a| a.owners.contains_key(prior))
    {
        return prior;
    }
    match answer {
        Some(answer) if answer.owners.len() > 1 && answer.owners.contains_key(claim) => {
            evident_owner(answer, choice.evidence).unwrap_or(claim)
        }
        _ => claim,
    }
}

/// The key's one owner with strictly the most bundle evidence, if any
/// owner has evidence at all; `None` on a tie.
fn evident_owner<'a>(
    answer: &'a ModuleAnswer,
    evidence: &BTreeMap<String, usize>,
) -> Option<&'a str> {
    let count = |owner: &str| evidence.get(owner).copied().unwrap_or(0);
    let best = answer.owners.keys().map(|o| count(o)).max()?;
    let mut leaders = answer.owners.keys().filter(|o| count(o) == best);
    match (leaders.next(), leaders.next()) {
        (Some(owner), None) if best > 0 => Some(owner.as_str()),
        _ => None,
    }
}

/// The one package claiming every unit of the region, else the reason it
/// stays: two packages in one group outrank the gap between them
/// (`contested` beats `partial` — a split between claimants is worse
/// news than a hole), and a group with no claim at all is `unclaimed`.
fn claim_verdict(region: &RegionInput) -> Result<&str, &'static str> {
    let mut package: Option<&str> = None;
    for claim in &region.claims {
        let Some(claim) = claim.as_deref() else {
            continue;
        };
        // A folded slot whose own contents disagree: no move of any kind.
        if claim == INNER_CONTESTED {
            return Err("contested");
        }
        match package {
            None => package = Some(claim),
            Some(seen) if seen == claim => {}
            Some(_) => return Err("contested"),
        }
    }
    match package {
        Some(_) if region.claims.iter().any(|c| c.is_none()) => Err("partial"),
        Some(package) => Ok(package),
        _ => Err("unclaimed"),
    }
}

/// The region's file from an index answer: ONE file is it; several files
/// take the index's MAIN file — the one whose basename is `index`, in the
/// caller's sorted order — when the answer has one (D1, plan step 7: a
/// package-level answer still lands the region inside the package, under a
/// sensible name). None, and the region falls to the content-hash
/// synthetic form below.
fn chosen_file(files: &[String]) -> Option<String> {
    if files.len() == 1 {
        return Some(files[0].clone());
    }
    files
        .iter()
        .find(|file| strip_js_extension(basename(file)) == "index")
        .cloned()
}

/// Everything after the last `/` (a file with no separator is its own
/// basename).
fn basename(file: &str) -> &str {
    file.rsplit_once('/').map_or(file, |(_, base)| base)
}

/// The index answer for the region's whole-module key, if it has one.
fn answer_for<'a>(
    region: &RegionInput,
    answers: &'a BTreeMap<String, ModuleAnswer>,
) -> Option<&'a ModuleAnswer> {
    region.module.as_deref().and_then(|key| answers.get(key))
}

/// The vendor path every region moves to (`None` = it does not move):
/// index files compose directly, synthetic regions take a per-package
/// ordinal ranked by sort key.
fn resolve_paths(input: &LayoutInput, wants: &[Want]) -> Vec<Option<String>> {
    let mut paths: Vec<Option<String>> = Vec::with_capacity(wants.len());
    for want in wants {
        paths.push(match want {
            Want::Leave { .. } => None,
            Want::Move {
                package,
                file: Some(file),
            } => compose_path(package, file),
            Want::Move { .. } => None, // synthetic, numbered below
        });
    }
    // D1: a composed path two regions claim moves its best claimant only
    // (sort key, then arrival) — the loser re-paths to its OWN synthetic
    // in-package name below. A package's regions all coalesce under its
    // folder; a loser never stays put beside the winner.
    let mut best: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for (idx, path) in paths.iter().enumerate() {
        if let Some(path) = path {
            let key = (input.regions[idx].sort_key.clone(), idx);
            if best.get(path).is_none_or(|incumbent| &key < incumbent) {
                best.insert(path.clone(), key);
            }
        }
    }
    for (idx, path) in paths.iter_mut().enumerate() {
        if let Some(p) = path.as_deref()
            && best[p].1 != idx
        {
            *path = None;
        }
    }
    assign_synthetic_paths(input, wants, &mut paths);
    paths
}

/// Name each package's synthetic regions after their OWN content —
/// `module-<first 8 of the sort key>.js`, the caller's content key — so
/// no other region's presence and no bundle order can rename one (exp157:
/// the ordinal scheme renumbered a package's synthetic regions whenever a
/// version's composition added or dropped one, and unchanged content
/// landed at a new path; exp158: the era's key is the module's whole
/// statement set hashed together, since a blurred head statement alone
/// collides across different modules). Only a tie in the sort key itself
/// falls back to arrival order, spelled `-2`, `-3`, … after the first.
fn assign_synthetic_paths(input: &LayoutInput, wants: &[Want], paths: &mut [Option<String>]) {
    let mut by_package: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (idx, want) in wants.iter().enumerate() {
        // Any mover without a composed path takes a synthetic name — an
        // index file that fails to compose falls back here too.
        if let Want::Move { package, .. } = want
            && paths[idx].is_none()
        {
            by_package.entry(package.as_str()).or_default().push(idx);
        }
    }
    for (package, idxs) in by_package {
        let mut idxs = idxs;
        idxs.sort_by(|&a, &b| {
            input.regions[a]
                .sort_key
                .cmp(&input.regions[b].sort_key)
                .then(a.cmp(&b))
        });
        let mut previous: Option<&str> = None;
        let mut ties = 0;
        for idx in idxs {
            let key = &input.regions[idx].sort_key;
            let stem = if previous == Some(key.as_str()) {
                ties += 1;
                format!("module-{}-{}", &key[..key.len().min(8)], ties + 1)
            } else {
                ties = 0;
                format!("module-{}", &key[..key.len().min(8)])
            };
            previous = Some(key.as_str());
            paths[idx] = compose_path(package, &stem);
        }
    }
}

/// `vendor/<package>/<file>.js`, both sides sanitized as PATHS (scoped
/// packages keep their directories); `None` when either side sanitizes to
/// nothing and the path would not be a vendor file at all.
fn compose_path(package: &str, file: &str) -> Option<String> {
    let package = sanitize_fs_path(package);
    let file = sanitize_fs_path(&strip_js_extension(file));
    if package.is_empty() || file.is_empty() {
        return None;
    }
    Some(format!("vendor/{package}/{file}.js"))
}

/// Turn resolved paths into decisions. Every path is unique here — the
/// duplicate race ran in [`resolve_paths`] and its losers re-pathed, so a
/// region moves exactly when a path resolved for it.
fn settle(wants: &[Want], paths: &[Option<String>]) -> Vec<Placement> {
    wants
        .iter()
        .zip(paths)
        .map(|(want, path)| match path {
            Some(path) => Placement::Move { path: path.clone() },
            None => match want {
                Want::Leave { reason } => Placement::Leave { reason },
                Want::Move { .. } => unreachable!("a Move want always resolves to a path"),
            },
        })
        .collect()
}
