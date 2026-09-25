//! Library functions in a mixed file — TS plugin.ts `detectAndMarkLibraries`
//! / `markLibraryFunctionsPreDone` (the freeze, before any transfer) and
//! `runLibraryPrefixPass` (after the waves), with
//! `src/rename/library-prefix-resolver.ts`: every eligible binding of a
//! library function is renamed `<sanitized library>_<name>` through
//! validated rename — deterministic, no LLM.
//!
//! The classification itself is a HOOK ([`LibraryHook`]): consulted only
//! when `skipLibraries` is on and the graph found no wrapper IIFE.

use oxc_span::Span;

use crate::graph::UnifiedGraph;
use crate::naming::report::{
    IdentifierOutcome, Outcomes, RenameReport, ReportStrategy, ReportType, Status,
};
use crate::rename::eligibility::Eligibility;
use crate::rename::transfer::rows::Rows;
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};

/// One `artifactDump.recordName` row from a path the strategy trail does
/// not see. `span` None = the TS's no-position sentinel (-1/-1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedName {
    pub span: Option<Span>,
    pub old_name: String,
    pub new_name: Option<String>,
    pub module: bool,
    /// "renamed" | "unchanged".
    pub classified: &'static str,
    pub function_id: String,
}

/// What the library-prefix pass reports.
#[derive(Clone, Debug, Default)]
pub struct LibraryOutcome {
    pub reports: Vec<RenameReport>,
    /// Library functions with no eligible binding (`libraryNoMinified`).
    pub no_minified: usize,
    pub names: Vec<RecordedName>,
}

/// `sanitizeLibraryName`: drop a leading `@`, `/ - .` → `_`, lowercase,
/// `_` in front of a leading digit.
pub fn sanitize_library_name(name: &str) -> String {
    let name = name.strip_prefix('@').unwrap_or(name);
    let mut out: String = name
        .chars()
        .map(|c| if matches!(c, '/' | '-' | '.') { '_' } else { c })
        .collect::<String>()
        .to_lowercase();
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// The library stage's classification hook (`markLibraryFunctionsPreDone`'s
/// input): (fn row, library name) for every library function of the file,
/// given the fresh text and its graph. NOT PORTED YET — the TS's
/// comment-region classification has open bugs (16-findings #32/#33, fixed
/// TS-first first); the stage that owns it wires this hook. The driver
/// consults it only when `skipLibraries` is on and there is no wrapper.
pub type LibraryHook<'h> = &'h dyn Fn(&str, &UnifiedGraph) -> Vec<(usize, String)>;

/// `detectAndMarkLibraries`: the hook's classification, or none.
pub fn classify_library_functions(
    fresh: &str,
    graph: &UnifiedGraph,
    has_wrapper: bool,
    skip_libraries: bool,
    hook: Option<LibraryHook<'_>>,
) -> Vec<(usize, String)> {
    match hook {
        Some(classify) if skip_libraries && !has_wrapper => classify(fresh, graph),
        _ => Vec::new(),
    }
}

/// `runLibraryPrefixPass` over the classified functions.
pub fn run_library_prefix_pass(
    state: &mut RenameState,
    rows: &Rows,
    graph: &UnifiedGraph,
    library: &[(usize, String)],
    eligible: &Eligibility,
) -> LibraryOutcome {
    let mut out = LibraryOutcome::default();
    for (f, lib) in library {
        let prefix = sanitize_library_name(lib);
        let scope = rows.fns[*f].scope;
        let fn_id = graph.functions[*f].session_id.clone();
        let identifiers: Vec<String> = state
            .bindings_in(scope)
            .into_iter()
            .map(|(n, _)| n)
            .filter(|n| eligible.is_eligible(n))
            .collect();
        if identifiers.is_empty() {
            out.no_minified += 1;
            continue;
        }
        let mut outcomes = Outcomes::default();
        let mut renamed = 0;
        for old in &identifiers {
            let new = format!("{prefix}_{old}");
            let attempt = state.attempt_validated_rename(
                RenameRequest {
                    scope,
                    old_name: old,
                    new_name: &new,
                    expected: None,
                },
                TrailSpec::Untrailed {
                    why: "library-prefix",
                },
            );
            if attempt.applied {
                renamed += 1;
                outcomes.set(old, IdentifierOutcome::renamed(&new, 1, None));
            } else {
                outcomes.set(
                    old,
                    IdentifierOutcome {
                        status: Status::Unchanged {
                            attempts: 1,
                            suggestion: None,
                        },
                        trail: None,
                    },
                );
            }
            // `scope.bindings[oldName]` read AFTER the rename: an applied
            // one re-keyed it, so the row carries no position (-1/-1).
            let span = state
                .binding_in(scope, old)
                .map(|b| state.view().binding(b).id_span);
            out.names.push(RecordedName {
                span,
                old_name: old.clone(),
                new_name: attempt.applied.then(|| new.clone()),
                module: false,
                classified: if attempt.applied {
                    "renamed"
                } else {
                    "unchanged"
                },
                function_id: fn_id.clone(),
            });
        }
        out.reports.push(RenameReport {
            ty: ReportType::Function,
            strategy: ReportStrategy::LibraryPrefix,
            target_id: fn_id,
            total_identifiers: identifiers.len(),
            renamed_count: renamed,
            outcomes,
            total_llm_calls: None,
            finish_reasons: Vec::new(),
            structural_hash: None,
        });
    }
    out
}
