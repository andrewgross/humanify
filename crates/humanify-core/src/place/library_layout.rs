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
//! Conservatism is the policy, not a fallback. A region moves only when a
//! single package claims every one of its units; an unclaimed unit
//! interleaved with claimed ones stays (`partial` — moving would split the
//! module group the units belong to), and so do two packages in one group
//! (`contested`) and a whole-module key the index attributes to someone
//! else (`index-disagrees`). A file path two regions would share moves the
//! first region by sort key only (`duplicate-file`).
//!
//! Synthetic names are the index's misses: a claimed region with no clean
//! file answer takes `module-<N>.js`, where N is the region's 1-based rank
//! among its package's synthetic regions sorted by sort key — so a version
//! that reshuffles the bundle renumbers nothing.

use std::collections::BTreeMap;

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
}

/// The index's answer for a whole vendored module: owning packages and
/// source files, sorted by the caller (`LibraryIndex` rows arrive sorted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleAnswer {
    pub owners: Vec<String>,
    pub files: Vec<String>,
}

/// The planner's whole input; `module_answers` is keyed by module key.
pub struct LayoutInput<'a> {
    pub regions: Vec<RegionInput>,
    pub module_answers: &'a BTreeMap<String, ModuleAnswer>,
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
    let wants: Vec<Want> = input
        .regions
        .iter()
        .map(|region| region_want(region, input.module_answers))
        .collect();
    let paths = resolve_paths(input, &wants);
    LayoutPlan {
        regions: settle(input, &wants, &paths),
    }
}

/// One region's claim verdict: the single claiming package and the file it
/// wants, else the reason it cannot move.
fn region_want(region: &RegionInput, answers: &BTreeMap<String, ModuleAnswer>) -> Want {
    let package = match claim_verdict(region) {
        Ok(package) => package,
        Err(reason) => return Want::Leave { reason },
    };
    let answer = answer_for(region, answers);
    if answer_is_foreign(answer, package) {
        // A whole-module key the index gives to someone else: the claim
        // and the index cannot both be right, so nothing moves.
        return Want::Leave {
            reason: "index-disagrees",
        };
    }
    // An agreed answer with exactly one file is the real npm spelling;
    // no answer (or no single file) falls to a synthetic name below.
    let file = answer.and_then(single_file);
    Want::Move {
        package: package.to_string(),
        file,
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

/// Whether the index's answer, when there is one, names a DIFFERENT
/// owner than the claim. No answer is not a disagreement.
fn answer_is_foreign(answer: Option<&ModuleAnswer>, package: &str) -> bool {
    match answer {
        Some(answer) => !(answer.owners.len() == 1 && answer.owners[0] == package),
        None => false,
    }
}

fn single_file(answer: &ModuleAnswer) -> Option<String> {
    if answer.files.len() == 1 {
        Some(answer.files[0].clone())
    } else {
        None
    }
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
    assign_synthetic_paths(input, wants, &mut paths);
    paths
}

/// Number each package's synthetic regions 1.. by (sort key, index) and
/// fill their paths in — the ONLY place order enters the plan, and it is
/// the order of the keys' CONTENT, never the bundle's.
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
    for (package, mut idxs) in by_package {
        idxs.sort_by(|&a, &b| {
            input.regions[a]
                .sort_key
                .cmp(&input.regions[b].sort_key)
                .then(a.cmp(&b))
        });
        for (rank, idx) in idxs.into_iter().enumerate() {
            paths[idx] = compose_path(package, &format!("module-{}", rank + 1));
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

/// Turn resolved paths into decisions, running the duplicate race as a
/// second pass so a displaced incumbent loses its slot cleanly: the one
/// region a path belongs to is its best (sort key, index) claimant, every
/// other claimant of the same path leaves as a duplicate.
fn settle(input: &LayoutInput, wants: &[Want], paths: &[Option<String>]) -> Vec<Placement> {
    let mut best: BTreeMap<&str, (String, usize)> = BTreeMap::new();
    for (idx, path) in paths.iter().enumerate() {
        if let Some(path) = path {
            let key = (input.regions[idx].sort_key.clone(), idx);
            let better = match best.get(path.as_str()) {
                None => true,
                Some(incumbent) => &key < incumbent,
            };
            if better {
                best.insert(path, key);
            }
        }
    }
    paths
        .iter()
        .enumerate()
        .map(|(idx, path)| match path {
            None => match &wants[idx] {
                Want::Leave { reason } => Placement::Leave { reason },
                Want::Move { .. } => unreachable!("a Move want always resolves to a path"),
            },
            // The path's best claimant is unique by (sort key, index), so
            // the index alone says whether this region is the winner.
            Some(path) => match best.get(path.as_str()) {
                Some((_, at)) if *at == idx => Placement::Move { path: path.clone() },
                _ => Placement::Leave {
                    reason: "duplicate-file",
                },
            },
        })
        .collect()
}
