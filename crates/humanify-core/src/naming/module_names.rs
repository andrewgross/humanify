//! The module step: one name per NEW recorded module, from its contents,
//! for both its lazy-init wrapper (`initColorUtils`) and its file
//! (`color-utils.js`) — docs/design/module-naming.md (Andrew, 2026-10-05:
//! "name files using an LLM based on some contents or some of the
//! functional code in there as opposed to the first reference";
//! approved 2026-10-06).
//!
//! Bun and esbuild record every original source file as one lazy-init
//! wrapper (`var initFoo = __esm(() => { … })`), and every split file holds
//! exactly one such module. Generic for any bundle whose toolchain knows
//! those boundaries: the step runs only when the module markers describe
//! the bundle ([`crate::place::method::markers_describe_bundle`] — the
//! rule the split method reads), on the split's own extraction
//! ([`crate::twins::fossil::extract_fossil_modules`]).
//!
//! Two halves around the naming waves:
//!
//! 1. [`plan_module_naming`], BEFORE the first wave: every module whose
//!    wrapper is still unnamed (the prior-version transfer did not carry
//!    it — on a fresh run, every module) is marked "named by the module
//!    step", so no wave asks its wrapper. That includes BARRELS (modules
//!    that declare nothing; their setup code only loads other modules —
//!    finding #94: left to the waves they drew filler like
//!    `setupModulesOnceVar` that their files then kept).
//! 2. [`run_module_naming`], after the waves and the library-prefix pass,
//!    before the naming floor and sweep. The modules that declare
//!    something first: one batched pass through the file namer's module
//!    kind ([`crate::place::assign::namer::NameKind::Module`], 8 per
//!    call), showing each module's evidence ([`evidence`]) under the
//!    names the waves chose. Each answer is checked
//!    ([`accept_proposed_name`], no filler-only name —
//!    [`is_filler_module_name`] — then the waves' answer-quality checks:
//!    no borrowed minified stem, no echo) and applied as `init<Name>`
//!    through the validated renamer, recorded on the trail under
//!    [`Tier::ModuleNaming`]. Then the barrels, loaded modules before the
//!    barrels that load them: named by RULE from their loaded modules'
//!    names ([`barrels`]); a barrel whose loaded modules share no phrase
//!    is asked of the model with those names as evidence.
//!
//! Every module the model is asked about follows one ladder, so none is
//! left minified (finding #94):
//!
//! - a duplicate (the first module in bundle order keeps the name) is
//!   asked ONCE more in a small batch that lists the taken name;
//! - anything still unnamed gets one DISCLOSED retry: why its answers
//!   were refused, and the taken names in the #89 owner's order
//!   ([`build_retry_used_names`]: its own refused answers, then the taken
//!   names sharing their stem, then its neighbours' names);
//! - still unnamed: a deterministic name, never minified and never
//!   filler — `init<FirstDeclared>` (the module's first function, the
//!   mechanical stem's word), else `init<FirstDeclared>Module`, else the
//!   suffix ladder ([`crate::naming::reask::ladder`]); a barrel takes
//!   `init<FirstLoaded>Index` the same way. Only a module with no usable
//!   word at all is left to the floor/sweep.
//!
//! The split reads the name back from the wrapper
//! ([`crate::place::stems::module_stem_of_wrapper`]): the wrapper's name
//! is the single record of the module's name, so the file can never
//! disagree with it. A matched module keeps its prior path verbatim (the
//! split's own rule), and a carried wrapper is never re-asked.

pub mod barrels;
pub mod evidence;

use std::collections::{HashMap, HashSet};

use humanify_model::llm::NameProvider;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::artifact_dump::DispatchLog;
use crate::graph::UnifiedGraph;
use crate::naming::waves::generate::TextView;
use crate::naming::waves::processor::{answer_refusal, build_retry_used_names};
use crate::naming::waves::render::Occurrences;
use crate::place::assign::namer::{
    NameKind, ProviderSplitNamer, SplitNameRequest, SplitNamer, SplitNamerBudget,
};
use crate::place::stems::{
    accept_proposed_name, is_filler_module_name, module_stem_of_wrapper, module_wrapper_name,
    stem_of,
};
use crate::rename::floor::MinifiedStems;
use crate::rename::transfer::lifecycle::Lifecycle;
use crate::rename::validated::scopes::BindingId;
use crate::rename::validated::{RejectionReason, RenameRequest, RenameState, TrailSpec};
use crate::trail::{Attempt, Outcome, Tier};
use crate::twins::fossil::{FossilExtract, FossilModule, extract_fossil_modules, marker_coverage};
use evidence::EvidenceSource;

#[cfg(test)]
mod module_names_test;

/// One names-mode placement group (library-names plan step 4, exp156):
/// the discovery report's `placementGroups` row for one lazy module. The
/// unpack reports it by RAW statement spans (`statements` empty); the era
/// converts it onto its own wrapper-body indices through the exact bridge
/// ([`convert_regions`], exp166), one entry per era statement — a raw
/// statement the formatter split repeats its claim and span. Claims are
/// the row's folded packages (pendings first, head last).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimedRegion {
    pub statements: Vec<usize>,
    pub claims: Vec<Option<String>>,
    /// The statements' RAW bundle spans, `statements` order — the diag's
    /// claims and restoration applications are in raw coordinates; the
    /// conversion (unpack) is the only place that knows both (exp164).
    pub raw_spans: Vec<(u32, u32)>,
}

/// The exact bridge from RAW bundle coordinates to the naming era's
/// statements (exp166). The unpack knows each retained container
/// statement's raw span and where the edit list puts it in the runtime
/// text; the formatter knows which runtime statement each of ITS printed
/// statements came from ([`crate::format::Formatted::statement_origins`]).
/// Together they say, for every era statement, which raw statement it
/// holds — a split statement included — where the ordinal arithmetic
/// (`i - removed_before`) assumed one era statement per raw one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EraBridge {
    /// Every retained container statement: (raw span, runtime span), in
    /// bundle order (runtime order too — the edits never reorder).
    pub container: Vec<((u32, u32), (u32, u32))>,
    /// Every vendored factory: (raw declaration span, written path).
    pub factories: Vec<((u32, u32), String)>,
    /// The unpacked file the container is written to.
    pub runtime_file: String,
    /// Site B's restorations (exp164/exp166): `(raw offset, restored
    /// name)` per edit, raw order — the identifiers the index actually
    /// answered, which names mode alone keeps the model off.
    pub restored: Vec<(u32, String)>,
}

impl EraBridge {
    /// The names Site B restored inside raw span `(start, end)`.
    pub fn restored_in(&self, (start, end): (u32, u32)) -> impl Iterator<Item = &str> {
        let at = self.restored.partition_point(|(o, _)| *o < start);
        self.restored[at..]
            .iter()
            .take_while(move |(o, _)| *o < end)
            .map(|(_, name)| name.as_str())
    }

    /// The raw span of the container statement whose runtime span holds
    /// runtime offset `at`.
    pub fn raw_of_runtime(&self, at: u32) -> Option<(u32, u32)> {
        let k = self
            .container
            .partition_point(|&(_, (start, _))| start <= at)
            .checked_sub(1)?;
        let (raw, (_, end)) = self.container[k];
        (at < end).then_some(raw)
    }

    /// Per era body statement (its span in the formatted text), the raw
    /// span of the container statement it came from: the first
    /// provenance mark at or after the previous statement's end and
    /// before this one's end is this statement's (a nested statement's
    /// mark always starts later than its enclosing one). A statement with
    /// no mark of its own (wholly synthesized) inherits its predecessor's.
    pub fn era_raw_spans(
        &self,
        body_spans: &[(u32, u32)],
        origins: &[(u32, u32)],
    ) -> Vec<Option<(u32, u32)>> {
        let mut out = Vec::with_capacity(body_spans.len());
        let mut prev_end = 0;
        let mut prev: Option<(u32, u32)> = None;
        for &(start, end) in body_spans {
            let k = origins.partition_point(|&(o, _)| o < prev_end.min(start));
            let raw = origins[k..]
                .iter()
                .take_while(|&&(o, _)| o < end)
                .next()
                .and_then(|&(_, origin)| self.raw_of_runtime(origin))
                .or(prev);
            out.push(raw);
            prev = raw;
            prev_end = end;
        }
        out
    }
}

/// What [`convert_regions`] did (exp166): the claimed groups the unpack
/// reported, how many reached this era's statements whole, and how many
/// era statements they now cover (a split raw statement covers several).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionConversion {
    pub groups: usize,
    pub converted: usize,
    pub dropped: usize,
    pub era_statements: usize,
}

/// The claimed groups (raw spans + claims, from the unpack) onto THIS
/// era's statements, through the exact bridge (exp166): an era statement
/// belongs to a group's raw statement when the bridge says it came from
/// it, so a raw statement the formatter split into several era statements
/// takes all of them (each with the raw statement's claim and span), and
/// nothing is ever joined to a statement it did not come from. The ordinal
/// arithmetic this replaces (`i - removed_before`) assumed one era
/// statement per raw one and was content-correct for 118 of 7,628
/// statements on cc (exp164) — it moved Claude Code's plugin installer to
/// `vendor/jose/` on jose's claims. A group with a raw statement no era
/// statement came from is dropped whole: a partial join would move part
/// of a module.
pub fn convert_regions(
    groups: &[ClaimedRegion],
    era_raw: &[Option<(u32, u32)>],
) -> (Vec<ClaimedRegion>, RegionConversion) {
    let mut era_of: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (i, raw) in era_raw.iter().enumerate() {
        if let Some(raw) = raw {
            era_of.entry(*raw).or_default().push(i);
        }
    }
    let mut stats = RegionConversion {
        groups: groups.len(),
        ..Default::default()
    };
    let mut out = Vec::with_capacity(groups.len());
    'group: for group in groups {
        let mut region = ClaimedRegion {
            statements: Vec::new(),
            claims: Vec::new(),
            raw_spans: Vec::new(),
        };
        for (raw, claim) in group.raw_spans.iter().zip(&group.claims) {
            let Some(era) = era_of.get(raw) else {
                stats.dropped += 1;
                continue 'group;
            };
            for &i in era {
                region.statements.push(i);
                region.claims.push(claim.clone());
                region.raw_spans.push(*raw);
            }
        }
        stats.converted += 1;
        stats.era_statements += region.statements.len();
        out.push(region);
    }
    (out, stats)
}

/// What the names-mode placement decided for the era's modules
/// (library-names plan step 4, exp156): the wholly claimed lazy modules
/// that MOVE to `vendor/<package>/<file>` (with the module's index in the
/// era's own extraction), the moved statements' spans, and the freeze —
/// since exp166 only the restored bindings, applied BEFORE the transfer
/// and the waves in both eras.
#[derive(Default, Debug)]
pub struct LibraryLayout {
    /// `(module index, vendor path)` per moved module, module order.
    pub moved: Vec<(usize, String)>,
    /// The FREEZE (exp166): the declaration spans of the bindings the
    /// index restored — every hold over the run (the transfer, the waves,
    /// the reconcile, the sweep) reads these. Not the moved statements
    /// any more: see `moved_spans`.
    pub spans: Vec<(u32, u32)>,
    /// The same spans in the GENERATED text's coordinates (the render's
    /// substitutions folded in; written by the naming era after it
    /// renders). The passes that re-parse the generated text — the
    /// prior-diff reconcile — resolve their own bindings and cannot read
    /// the era's coordinates, so they read these instead.
    pub generated_spans: Vec<(u32, u32)>,
    /// The claimed statements the restoration renamed (exp160), in era
    /// coordinates and then (after the era renders) in generated ones —
    /// the protected-span set beside the placed modules': the prior-diff
    /// reconcile holds these too, so the prior's names never overwrite
    /// the index's.
    pub restored_spans: Vec<(u32, u32)>,
    pub restored_generated_spans: Vec<(u32, u32)>,
    /// The moved modules' statement spans (era coordinates) — where the
    /// split puts library files. Names mode's freeze (`spans`) is NOT this
    /// any more (exp166, "restore or ask, never freeze"): it holds only
    /// the bindings the index restored; every other identifier in a moved
    /// module goes through the normal naming flow.
    pub moved_spans: Vec<(u32, u32)>,
    /// The claimed regions as this era sees them (exp166: converted
    /// through the exact bridge) and what the conversion did.
    pub regions: Vec<ClaimedRegion>,
    pub conversion: Option<RegionConversion>,
    /// Each era statement's raw bundle span (the bridge; empty without).
    pub era_raw: Vec<Option<(u32, u32)>>,
}

/// The names-mode placement plan over the claimed regions (the planner
/// is [`crate::place::library_layout`], pure): join each region to the
/// era's own extraction — a region joins the module whose statement set
/// is exactly its own ([`join_regions`]; a module two packages claim is
/// never joined) — then let the planner decide move or stay. Same gate as
/// the module step: the markers must describe the bundle, else nothing
/// moves and nothing is excluded (the split would not split by modules
/// either).
///
/// exp158 tried the drift-tolerant relaxation (a join at a two-sided
/// overlap bar, X derived from the exp157 join census at 0.55) and
/// REFUTED it by measurement: replaying the join offline over the
/// 2.1.85→2.1.86 pair (the join inputs dumped from the binary, the replay
/// reproducing both the strict join counts and the pair's bucket counts
/// exactly), every bar from 0.5 to 0.95 and all three forms (two-sided,
/// region-side only, module-side only) INCREASED the era's one-sided
/// placement — strict 34/13 moved paths held on one version only, 0.55
/// two-sided 99/59 — because the near-miss relationships themselves do
/// not replicate across versions (the claims flip packages wholesale,
/// matching's territory). The strict join stays; what changed is the
/// NAME ([`module_content_key`]) and the guard.
pub fn plan_library_modules(source: &ModuleSource<'_>, regions: &[ClaimedRegion]) -> LibraryLayout {
    use crate::place::library_layout::{LayoutInput, RegionInput, plan};
    let mut layout = LibraryLayout::default();
    if regions.is_empty() {
        return layout;
    }
    let body = source.body;
    let spans: Vec<(u32, u32)> = body.iter().map(|s| span_of(s).unwrap_or((0, 0))).collect();
    let factory = source.grammar.identify_factory_helper(source.text);
    let coverage = marker_coverage(body, &spans, factory.as_ref().map(|h| h.name.as_str()));
    if !crate::place::method::markers_describe_bundle(&coverage) {
        return layout;
    }
    let hashes = vec![String::new(); body.len()];
    let Ok(extract) = extract_fossil_modules(body, &hashes) else {
        return layout;
    };
    let join_of = join_regions(regions, &extract);
    let mut joined: Vec<(usize, RegionInput)> = Vec::new();
    for (region, module) in regions.iter().zip(&join_of) {
        let Some(module) = module else { continue };
        joined.push((
            *module,
            RegionInput {
                claims: region.claims.clone(),
                // Lazy regions have no whole-module key in the index (it
                // holds bundled CommonJS modules), so an agreed region
                // takes the planner's synthetic spelling — stable by the
                // module's own statement-set content key.
                module: None,
                sort_key: module_content_key(&extract.modules[*module], body),
            },
        ));
    }
    if joined.is_empty() {
        return layout;
    }
    let answers = std::collections::BTreeMap::new();
    let planned = plan(&LayoutInput {
        regions: joined.iter().map(|(_, region)| region.clone()).collect(),
        module_answers: &answers,
    });
    for ((module, _), placement) in joined.iter().zip(&planned.regions) {
        if let crate::place::library_layout::Placement::Move { path } = placement {
            layout.moved.push((*module, path.clone()));
            for &statement in &extract.modules[*module].statements {
                if let Some(span) = spans.get(statement) {
                    layout.spans.push(*span);
                    layout.moved_spans.push(*span);
                }
            }
        }
    }
    layout
}

/// Join every region to the era module it sits in, per region (`None` =
/// no join). The extraction partitions the body, so every statement sits
/// in at most one module; every claim names a package. One pass: a
/// region joins the module whose statement set is exactly its own — the
/// drift-tolerant relaxation was measured and refuted (see
/// [`plan_library_modules`]) — and never one two packages claim.
fn join_regions(regions: &[ClaimedRegion], extract: &FossilExtract) -> Vec<Option<usize>> {
    let mut claims_of: HashMap<usize, Vec<&str>> = HashMap::new();
    for region in regions {
        for (&statement, claim) in region.statements.iter().zip(&region.claims) {
            if let Some(package) = claim.as_deref() {
                claims_of.entry(statement).or_default().push(package);
            }
        }
    }
    let mut module_of: HashMap<&[usize], usize> = HashMap::new();
    for (i, module) in extract.modules.iter().enumerate() {
        module_of.insert(module.statements.as_slice(), i);
    }
    // The exact pass — with exp158's package guard: two packages in one
    // module never move.
    let mut sorted: Vec<usize>;
    let mut join_of: Vec<Option<usize>> = Vec::with_capacity(regions.len());
    for region in regions {
        sorted = region.statements.clone();
        sorted.sort_unstable();
        let join = module_of
            .get(sorted.as_slice())
            .copied()
            .and_then(|module| {
                let package = region_package(&region.claims);
                (!packages_conflict(&extract.modules[module], &claims_of, package.as_deref()))
                    .then_some(module)
            });
        join_of.push(join);
    }
    join_of
}

/// The region's own claim fold — the one package its claims agree on, or
/// `None` (unclaimed, or a fold whose own contents disagree, the
/// [`INNER_CONTESTED`] sentinel).
fn region_package(claims: &[Option<String>]) -> Option<String> {
    use crate::place::library_layout::{INNER_CONTESTED, fold_claims};
    match fold_claims(claims.iter().map(|c| c.as_deref())) {
        Some(package) if package != INNER_CONTESTED => Some(package),
        _ => None,
    }
}

/// exp158's package guard: the claims inside the module's statements and
/// the joining region's own claim fold must name at most ONE package —
/// two in one vendor file would merge two packages' code, whoever
/// agrees. `None` (an unclaimed or contested region) contributes nothing
/// of its own; the module's own claims still decide.
fn packages_conflict(
    module: &FossilModule,
    claims_of: &HashMap<usize, Vec<&str>>,
    package: Option<&str>,
) -> bool {
    let mut names: HashSet<&str> = module
        .statements
        .iter()
        .filter_map(|&s| claims_of.get(&s))
        .flatten()
        .copied()
        .collect();
    names.extend(package);
    names.len() > 1
}

/// The region's content key (exp158): the module's whole statement set,
/// each statement rename-invariantly hashed, the hashes SORTED and hashed
/// together — so the name follows the module's own content, in either
/// statement order, and neither a sibling region's presence nor a
/// composition change can rename it. The head statement's hash alone is
/// what the scheme replaced: short generic wrapper statements collide
/// across different modules (three lodash regions whose wrappers are the
/// same blurred shape drew one name in arrival order), so a version that
/// discovered one more region renamed the survivors.
fn module_content_key(module: &FossilModule, body: &[Value]) -> String {
    let mut hashes: Vec<String> = module
        .statements
        .iter()
        .map(|&s| crate::hash::statement_hash::statement_hash(&body[s]))
        .collect();
    hashes.sort_unstable();
    let mut hasher = Sha256::default();
    for hash in &hashes {
        hasher.update(hash.as_bytes());
        hasher.update(b"|");
    }
    format!("{:x}", hasher.finalize())
}

/// The lifecycle reason a marked wrapper is skipped by the waves under.
pub const MODULE_STEP_SKIP: &str = "module-naming";

/// Trail reasons of an applied module name that the model's first answer
/// (or its duplicate retry) did not give.
pub const BY_BARREL_RULE: &str = "barrel-rule";
pub const BY_DISCLOSED_RETRY: &str = "disclosed-retry";
pub const BY_FALLBACK: &str = "fallback";
/// The refusal code of an answer made only of filler words.
pub const FILLER_NAME: &str = "filler-name";
/// The refusal code of a module the step had no answer for.
const NO_ANSWER: &str = "no-answer";

/// The step's plan: the recorded modules and which ones it names.
#[derive(Default)]
pub struct ModulePlan {
    modules: Vec<FossilModule>,
    /// Per module: the wrapper's binding (None: not resolved).
    wrappers: Vec<Option<BindingId>>,
    /// Module indexes the step names that declare something, bundle order.
    asked: Vec<usize>,
    /// Barrel module indexes the step names, bundle order.
    barrels: Vec<usize>,
    /// The lazy-init helper's binding(s) (masked as `__esm`).
    helpers: HashSet<BindingId>,
    /// Modules whose wrapper the prior-version transfer already named.
    carried: usize,
}

/// What the step did, for the run's record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleNamingReport {
    /// Recorded modules (0: the step did not run).
    pub modules: usize,
    /// Modules that declare something the step asked about.
    pub asked: usize,
    /// Barrels the step named (by rule, else by the model).
    pub barrels: usize,
    /// Modules whose wrapper the transfer carried (never re-asked).
    pub carried: usize,
    /// Provider calls, every round together.
    pub calls: usize,
    /// Calls that failed (every module in them fell back).
    pub failed_calls: usize,
    /// Modules asked again because their name was taken.
    pub retried: usize,
    /// Modules given the disclosed retry.
    pub disclosed: usize,
    /// Barrels asked of the model (their loaded modules share no phrase).
    pub barrels_asked: usize,
    /// `(wrapper before, wrapper after)` per module the model's first
    /// answer or its duplicate retry named.
    pub named: Vec<(String, String)>,
    /// `(wrapper before, wrapper after)` per module the disclosed retry
    /// named.
    pub named_on_disclosure: Vec<(String, String)>,
    /// `(wrapper before, wrapper after)` per barrel named by rule.
    pub barrel_rule: Vec<(String, String)>,
    /// `(wrapper before, wrapper after)` per module the deterministic
    /// fallback named.
    pub fallback: Vec<(String, String)>,
    /// `(wrapper, answer, why)` per refused answer.
    pub refused: Vec<(String, String, String)>,
    /// Wrappers still unnamed: the floor/sweep asks them.
    pub left: Vec<String>,
}

/// The module bodies' input: the wrapper body's top-level statements (the
/// program body when the layout found no wrapper), the text they index,
/// and the run's module wrapper grammar (the toolchain's P3 piece).
pub struct ModuleSource<'j> {
    pub body: &'j [Value],
    pub text: &'j str,
    pub grammar: crate::toolchain::ModuleWrapperGrammar,
}

pub(crate) fn span_of(v: &Value) -> Option<(u32, u32)> {
    Some((
        u32::try_from(v.get("start")?.as_u64()?).ok()?,
        u32::try_from(v.get("end")?.as_u64()?).ok()?,
    ))
}

/// The init statement's wrapper identifier node (the declarator whose
/// name is the module's `init_name`).
fn wrapper_ident<'v>(stmt: &'v Value, name: &str) -> Option<&'v Value> {
    stmt.get("declarations")?
        .as_array()?
        .iter()
        .filter_map(|d| d.get("id"))
        .find(|id| id.get("name").and_then(Value::as_str) == Some(name))
}

/// Does the module declare anything besides its wrapper?
fn is_barrel(module: &FossilModule) -> bool {
    module.declared.iter().all(|d| *d == module.init_name)
}

/// Plan the step and mark the modules it names, so no wave asks their
/// wrappers. Empty (the step does not run) when the markers do not
/// describe the bundle. `helper_names` are the lazy-init helpers' names
/// in the text (`naming::plumbing::lazy_init_helpers_of`).
/// A module names mode placed at a vendor path takes the same step
/// (exp166, "restore or ask, never freeze"): its wrapper is asked unless
/// the index restored it (a restored wrapper's row is already settled, so
/// it counts as `carried`); its FILE path stays the placement's.
pub fn plan_module_naming(
    source: &ModuleSource<'_>,
    graph: &UnifiedGraph,
    state: &RenameState,
    occ: &Occurrences,
    helper_names: &HashSet<String>,
    binding_state: &mut [Lifecycle],
) -> ModulePlan {
    let body = source.body;
    let spans: Vec<(u32, u32)> = body.iter().map(|s| span_of(s).unwrap_or((0, 0))).collect();
    let factory = source.grammar.identify_factory_helper(source.text);
    let coverage = marker_coverage(body, &spans, factory.as_ref().map(|h| h.name.as_str()));
    if !crate::place::method::markers_describe_bundle(&coverage) {
        return ModulePlan::default();
    }
    // The hashes only feed the cross-version signature, which naming
    // never reads.
    let hashes = vec![String::new(); body.len()];
    let Ok(extract) = extract_fossil_modules(body, &hashes) else {
        return ModulePlan::default();
    };
    let row_of: HashMap<BindingId, usize> = graph
        .module_bindings
        .iter()
        .enumerate()
        .filter_map(|(row, b)| Some((state.view().binding_of_symbol(b.symbol)?, row)))
        .collect();
    let mut plan = ModulePlan {
        helpers: graph
            .module_bindings
            .iter()
            .filter(|b| helper_names.contains(&b.name))
            .filter_map(|b| state.view().binding_of_symbol(b.symbol))
            .collect(),
        ..ModulePlan::default()
    };
    for (i, module) in extract.modules.iter().enumerate() {
        let wrapper = wrapper_ident(&body[module.init_index], &module.init_name)
            .and_then(span_of)
            .and_then(|(start, _)| occ.binding_at(start));
        plan.wrappers.push(wrapper);
        let Some(row) = wrapper.and_then(|b| row_of.get(&b).copied()) else {
            continue;
        };
        if binding_state[row].is_pending() {
            binding_state[row]
                .mark_skipped(MODULE_STEP_SKIP, &graph.module_bindings[row].session_id);
            if is_barrel(module) {
                plan.barrels.push(i);
            } else {
                plan.asked.push(i);
            }
        } else {
            plan.carried += 1;
        }
    }
    plan.modules = extract.modules;
    plan
}

/// The read-only inputs the step's evidence and answers need.
pub struct ModuleStepInputs<'a, 's> {
    pub view: &'a TextView<'s>,
    pub occ: &'a Occurrences,
    pub source: &'a ModuleSource<'a>,
    pub stems: &'a MinifiedStems,
    pub budget: SplitNamerBudget,
    pub window: usize,
}

/// One module's answer, checked: the wrapper name to apply, or why not.
fn checked_wrapper_name(
    proposal: Option<&str>,
    old: &str,
    stems: &MinifiedStems,
) -> Result<String, &'static str> {
    let answer = proposal.ok_or(NO_ANSWER)?;
    let camel = accept_proposed_name(answer).ok_or("not-a-file-name")?;
    if is_filler_module_name(&stem_of(&camel)) {
        return Err(FILLER_NAME);
    }
    let wrapper = module_wrapper_name(&camel);
    match answer_refusal(old, &wrapper, stems) {
        Some(code) => Err(code),
        None => Ok(wrapper),
    }
}

/// A rejection that another holder of the name caused (the duplicate path).
fn is_collision(reason: Option<RejectionReason>) -> bool {
    matches!(
        reason,
        Some(
            RejectionReason::TargetInScope
                | RejectionReason::TargetVisible
                | RejectionReason::ShadowsChild
                | RejectionReason::CaptureInSubtree
                | RejectionReason::TargetFreeName
        )
    )
}

/// Why an answer was refused, as the disclosed retry tells the model.
fn refusal_text(why: &str) -> &'static str {
    match why {
        FILLER_NAME => "only generic words",
        "not-a-file-name" => "not a usable file name (generic, numbered or not kebab-case)",
        crate::naming::waves::processor::BORROWED_STEM => "it copies a minified name",
        crate::naming::waves::processor::MINIFIED_ECHO => "it repeats the minified name",
        _ => "another file already has that name",
    }
}

/// Which ask an answer came from.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Round {
    /// The first ask: a taken name is handed back for the duplicate retry.
    First,
    /// The duplicate retry.
    Duplicate,
    /// The disclosed retry.
    Disclosed,
}

/// The step at work: the plan, its inputs, what it did so far.
struct Step<'p, 'a, 's> {
    plan: &'p ModulePlan,
    inputs: &'p ModuleStepInputs<'a, 's>,
    report: ModuleNamingReport,
    /// Modules named so far.
    named: HashSet<usize>,
    /// Per module: the answers refused so far, with why.
    history: HashMap<usize, Vec<(String, String)>>,
}

impl<'p, 'a, 's> Step<'p, 'a, 's> {
    fn source<'x>(&'x self, state: &'x RenameState) -> EvidenceSource<'x, 'a>
    where
        'p: 'x,
    {
        EvidenceSource::new(
            self.inputs.source.text,
            self.inputs.view,
            self.inputs.occ,
            state,
            self.inputs.source.body,
            &self.plan.modules,
            &self.plan.wrappers,
            &self.plan.helpers,
        )
    }

    fn is_barrel(&self, m: usize) -> bool {
        is_barrel(&self.plan.modules[m])
    }

    /// Rename `binding` to `wrapper`; `how` is the trail reason of a name
    /// the model's first answer did not give.
    fn apply(
        state: &mut RenameState,
        binding: BindingId,
        old: &str,
        wrapper: &str,
        how: Option<&str>,
    ) -> Result<(), Option<RejectionReason>> {
        let request = RenameRequest {
            scope: state.scope_of_binding(binding),
            old_name: old,
            new_name: wrapper,
            expected: Some(binding),
        };
        let spec = match how {
            None => TrailSpec::settling(Tier::ModuleNaming),
            Some(_) => TrailSpec::CallerRecords {
                tier: Tier::ModuleNaming,
            },
        };
        let attempt = state.attempt_validated_rename(request, spec);
        if let Some(how) = how {
            let (outcome, why) = match attempt.reason {
                Some(r) if !attempt.applied => (Outcome::Rejected, r.as_str()),
                _ => (Outcome::Applied, how),
            };
            let row = Attempt::new(Tier::ModuleNaming, outcome)
                .proposed(wrapper)
                .reason(why);
            state.record(binding, old, row, false);
        }
        if attempt.applied {
            Ok(())
        } else {
            Err(attempt.reason)
        }
    }

    /// Note a refused answer: the report and the module's history (the
    /// disclosed retry reads it back).
    fn refuse(&mut self, m: usize, old: &str, answer: &str, why: &str) {
        self.report
            .refused
            .push((old.to_string(), answer.to_string(), why.to_string()));
        self.history
            .entry(m)
            .or_default()
            .push((answer.to_string(), why.to_string()));
    }

    /// Apply one round of answers. The first round hands a taken name back
    /// (with the name) for the duplicate retry instead of refusing it.
    fn apply_round(
        &mut self,
        asked: &[usize],
        proposals: &[Option<String>],
        state: &mut RenameState,
        round: Round,
    ) -> Vec<(usize, String)> {
        let mut taken = Vec::new();
        for (&m, proposal) in asked.iter().zip(proposals) {
            let Some(binding) = self.plan.wrappers[m] else {
                continue;
            };
            let old = state.name_of(binding).to_string();
            let answer = proposal.clone().unwrap_or_default();
            let wrapper = match checked_wrapper_name(proposal.as_deref(), &old, self.inputs.stems) {
                Ok(w) => w,
                Err(why) => {
                    let row = Attempt::new(Tier::ModuleNaming, Outcome::Rejected)
                        .reason(why)
                        .proposed(answer.as_str());
                    state.record(binding, &old, row, false);
                    self.refuse(m, &old, &answer, why);
                    continue;
                }
            };
            let how = (round == Round::Disclosed).then_some(BY_DISCLOSED_RETRY);
            match Self::apply(state, binding, &old, &wrapper, how) {
                Ok(()) => {
                    self.named.insert(m);
                    let pair = (old, wrapper);
                    if round == Round::Disclosed {
                        self.report.named_on_disclosure.push(pair);
                    } else {
                        self.report.named.push(pair);
                    }
                }
                Err(reason) if round == Round::First && is_collision(reason) => {
                    let why = reason.map_or("unknown", |r| r.as_str());
                    self.history
                        .entry(m)
                        .or_default()
                        .push((answer.clone(), why.to_string()));
                    taken.push((m, answer));
                }
                Err(reason) => {
                    let why = reason.map_or("unknown", |r| r.as_str());
                    self.refuse(m, &old, &answer, why);
                }
            }
        }
        taken
    }

    /// The step's requests for `asked`: each with its taken names (a
    /// barrel's loaded modules' names always among them) and any extra
    /// evidence line.
    fn requests(
        &self,
        state: &RenameState,
        asked: &[usize],
        taken: &HashMap<usize, Vec<String>>,
        extra: &HashMap<usize, String>,
    ) -> Vec<SplitNameRequest> {
        let source = self.source(state);
        asked
            .iter()
            .map(|&m| {
                let mut siblings = taken.get(&m).cloned().unwrap_or_default();
                if self.is_barrel(m) {
                    for name in source.loaded_names(m) {
                        if !siblings.contains(&name) {
                            siblings.push(name);
                        }
                    }
                }
                let mut evidence = source.entry(m);
                if let Some(line) = extra.get(&m) {
                    evidence.push('\n');
                    evidence.push_str(line);
                }
                SplitNameRequest {
                    kind: NameKind::Module,
                    mechanical_stem: stem_of(&self.plan.modules[m].init_name),
                    siblings,
                    bindings: Vec::new(),
                    members: None,
                    level: None,
                    evidence: Some(evidence),
                }
            })
            .collect()
    }

    /// The disclosed retry's two halves for `m`: why its answers were
    /// refused, and the taken names in the #89 owner's order (its own
    /// refused answers, the taken module names sharing their stem, then
    /// its neighbours' names), as file names.
    fn disclosure(&self, state: &RenameState, m: usize) -> (String, Vec<String>) {
        let history = self.history.get(&m).cloned().unwrap_or_default();
        let mut answered: Vec<&(String, String)> = Vec::new();
        for entry in history.iter().filter(|(a, _)| !a.is_empty()) {
            if !answered.iter().any(|(a, _)| *a == entry.0) {
                answered.push(entry);
            }
        }
        let reason = if answered.is_empty() {
            "No usable name came back for this file earlier: pick a specific one.".to_string()
        } else {
            let parts: Vec<String> = answered
                .iter()
                .map(|(a, why)| format!("\"{a}\" ({})", refusal_text(why)))
                .collect();
            format!(
                "Your earlier answers for this file were refused: {}. Pick a different, specific name.",
                parts.join("; ")
            )
        };
        let camel = |name: &str| accept_proposed_name(name).unwrap_or_else(|| name.to_string());
        let refused: Vec<String> = answered.iter().map(|(a, _)| camel(a)).collect();
        let in_use: Vec<String> = (0..self.plan.modules.len())
            .filter(|&j| j != m)
            .filter_map(|j| self.plan.wrappers[j])
            .filter_map(|b| module_stem_of_wrapper(state.name_of(b)))
            .map(|stem| camel(&stem))
            .collect();
        let source = self.source(state);
        let neighbours: Vec<String> = source
            .loaded(m)
            .into_iter()
            .filter_map(|j| source.module_name(j))
            .map(|stem| camel(&stem))
            .collect();
        let taken = build_retry_used_names(&refused, &in_use, &neighbours)
            .into_iter()
            .map(|n| stem_of(&n))
            .collect();
        (reason, taken)
    }

    /// Ask the model about `ms` and follow the ladder: the duplicate
    /// retry, the disclosed retry, the deterministic fallback.
    fn settle(&mut self, ms: &[usize], state: &mut RenameState, namer: &mut ProviderSplitNamer) {
        if ms.is_empty() {
            return;
        }
        let first = self.requests(state, ms, &HashMap::new(), &HashMap::new());
        let proposals = namer.name(&first);
        let taken = self.apply_round(ms, &proposals, state, Round::First);
        if !taken.is_empty() {
            let again: Vec<usize> = taken.iter().map(|(m, _)| *m).collect();
            let names: HashMap<usize, Vec<String>> =
                taken.into_iter().map(|(m, n)| (m, vec![n])).collect();
            let retry = self.requests(state, &again, &names, &HashMap::new());
            self.report.retried += again.len();
            let proposals = namer.name(&retry);
            self.apply_round(&again, &proposals, state, Round::Duplicate);
        }
        let rest: Vec<usize> = ms
            .iter()
            .copied()
            .filter(|m| !self.named.contains(m) && self.plan.wrappers[*m].is_some())
            .collect();
        if !rest.is_empty() {
            let mut names = HashMap::new();
            let mut lines = HashMap::new();
            for &m in &rest {
                let (line, taken) = self.disclosure(state, m);
                names.insert(m, taken);
                lines.insert(m, line);
            }
            let retry = self.requests(state, &rest, &names, &lines);
            self.report.disclosed += rest.len();
            let proposals = namer.name(&retry);
            self.apply_round(&rest, &proposals, state, Round::Disclosed);
        }
        for &m in ms {
            if !self.named.contains(&m) {
                self.fallback(m, state);
            }
        }
    }

    /// The deterministic name of a module the model could not name:
    /// `init<FirstDeclared>`, else `init<FirstDeclared>Module` (its first
    /// function, else its first declared name, that makes a usable name);
    /// a barrel's
    /// `init<FirstLoaded>Index`, else `init<FirstLoaded>Exports`; else the
    /// suffix ladder on the last. Never minified, never filler; a module
    /// with no usable word is left to the floor/sweep.
    fn fallback(&mut self, m: usize, state: &mut RenameState) {
        let Some(binding) = self.plan.wrappers[m] else {
            return;
        };
        let old = state.name_of(binding).to_string();
        let words: Vec<String> = {
            let source = self.source(state);
            if self.is_barrel(m) {
                source
                    .loaded_names(m)
                    .first()
                    .map(|n| barrels::rule_candidates(barrels::base_name(n))[1..].to_vec())
                    .unwrap_or_default()
            } else {
                source
                    .declared_in_stem_order(m)
                    .iter()
                    .map(|n| stem_of(n))
                    .find(|stem| checked_wrapper_name(Some(stem), &old, self.inputs.stems).is_ok())
                    .map(|stem| vec![stem.clone(), format!("{stem}-module")])
                    .unwrap_or_default()
            }
        };
        let candidates: Vec<String> = words
            .iter()
            .filter_map(|w| checked_wrapper_name(Some(w), &old, self.inputs.stems).ok())
            .collect();
        let Some(last) = candidates.last().cloned() else {
            self.report.left.push(old);
            return;
        };
        let scope = state.scope_of_binding(binding);
        let wrapper = candidates
            .into_iter()
            .find(|n| state.get_rename_rejection(scope, &old, n).is_none())
            .unwrap_or_else(|| {
                crate::naming::reask::ladder(
                    &old,
                    &last,
                    |_| false,
                    |n| state.get_rename_rejection(scope, &old, n),
                )
            });
        match Self::apply(state, binding, &old, &wrapper, Some(BY_FALLBACK)) {
            Ok(()) => {
                self.named.insert(m);
                self.report.fallback.push((old, wrapper));
            }
            Err(_) => self.report.left.push(old),
        }
    }

    /// Name one barrel by rule from its loaded modules' names; false when
    /// the rule has no name for it (the model is asked).
    fn barrel_rule(&mut self, m: usize, state: &mut RenameState) -> bool {
        let Some(binding) = self.plan.wrappers[m] else {
            return true;
        };
        let Some(phrase) = barrels::shared_phrase(&self.source(state).loaded_names(m)) else {
            return false;
        };
        let old = state.name_of(binding).to_string();
        let scope = state.scope_of_binding(binding);
        for candidate in barrels::rule_candidates(&phrase) {
            let Ok(wrapper) = checked_wrapper_name(Some(&candidate), &old, self.inputs.stems)
            else {
                continue;
            };
            if state.get_rename_rejection(scope, &old, &wrapper).is_some() {
                continue;
            }
            if Self::apply(state, binding, &old, &wrapper, Some(BY_BARREL_RULE)).is_ok() {
                self.named.insert(m);
                self.report.barrel_rule.push((old, wrapper));
                return true;
            }
        }
        false
    }

    /// Name the barrels, loaded barrels before the barrels that load them
    /// (a cycle is broken by taking what is left): by rule, else by the
    /// model with the ladder.
    fn name_barrels(&mut self, state: &mut RenameState, namer: &mut ProviderSplitNamer) {
        let pending: Vec<usize> = self.plan.barrels.clone();
        let in_step: HashSet<usize> = pending.iter().copied().collect();
        let mut done: HashSet<usize> = HashSet::new();
        while done.len() < pending.len() {
            let waiting: Vec<usize> = pending
                .iter()
                .copied()
                .filter(|m| !done.contains(m))
                .collect();
            let mut ready: Vec<usize> = waiting
                .iter()
                .copied()
                .filter(|&m| {
                    self.plan.modules[m]
                        .imports
                        .iter()
                        .all(|j| *j == m || !in_step.contains(j) || done.contains(j))
                })
                .collect();
            if ready.is_empty() {
                ready = waiting;
            }
            let ask: Vec<usize> = ready
                .iter()
                .copied()
                .filter(|&m| !self.barrel_rule(m, state))
                .collect();
            self.report.barrels_asked += ask.len();
            self.settle(&ask, state, namer);
            done.extend(ready);
        }
    }
}

/// Run the step over the plan: the modules that declare something (ask,
/// check, apply, the retry ladder), then the barrels. Every rename goes
/// through the validated renamer.
pub fn run_module_naming<P: NameProvider>(
    plan: &ModulePlan,
    inputs: &ModuleStepInputs<'_, '_>,
    state: &mut RenameState,
    provider: &P,
    log: &mut DispatchLog,
) -> ModuleNamingReport {
    let report = ModuleNamingReport {
        modules: plan.modules.len(),
        asked: plan.asked.len(),
        barrels: plan.barrels.len(),
        carried: plan.carried,
        ..ModuleNamingReport::default()
    };
    if plan.asked.is_empty() && plan.barrels.is_empty() {
        return report;
    }
    let mut namer = ProviderSplitNamer::with_budget(provider, log, inputs.budget);
    namer.window = inputs.window;
    let mut step = Step {
        plan,
        inputs,
        report,
        named: HashSet::new(),
        history: HashMap::new(),
    };
    step.settle(&plan.asked, state, &mut namer);
    step.name_barrels(state, &mut namer);
    let mut report = step.report;
    report.calls = namer.calls;
    report.failed_calls = namer.failed_batches;
    report
}
