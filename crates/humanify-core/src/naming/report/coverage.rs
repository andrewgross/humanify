//! `buildCoverageSummary` + `formatCoverageSummary` (src/rename/coverage.ts).
//! The record is the stats model's [`CoverageSummary`] (its key order is
//! the TS construction order); the printed block is what the CLI shows.

use humanify_model::js::{format_duration, to_fixed};
use humanify_model::stats::{
    BindingProvenance as StatsBindingProvenance, CoverageSummary, IdentifierCounts, LlmCoverage,
    ProvenanceSplit, RenameCounts,
};

use super::{RenameReport, ReportStrategy, ReportType, SkipReasons};
use crate::naming::passes::census::MintedCensus;
use crate::rename::floor::is_single_letter;
use crate::trail::StrategyTrail;

/// The inputs `buildCoverageSummary` reads besides the reports.
#[derive(Clone, Copy, Debug, Default)]
pub struct CoverageInputs {
    pub total_functions: usize,
    pub skipped_by_skip_list: usize,
    pub skip_reasons: SkipReasons,
    pub library_no_minified: usize,
    pub prior_version_applied: usize,
    pub prior_version_already_named: usize,
    pub prior_version_bindings_applied: usize,
    pub prior_version_close_match: usize,
    /// The metrics' completed LLM calls and retries.
    pub llm_calls: usize,
    pub llm_retries: usize,
    /// The metrics' average response time (wall clock — not a decision).
    pub avg_response_time_ms: f64,
    pub elapsed_ms: f64,
}

fn bump(counts: &mut RenameCounts, strategy: ReportStrategy, by: f64) {
    match strategy {
        ReportStrategy::Llm => counts.llm += by,
        ReportStrategy::LibraryPrefix => counts.library_prefix += by,
    }
}

fn bump_ids(counts: &mut IdentifierCounts, strategy: ReportStrategy) {
    match strategy {
        ReportStrategy::Llm => counts.llm += 1.0,
        ReportStrategy::LibraryPrefix => counts.library_prefix += 1.0,
    }
}

/// `buildCoverageSummary`.
pub fn build_coverage_summary(reports: &[RenameReport], inp: &CoverageInputs) -> CoverageSummary {
    let total_functions = inp.total_functions as f64;
    let mut functions = RenameCounts {
        total: total_functions,
        ..RenameCounts::default()
    };
    let mut module_bindings = RenameCounts::default();
    let mut identifiers = IdentifierCounts {
        skipped_by_skip_list: inp.skipped_by_skip_list as f64,
        ..IdentifierCounts::default()
    };
    for report in reports {
        match report.ty {
            ReportType::ModuleBinding => {
                module_bindings.total += report.total_identifiers as f64;
                bump(
                    &mut module_bindings,
                    report.strategy,
                    report.renamed_count as f64,
                );
                module_bindings.not_renamed +=
                    report.total_identifiers as f64 - report.renamed_count as f64;
            }
            ReportType::Function => {
                if report.renamed_count > 0 {
                    bump(&mut functions, report.strategy, 1.0);
                }
            }
        }
        identifiers.total += report.total_identifiers as f64;
        for (_, outcome) in report.outcomes.iter() {
            if outcome.is_renamed() {
                bump_ids(&mut identifiers, report.strategy);
            } else {
                identifiers.not_renamed += 1.0;
            }
        }
    }
    functions.not_renamed =
        (total_functions - functions.llm - functions.library_prefix - functions.fallback).max(0.0);
    functions.cached = (inp.prior_version_applied as f64).min(functions.not_renamed);
    functions.already_named =
        (inp.prior_version_already_named as f64).min(functions.not_renamed - functions.cached);
    let nothing = (inp.skip_reasons.zero_bindings
        + inp.skip_reasons.all_preserved
        + inp.library_no_minified) as f64;
    functions.nothing_to_rename =
        nothing.min(functions.not_renamed - functions.cached - functions.already_named);
    functions.failed = (functions.not_renamed
        - functions.cached
        - functions.already_named
        - functions.nothing_to_rename)
        .max(0.0);
    functions.close_match = inp.prior_version_close_match as f64;
    let mb_cached = inp.prior_version_bindings_applied as f64;
    if mb_cached > 0.0 {
        module_bindings.total += mb_cached;
        module_bindings.cached = mb_cached;
    }
    CoverageSummary {
        functions,
        module_bindings,
        identifiers,
        llm: Some(LlmCoverage {
            total_calls: inp.llm_calls as f64,
            retries: inp.llm_retries as f64,
            avg_response_time_ms: inp.avg_response_time_ms,
            total_tokens: None,
            input_tokens: None,
            output_tokens: None,
        }),
        elapsed_ms: Some(inp.elapsed_ms),
        minted_census: None,
    }
}

/// The census record (`summarizeCensus`'s return literal).
pub fn census_record(c: &MintedCensus) -> humanify_model::stats::MintedCensus {
    humanify_model::stats::MintedCensus {
        total: c.total as f64,
        decorated: Some(c.decorated as f64),
        total_bindings: Some(c.total_bindings as f64),
        free_references: Some(c.free_references.clone()),
        by_family: humanify_model::stats::MintedFamilies {
            class_expr_id: c.by_family[0] as f64,
            fn_expr_id: c.by_family[1] as f64,
            param: c.by_family[2] as f64,
            fn_decl: c.by_family[3] as f64,
            var_other: c.by_family[4] as f64,
        },
        derivable_expr_ids: c.derivable_expr_ids as f64,
        zero_ref_expr_ids: c.zero_ref_expr_ids as f64,
        names: Some(c.names.clone()),
        decorated_names: Some(c.decorated_names.clone()),
        provenance: None,
        single_letters: None,
        binding_provenance: None,
    }
}

/// The survivors' provenance split — the TREE-WALK half of the 2026-09-30
/// provenance meter, generalized from the single-letter special case
/// (finding #62's monitor, kept as the returned single-letter slice): of
/// the minted leftovers the census counts, which have a RECORDED decision
/// behind them — `model_chosen` (some tier APPLIED the surviving name:
/// the reports' `Renamed` outcomes and the trail's `final_name`s), or
/// `asked_kept` (asked about and kept: the ask-lane rows' names, the
/// reports' non-`Renamed` outcomes) — and which have no record anywhere
/// (`never_asked`, the real gap finding #64 named) or whose retry budget
/// died still-unrenamed (`exhausted`, supplied by the run).
///
/// This joins the run's records BY NAME because binding identity (the
/// declaration spans) does not survive the generate/reconcile/render
/// boundary: a record on ANY same-named binding classifies every
/// same-named survivor as decided, so `never_asked` here is a LOWER
/// BOUND. The exact, per-binding half is the sweep's own
/// `bindingProvenance` — the two blocks are both kept and the difference
/// is declared in their docs. This is a meter — no decision reads it (the
/// row in docs/responsibility.md says so).
pub fn survivor_provenance_split(
    census: &MintedCensus,
    reports: &[RenameReport],
    trail: &StrategyTrail,
    exhausted: &[String],
) -> (ProvenanceSplit, ProvenanceSplit) {
    let decided = crate::naming::passes::sweep::DecidedNames::of(trail, reports, exhausted);
    let class_of = |name: &str| -> Option<Class> {
        if decided.renamed_to.contains(name) {
            Some(Class::ModelChosen)
        } else if decided.exhausted.contains(name) {
            Some(Class::Exhausted)
        } else if decided.asked.contains(name) {
            Some(Class::AskedKept)
        } else {
            None
        }
    };
    let mut all = ClassCounts::default();
    let mut letters = ClassCounts::default();
    for name in census.names.iter() {
        all.total += 1;
        let class = class_of(name).unwrap_or(Class::NeverAsked);
        all.bump(class);
        if is_single_letter(name) {
            letters.total += 1;
            letters.bump(class);
        }
    }
    (all.into_split(), letters.into_split())
}

/// The split's classes, in the priority the join reads them.
#[derive(Clone, Copy)]
enum Class {
    ModelChosen,
    AskedKept,
    Exhausted,
    NeverAsked,
}

#[derive(Default)]
struct ClassCounts {
    total: usize,
    model_chosen: usize,
    asked_kept: usize,
    exhausted: usize,
    never_asked: usize,
}

impl ClassCounts {
    fn bump(&mut self, class: Class) {
        match class {
            Class::ModelChosen => self.model_chosen += 1,
            Class::AskedKept => self.asked_kept += 1,
            Class::Exhausted => self.exhausted += 1,
            Class::NeverAsked => self.never_asked += 1,
        }
    }
    fn into_split(self) -> ProvenanceSplit {
        ProvenanceSplit {
            total: self.total as f64,
            model_chosen: self.model_chosen as f64,
            asked_kept: self.asked_kept as f64,
            exhausted: self.exhausted as f64,
            never_asked: self.never_asked as f64,
        }
    }
}

/// The in-stage per-binding classification (`naming::passes::sweep`'s
/// `BindingProvenance`) as the stats record.
pub fn binding_provenance_record(
    p: &crate::naming::passes::sweep::BindingProvenance,
) -> StatsBindingProvenance {
    StatsBindingProvenance {
        join: if p.joined { "by-name" } else { "per-binding" }.to_string(),
        total: p.total as f64,
        renamed: p.renamed as f64,
        model_chosen: p.model_chosen as f64,
        asked_kept: p.asked_kept as f64,
        exhausted: p.exhausted as f64,
        never_asked: p.never_asked as f64,
    }
}

/// `n.toLocaleString("en-US")` for a non-negative integer.
pub fn locale_int(n: f64) -> String {
    let digits = format!("{}", n as u64);
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `fmt(n)`: locale digits right-aligned to 8.
fn fmt(n: f64) -> String {
    format!("{:>8}", locale_int(n))
}

fn pad_end(s: &str, width: usize) -> String {
    format!("{s:<width$}")
}

fn push_count_line(lines: &mut Vec<String>, label: &str, count: f64, total: f64, width: usize) {
    if count <= 0.0 {
        return;
    }
    let pct = if total > 0.0 {
        to_fixed(count / total * 100.0, 1)
    } else {
        "0.0".to_string()
    };
    lines.push(format!(
        "   {}{}  ({pct}%)",
        pad_end(label, width - 2),
        fmt(count)
    ));
}

fn format_section(label: &str, c: &RenameCounts, width: usize) -> Vec<String> {
    let mut lines = vec![format!(" {}{} total", pad_end(label, width), fmt(c.total))];
    push_count_line(&mut lines, "LLM:", c.llm, c.total, width);
    push_count_line(
        &mut lines,
        "Library prefix:",
        c.library_prefix,
        c.total,
        width,
    );
    push_count_line(&mut lines, "Fallback:", c.fallback, c.total, width);
    push_count_line(&mut lines, "Cached:", c.cached, c.total, width);
    push_count_line(&mut lines, "Close match:", c.close_match, c.total, width);
    push_count_line(
        &mut lines,
        "Already named:",
        c.already_named,
        c.total,
        width,
    );
    push_count_line(
        &mut lines,
        "Nothing to rename:",
        c.nothing_to_rename,
        c.total,
        width,
    );
    push_count_line(&mut lines, "Failed:", c.failed, c.total, width);
    if c.not_renamed > 0.0 && c.nothing_to_rename == 0.0 && c.failed == 0.0 {
        lines.push(format!(
            "   {}{}",
            pad_end("Not renamed:", width - 2),
            fmt(c.not_renamed)
        ));
    }
    lines
}

fn identifier_counts(c: &IdentifierCounts) -> RenameCounts {
    RenameCounts {
        total: c.total,
        llm: c.llm,
        library_prefix: c.library_prefix,
        fallback: c.fallback,
        not_renamed: c.not_renamed,
        nothing_to_rename: c.nothing_to_rename,
        cached: c.cached,
        close_match: c.close_match,
        already_named: c.already_named,
        failed: c.failed,
    }
}

fn format_minted_census(c: &humanify_model::stats::MintedCensus, width: usize) -> Vec<String> {
    let mut lines = vec![format!(
        " {}{} total",
        pad_end("Minted leftovers:", width),
        fmt(c.total)
    )];
    let free = c.free_references.clone().unwrap_or_default();
    if !free.is_empty() {
        let shown: Vec<&str> = free.iter().take(8).map(String::as_str).collect();
        lines.push(format!(
            " {}{} — no binding, renamer cannot reach: {}{}",
            pad_end("Unreachable (free refs):", width),
            fmt(free.len() as f64),
            shown.join(", "),
            if free.len() > 8 { ", …" } else { "" }
        ));
    }
    if c.total == 0.0 {
        return lines;
    }
    let f = &c.by_family;
    for (count, label) in [
        (f.class_expr_id, "class-expr id:"),
        (f.fn_expr_id, "fn-expr id:"),
        (f.param, "param:"),
        (f.fn_decl, "fn/class decl:"),
        (f.var_other, "var/other:"),
    ] {
        push_count_line(&mut lines, label, count, c.total, width);
    }
    if let Some(p) = &c.provenance
        && p.total > 0.0
    {
        lines.push(format!(
            " {}{} total",
            pad_end("Provenance:", width),
            fmt(p.total)
        ));
        push_count_line(&mut lines, "Model-chosen:", p.model_chosen, p.total, width);
        push_count_line(&mut lines, "Asked, kept:", p.asked_kept, p.total, width);
        push_count_line(
            &mut lines,
            "Retries exhausted:",
            p.exhausted,
            p.total,
            width,
        );
        push_count_line(
            &mut lines,
            "Never processed:",
            p.never_asked,
            p.total,
            width,
        );
    }
    if let Some(sl) = &c.single_letters
        && sl.total > 0.0
    {
        lines.push(format!(
            " {}{} total",
            pad_end("Single-letter:", width),
            fmt(sl.total)
        ));
        push_count_line(
            &mut lines,
            "Model-chosen:",
            sl.model_chosen,
            sl.total,
            width,
        );
        push_count_line(&mut lines, "Asked, kept:", sl.asked_kept, sl.total, width);
        push_count_line(
            &mut lines,
            "Retries exhausted:",
            sl.exhausted,
            sl.total,
            width,
        );
        push_count_line(
            &mut lines,
            "Never processed:",
            sl.never_asked,
            sl.total,
            width,
        );
    }
    if let Some(b) = &c.binding_provenance {
        lines.push(format!(
            " {}{} total ({} join)",
            pad_end("Binding provenance:", width),
            fmt(b.total),
            b.join
        ));
        push_count_line(&mut lines, "Renamed:", b.renamed, b.total, width);
        push_count_line(&mut lines, "Model-chosen:", b.model_chosen, b.total, width);
        push_count_line(&mut lines, "Asked, kept:", b.asked_kept, b.total, width);
        push_count_line(
            &mut lines,
            "Retries exhausted:",
            b.exhausted,
            b.total,
            width,
        );
        push_count_line(
            &mut lines,
            "Never processed:",
            b.never_asked,
            b.total,
            width,
        );
    }
    lines
}

/// `formatCoverageSummary`.
pub fn format_coverage_summary(s: &CoverageSummary) -> String {
    let sep = '\u{2500}';
    let width = 18;
    let mut lines = vec![format!(
        " {sep}{sep} Coverage Summary {}",
        sep.to_string().repeat(60)
    )];
    for (label, counts) in [
        ("Functions:", s.functions.clone()),
        ("Module bindings:", s.module_bindings.clone()),
        ("Identifiers:", identifier_counts(&s.identifiers)),
    ] {
        if counts.total > 0.0 {
            lines.extend(format_section(label, &counts, width));
        }
    }
    if s.identifiers.skipped_by_skip_list > 0.0 {
        lines.push(format!(
            "   {}{}",
            pad_end("Skipped (skip-list):", width - 2),
            fmt(s.identifiers.skipped_by_skip_list)
        ));
    }
    if let Some(c) = &s.minted_census {
        lines.extend(format_minted_census(c, width));
    }
    if let Some(llm) = &s.llm {
        let mut parts = vec![format!("{} calls", fmt(llm.total_calls).trim())];
        if llm.retries > 0.0 {
            parts.push(format!("{} retries", llm.retries));
        }
        parts.push(format!(
            "avg {}ms",
            humanify_model::js::number_to_string(llm.avg_response_time_ms)
        ));
        lines.push(format!(" LLM:              {}", parts.join(", ")));
    }
    if let Some(ms) = s.elapsed_ms
        && ms != 0.0
    {
        lines.push(format!(
            " Time:             {} elapsed",
            format_duration(ms)
        ));
    }
    lines.join("\n")
}
