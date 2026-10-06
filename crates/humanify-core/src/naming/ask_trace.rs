//! The ask trace (`--dump-asks <path>`) — the reason taxonomy for "why was
//! an identifier asked of the model" (see docs/responsibility.md, "why an
//! identifier was asked").
//!
//! Every LLM call in the naming pipeline is a FALL-THROUGH RECORD: the
//! deterministic machinery (matching, cascades, votes, transfer, placement)
//! either resolved a name or fell through to asking. The ask — which scope,
//! which identifiers, which wave/round, first ask or re-ask and why — is
//! the complete decision signal; the model's ANSWER is irrelevant for path
//! debugging.
//!
//! The taxonomy reuses [`crate::naming::reask`]'s classes verbatim as the
//! re-ask cause vocabulary ([`RetryCause`]), so the two can only drift
//! together: a barrier/sweep re-ask records `reask::ReaskClass` as it was
//! computed by the policy that seeded it, and a lane loop re-ask (which
//! re-asks a rejected RESPONSE, not an applier rejection) records the one
//! class its failure list maps to — UNLESS the window's retries include a
//! suggestion the scope-safety check rejected at claim time (the `late`
//! flow): then the rejection's own class and code are recorded
//! ([`AskSite::lane_reask`]), because the lane's duplicate failure is the
//! generic view of a rejection whose reason it now knows.

use crate::naming::reask::ReaskClass;
use crate::rename::validated::RejectionReason;

/// Why an ask happened — the one taxonomy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AskReason {
    /// A function's main-pass first ask, no prior-version context.
    Fresh,
    /// A function ask carrying a close-match prior-version context
    /// (`priorVersionCode` / prior-name hints).
    PriorHinted,
    /// The round-B shadowed-binding pass (lane phase 1).
    Shadowed,
    /// Any re-ask (`request.is_retry`); the cause is the [`RetryCause`].
    Retry,
    /// A module-binding group ask.
    ModuleLane,
    /// The coverage sweep's first round.
    Sweep,
    /// The vendor namer's batch ask.
    Vendor,
    /// The split's file namer / tree reviser.
    Folders,
    /// The module namer's batch ask (`naming::module_names`).
    Modules,
}

impl AskReason {
    /// The taxonomy's wire names, one word each (`--dump-asks` rows and the
    /// `diff-asks` comparator read these — never rename one in place).
    pub fn as_str(&self) -> &'static str {
        match self {
            AskReason::Fresh => "fresh",
            AskReason::PriorHinted => "prior-hinted",
            AskReason::Shadowed => "shadowed",
            AskReason::Retry => "retry",
            AskReason::ModuleLane => "module-lane",
            AskReason::Sweep => "sweep",
            AskReason::Vendor => "vendor",
            AskReason::Folders => "folders",
            AskReason::Modules => "modules",
        }
    }
}

/// A re-ask's cause. [`ReaskClass`] verbatim for the applier-rejection
/// re-asks (the wave barrier's and the sweep's — the same classes reask.rs
/// gates them by), plus the lane loop's own per-response failure kinds,
/// which have no reask class (it re-asks a rejected RESPONSE).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RetryCause {
    /// `ReaskClass::NameTaken` — the name is taken (a used-set collision,
    /// or a target-in-scope / target-visible / shadows-child /
    /// target-free-name rejection).
    NameTaken,
    /// `ReaskClass::InvalidSuggestion` — the suggestion was not a legal
    /// rename target (`invalid-target`).
    InvalidSuggestion,
    /// `ReaskClass::Unrecoverable` — no suggestion can fix the rejection.
    /// The barrier/sweep never re-ask one; the LANE round-2 can carry one
    /// (a late `exported-name` rides the duplicate preamble — the flow fix
    /// is eval-gated), and its record must say unrecoverable, not the
    /// generic NameTaken the failure lists derive.
    Unrecoverable,
    /// The lane loop's `missing` failure — no suggestion for the id.
    Missing,
    /// The lane loop's `unchanged` failure — the model returned the name
    /// itself.
    Unchanged,
}

impl RetryCause {
    /// The cause's wire name (the reask.rs class names verbatim).
    pub fn as_str(&self) -> &'static str {
        match self {
            RetryCause::NameTaken => "NameTaken",
            RetryCause::InvalidSuggestion => "InvalidSuggestion",
            RetryCause::Unrecoverable => "Unrecoverable",
            RetryCause::Missing => "Missing",
            RetryCause::Unchanged => "Unchanged",
        }
    }

    /// The lane loop's re-ask cause from the request's failure lists
    /// (`BatchRenameRequest.failures`): the FIRST list that is non-empty
    /// decides, in the order the retry prompt renders them.
    pub fn of_failures(f: &humanify_model::llm::RenameFailures) -> Option<RetryCause> {
        if !f.duplicates.is_empty() {
            Some(RetryCause::NameTaken)
        } else if !f.invalid.is_empty() {
            Some(RetryCause::InvalidSuggestion)
        } else if !f.missing.is_empty() {
            Some(RetryCause::Missing)
        } else if !f.unchanged.is_empty() {
            Some(RetryCause::Unchanged)
        } else {
            None
        }
    }
}

impl From<ReaskClass> for RetryCause {
    /// The vocabulary reuse: an applier-rejection re-ask's class is the
    /// cause, verbatim. `Unrecoverable` appears only on the lane round-2
    /// record (the barrier/sweep re-ask nothing of that class).
    fn from(class: ReaskClass) -> Self {
        match class {
            ReaskClass::NameTaken => RetryCause::NameTaken,
            ReaskClass::InvalidSuggestion => RetryCause::InvalidSuggestion,
            ReaskClass::Unrecoverable => RetryCause::Unrecoverable,
            // A key-mismatch re-ask asks again for an id the answer left
            // unanswered: the lane loop's `missing` cause.
            ReaskClass::AnswerKey => RetryCause::Missing,
        }
    }
}

/// What kind of scope an ask names (the writer derives it from the dispatch
/// site; the reason derivation reads it).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AskScope {
    /// A function lane's ask (`site: "naming"`, a function's session id).
    Fn,
    /// A module-binding group ask (`site: "naming"`, `module-binding-batch`).
    Module,
    /// The coverage sweep (`site: "sweep"`).
    Sweep,
    /// The vendor namer (`site: "vendor"`).
    Vendor,
    /// The split's file namer / tree reviser (`site: "folders"`).
    Folders,
    /// The module namer (`site: "modules"`).
    Modules,
}

/// The ask-site context a dispatch records that its request cannot express.
/// Carried on every dispatch record ([`crate::naming::waves::processor::DispatchRecord`],
/// [`crate::naming::passes::sweep::SweepDispatch`]); the writer derives the
/// rest of the row from the request.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct AskSite {
    /// The lane's phase (0 = main pass, 1 = the shadowed round-B pass).
    pub phase: u8,
    /// Whether the ask carries prior-version context that lives outside the
    /// request: a module lane's prior SUGGESTED NAMES are rendered into its
    /// prompt text, so the site records them here (a fn ask's close-match
    /// context IS in the request — `priorVersionCode`/hints — and the writer
    /// ORs it in).
    pub prior: bool,
    /// The recorded re-ask cause (an applier-rejection re-ask's reask
    /// class); a lane's round-2 asks leave it unset and the writer derives
    /// it from the request's `failures`.
    pub cause: Option<RetryCause>,
    /// The applier's rejection codes (e.g. "target-free-name"), when the
    /// re-ask was seeded from a validated rejection.
    pub detail: Option<String>,
}

impl AskSite {
    /// A first-round ask's site record (no cause).
    pub fn fresh(phase: u8) -> AskSite {
        AskSite {
            phase,
            prior: false,
            cause: None,
            detail: None,
        }
    }

    /// A barrier/sweep re-ask's site record: the reask class (verbatim),
    /// the lane phase it retries, and the rejection code it was computed
    /// from.
    pub fn reask(class: ReaskClass, phase: u8, detail: Option<&str>) -> AskSite {
        AskSite {
            phase,
            prior: false,
            cause: Some(RetryCause::from(class)),
            detail: detail.map(str::to_string),
        }
    }

    /// A lane round-2's site record. `rejections` is the call's per-id
    /// scope-check rejections (the `late` flow — `(id, reason)` in batch
    /// order, empty for a plain duplicate/invalid/missing retry): the
    /// FIRST recorded rejection's reask class is the ask's cause and every
    /// distinct reason code is the detail, in preference to the writer's
    /// generic failure-list derivation (a late rejection IS a duplicate
    /// failure to the flow, so the derivation alone cannot name the real
    /// class — an export id would read NameTaken, the audit's mis-worded
    /// round-2). Recording only: the retry flow is unchanged.
    pub fn lane_reask(rejections: &[(String, RejectionReason)], phase: u8) -> AskSite {
        match rejections.first() {
            None => AskSite::fresh(phase),
            Some(&(_, reason)) => {
                let mut codes: Vec<&str> = Vec::new();
                for &(_, r) in rejections {
                    let code = r.as_str();
                    if !codes.contains(&code) {
                        codes.push(code);
                    }
                }
                AskSite::reask(
                    crate::naming::reask::class_of(reason),
                    phase,
                    Some(&codes.join(",")),
                )
            }
        }
    }
}

/// The ask's reason — the ONE derivation (the writer and the tests read
/// this; nothing else re-derives it). A re-ask is a re-ask FIRST (its
/// cause field carries the why; the record keeps the lane phase and prior
/// flag); a shadowed round-B pass and the module/sweep/vendor/folders
/// variants read off the scope.
pub fn reason_of(
    site: &AskSite,
    scope: AskScope,
    request: &humanify_model::llm::BatchRenameRequest,
) -> AskReason {
    if request.is_retry == Some(true) {
        return AskReason::Retry;
    }
    match scope {
        AskScope::Fn if site.phase == 1 => AskReason::Shadowed,
        AskScope::Fn => {
            if site.prior {
                AskReason::PriorHinted
            } else {
                AskReason::Fresh
            }
        }
        AskScope::Module => AskReason::ModuleLane,
        AskScope::Sweep => AskReason::Sweep,
        AskScope::Vendor => AskReason::Vendor,
        AskScope::Folders => AskReason::Folders,
        AskScope::Modules => AskReason::Modules,
    }
}

/// The request's own prior-version context (the close-match fields the fn
/// path sets). The writer ORs this with the SITE's `prior` flag — a module
/// lane's prior suggested names never appear in its request.
pub fn prior_context_of(request: &humanify_model::llm::BatchRenameRequest) -> bool {
    request.prior_version_code.is_some()
        || request.prior_version_names.is_some()
        || request.prior_name_hints.is_some()
}

#[cfg(test)]
mod ask_trace_test;
