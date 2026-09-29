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
//! class its failure list maps to.

use crate::naming::reask::ReaskClass;

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
    /// cause, verbatim (only the re-askable classes ever appear —
    /// `Unrecoverable` re-asks nothing).
    fn from(class: ReaskClass) -> Self {
        match class {
            ReaskClass::NameTaken => RetryCause::NameTaken,
            ReaskClass::InvalidSuggestion => RetryCause::InvalidSuggestion,
            ReaskClass::Unrecoverable => RetryCause::NameTaken,
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
