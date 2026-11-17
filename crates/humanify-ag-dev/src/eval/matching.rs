//! The matching counts of a scored run, read from its stats file: what the
//! pipeline carried, close-matched, left unmatched and sent to the LLM.

use super::llm_diag::{Col, Spec, sum};

const FUNCTIONS: &[Col] = &[
    sum("fnTotal", "coverage.functions.total"),
    sum("fnCarried", "coverage.functions.cached"),
    sum("fnClose", "coverage.functions.closeMatch"),
    sum(
        "fnAsked",
        "coverage.functions.llm+coverage.functions.failed",
    ),
    sum("fnLLMRenamed", "coverage.functions.llm"),
    sum("fnUnmatched", "resolutionStats.unmatched"),
    sum("fnAmbiguous", "resolutionStats.stillAmbiguous"),
];

const MODULE_BINDINGS: &[Col] = &[
    sum("mbCarried", "coverage.moduleBindings.cached"),
    sum("mbLLMRenamed", "coverage.moduleBindings.llm"),
];

const IDENTIFIERS: &[Col] = &[
    sum("idAsked", "coverage.identifiers.total"),
    sum("idLLMRenamed", "coverage.identifiers.llm"),
];

/// The matching counts.
pub const MATCHING: Spec = Spec {
    present: "coverage",
    groups: &[
        (
            "FUNCTIONS (total; carried from the prior; close-matched; asked the LLM = stats llm+failed; asked AND renamed by the LLM = stats llm; unmatched; still ambiguous)",
            FUNCTIONS,
        ),
        (
            "MODULE BINDINGS (carried from the prior; asked AND renamed by the LLM; statement-twin names are in neither)",
            MODULE_BINDINGS,
        ),
        (
            "IDENTIFIERS (asked the LLM; asked AND renamed by the LLM)",
            IDENTIFIERS,
        ),
    ],
    caption: "MATCHING. EXACT columns (fnTotal fnClose fnAsked fnUnmatched fnAmbiguous) must be EQUAL across same-commit repeats: a difference is a harness or determinism problem, not the LLM. The other columns follow the cold-built base's names and the model's draws. '-' = not in this run's stats.",
};

#[cfg(test)]
#[path = "matching_test.rs"]
mod matching_test;
