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
//!    it — on a fresh run, every module) and that declares something
//!    besides its wrapper is marked "named by the module step", so no wave
//!    asks its wrapper. A BARREL (declares nothing; its setup code only
//!    loads other modules) is left to the waves: the prompt would have
//!    only the names of what it loads, and in the 60-module test 4 of 8
//!    barrel answers came back generic (`module-loader`).
//! 2. [`run_module_naming`], after the waves and the library-prefix pass,
//!    before the naming floor and sweep: one batched pass over the marked
//!    modules through the file namer's module kind
//!    ([`crate::place::assign::namer::NameKind::Module`], 8 per call),
//!    showing each module's evidence ([`evidence`]) under the names the
//!    waves chose. Each answer is checked ([`accept_proposed_name`], then
//!    the waves' answer-quality checks — no borrowed minified stem, no
//!    echo) and applied as `init<Name>` through the validated renamer,
//!    recorded on the trail under [`Tier::ModuleNaming`]. Duplicates: the
//!    first module in bundle order keeps the name; every later one is
//!    asked ONCE more in a small batch that lists the taken names; one
//!    still refused or colliding stays unnamed and the floor/sweep asks
//!    it — the disclosed path, so nothing is left minified, and its file
//!    falls back to the mechanical stem, today's behaviour.
//!
//! The split reads the name back from the wrapper
//! ([`crate::place::stems::module_stem_of_wrapper`]): the wrapper's name
//! is the single record of the module's name, so the file can never
//! disagree with it. A matched module keeps its prior path verbatim (the
//! split's own rule), and a carried wrapper is never re-asked.

pub mod evidence;

use std::collections::{HashMap, HashSet};

use humanify_model::llm::NameProvider;
use serde_json::Value;

use crate::artifact_dump::DispatchLog;
use crate::graph::UnifiedGraph;
use crate::naming::waves::generate::TextView;
use crate::naming::waves::processor::answer_refusal;
use crate::naming::waves::render::Occurrences;
use crate::place::assign::namer::{
    NameKind, ProviderSplitNamer, SplitNameRequest, SplitNamer, SplitNamerBudget,
};
use crate::place::stems::{accept_proposed_name, module_wrapper_name, stem_of};
use crate::rename::floor::MinifiedStems;
use crate::rename::transfer::lifecycle::Lifecycle;
use crate::rename::validated::scopes::BindingId;
use crate::rename::validated::{RejectionReason, RenameRequest, RenameState, TrailSpec};
use crate::trail::{Attempt, Outcome, Tier};
use crate::twins::fossil::{FossilModule, extract_fossil_modules, marker_coverage};
use evidence::EvidenceSource;

#[cfg(test)]
mod module_names_test;

/// The lifecycle reason a marked wrapper is skipped by the waves under.
pub const MODULE_STEP_SKIP: &str = "module-naming";

/// The step's plan: the recorded modules and which ones it names.
#[derive(Default)]
pub struct ModulePlan {
    modules: Vec<FossilModule>,
    /// Per module: the wrapper's binding (None: not resolved).
    wrappers: Vec<Option<BindingId>>,
    /// Module indexes the step names, bundle order.
    asked: Vec<usize>,
    /// The lazy-init helper's binding(s) (masked as `__esm`).
    helpers: HashSet<BindingId>,
    /// Modules left to the waves because they declare nothing.
    barrels: usize,
    /// Modules whose wrapper the prior-version transfer already named.
    carried: usize,
}

/// What the step did, for the run's record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleNamingReport {
    /// Recorded modules (0: the step did not run).
    pub modules: usize,
    /// Modules the step asked about.
    pub asked: usize,
    /// Barrels left to the waves.
    pub barrels: usize,
    /// Modules whose wrapper the transfer carried (never re-asked).
    pub carried: usize,
    /// Provider calls, first round and the duplicate retry together.
    pub calls: usize,
    /// Calls that failed (every module in them fell back).
    pub failed_calls: usize,
    /// Modules asked again because their name was taken.
    pub retried: usize,
    /// `(wrapper before, wrapper after)` per module named.
    pub named: Vec<(String, String)>,
    /// `(wrapper, answer, why)` per module left to the floor/sweep.
    pub refused: Vec<(String, String, String)>,
}

/// The module bodies' input: the wrapper body's top-level statements (the
/// program body when the layout found no wrapper), the text they index,
/// and the run's module wrapper grammar (the toolchain's P3 piece).
pub struct ModuleSource<'j> {
    pub body: &'j [Value],
    pub text: &'j str,
    pub grammar: crate::toolchain::ModuleWrapperGrammar,
}

fn span_of(v: &Value) -> Option<(u32, u32)> {
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
        if is_barrel(module) {
            plan.barrels += 1;
            continue;
        }
        let Some(row) = wrapper.and_then(|b| row_of.get(&b).copied()) else {
            continue;
        };
        if binding_state[row].is_pending() {
            binding_state[row]
                .mark_skipped(MODULE_STEP_SKIP, &graph.module_bindings[row].session_id);
            plan.asked.push(i);
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
    let answer = proposal.ok_or("no-answer")?;
    let camel = accept_proposed_name(answer).ok_or("not-a-file-name")?;
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

/// What applying one round left: the modules whose name was taken, with
/// the name they wanted (kebab).
type Taken = Vec<(usize, String)>;

/// Name the planned modules. `retry` false on the first round: a taken
/// name is handed back for the one retry instead of refused.
fn apply_round(
    plan: &ModulePlan,
    asked: &[usize],
    proposals: &[Option<String>],
    state: &mut RenameState,
    stems: &MinifiedStems,
    retry: bool,
    report: &mut ModuleNamingReport,
) -> Taken {
    let mut taken = Vec::new();
    for (&m, proposal) in asked.iter().zip(proposals) {
        let Some(binding) = plan.wrappers[m] else {
            continue;
        };
        let old = state.name_of(binding).to_string();
        let refuse =
            |state: &mut RenameState, report: &mut ModuleNamingReport, why: &str, answer: &str| {
                let row = Attempt::new(Tier::ModuleNaming, Outcome::Rejected)
                    .reason(why)
                    .proposed(answer);
                state.record(binding, &old, row, false);
                report
                    .refused
                    .push((old.clone(), answer.to_string(), why.to_string()));
            };
        let wrapper = match checked_wrapper_name(proposal.as_deref(), &old, stems) {
            Ok(w) => w,
            Err(why) => {
                refuse(state, report, why, proposal.as_deref().unwrap_or(""));
                continue;
            }
        };
        let attempt = state.attempt_validated_rename(
            RenameRequest {
                scope: state.scope_of_binding(binding),
                old_name: &old,
                new_name: &wrapper,
                expected: Some(binding),
            },
            TrailSpec::Standard {
                tier: Tier::ModuleNaming,
                post_pass: false,
            },
        );
        if attempt.applied {
            report.named.push((old, wrapper));
        } else if !retry && is_collision(attempt.reason) {
            taken.push((m, proposal.clone().unwrap_or_default()));
        } else {
            let why = attempt.reason.map_or("unknown", |r| r.as_str());
            report.refused.push((
                old.clone(),
                proposal.clone().unwrap_or_default(),
                why.to_string(),
            ));
        }
    }
    taken
}

/// The step's requests for `asked`, each with its taken names.
fn requests(
    source: &EvidenceSource<'_, '_>,
    asked: &[usize],
    taken: &HashMap<usize, Vec<String>>,
) -> Vec<SplitNameRequest> {
    asked
        .iter()
        .map(|&m| SplitNameRequest {
            kind: NameKind::Module,
            mechanical_stem: stem_of(&source.modules[m].init_name),
            siblings: taken.get(&m).cloned().unwrap_or_default(),
            bindings: Vec::new(),
            members: None,
            level: None,
            evidence: Some(source.entry(m)),
        })
        .collect()
}

/// Run the step over the plan: ask, check, apply, retry the duplicates
/// once. Every rename goes through the validated renamer.
pub fn run_module_naming<P: NameProvider>(
    plan: &ModulePlan,
    inputs: &ModuleStepInputs<'_, '_>,
    state: &mut RenameState,
    provider: &P,
    log: &mut DispatchLog,
) -> ModuleNamingReport {
    let mut report = ModuleNamingReport {
        modules: plan.modules.len(),
        asked: plan.asked.len(),
        barrels: plan.barrels,
        carried: plan.carried,
        ..ModuleNamingReport::default()
    };
    if plan.asked.is_empty() {
        return report;
    }
    let mut namer = ProviderSplitNamer::with_budget(provider, log, inputs.budget);
    namer.window = inputs.window;
    let first = {
        let source = EvidenceSource::new(
            inputs.source.text,
            inputs.view,
            inputs.occ,
            state,
            inputs.source.body,
            &plan.modules,
            &plan.wrappers,
            &plan.helpers,
        );
        requests(&source, &plan.asked, &HashMap::new())
    };
    let proposals = namer.name(&first);
    let taken = apply_round(
        plan,
        &plan.asked,
        &proposals,
        state,
        inputs.stems,
        false,
        &mut report,
    );
    if !taken.is_empty() {
        let again: Vec<usize> = taken.iter().map(|(m, _)| *m).collect();
        let names: HashMap<usize, Vec<String>> =
            taken.into_iter().map(|(m, n)| (m, vec![n])).collect();
        let retry = {
            let source = EvidenceSource::new(
                inputs.source.text,
                inputs.view,
                inputs.occ,
                state,
                inputs.source.body,
                &plan.modules,
                &plan.wrappers,
                &plan.helpers,
            );
            requests(&source, &again, &names)
        };
        report.retried = again.len();
        let proposals = namer.name(&retry);
        apply_round(
            plan,
            &again,
            &proposals,
            state,
            inputs.stems,
            true,
            &mut report,
        );
    }
    report.calls = namer.calls;
    report.failed_calls = namer.failed_batches;
    report
}
