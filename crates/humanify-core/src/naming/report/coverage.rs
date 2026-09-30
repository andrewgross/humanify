//! `buildCoverageSummary` + `formatCoverageSummary` (src/rename/coverage.ts).
//! The record is the stats model's [`CoverageSummary`] (its key order is
//! the TS construction order); the printed block is what the CLI shows.

use humanify_model::js::{format_duration, to_fixed};
use humanify_model::stats::{
    CoverageSummary, IdentifierCounts, LlmCoverage, RenameCounts, SingleLetterSplit,
};

use super::{RenameReport, ReportStrategy, ReportType, SkipReasons, Status};
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
        single_letters: None,
    }
}

/// The single-letter survivors' provenance split (Andrew's 2026-09-30
/// decision): of the minted leftovers the census counts, which single
/// letters have a RECORDED decision behind them — `model_chosen` (some
/// tier APPLIED the letter as a name: the rename reports' `Renamed`
/// outcomes and the trail's `final_name`, the carried rule's population)
/// or `asked_kept` (asked about and kept: the reports' non-`Renamed`
/// outcomes under that name, the trail's rows under its old name) — and
/// which have NO record anywhere (`never_asked`, the real gap finding #62
/// was blind to and #64 named).
///
/// The outcome records ARE the provenance — this joins them by NAME
/// (ask-time name for the reports, old/final name for the trail), because
/// identity systems (spans) do not survive the generate/reconcile/render
/// boundary. Consequence, declared: a letter with a record on ANY
/// same-named binding classifies every same-named survivor as decided, so
/// `never_asked` is a LOWER BOUND on the true never-processed population.
/// It is a meter — no decision reads it (the row in
/// docs/responsibility.md says so).
pub fn single_letter_split(
    census: &MintedCensus,
    reports: &[RenameReport],
    trail: &StrategyTrail,
) -> SingleLetterSplit {
    let survivors: Vec<&String> = census
        .names
        .iter()
        .filter(|n| is_single_letter(n))
        .collect();
    let total = survivors.len();
    if total == 0 {
        return SingleLetterSplit {
            total: 0.0,
            model_chosen: 0.0,
            asked_kept: 0.0,
            never_asked: 0.0,
        };
    }
    let letters: std::collections::HashSet<&str> = survivors.iter().map(|n| n.as_str()).collect();
    let mut chosen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut asked: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in trail.entries() {
        if let Some(final_name) = entry.final_name.as_ref()
            && letters.contains(final_name.as_str())
        {
            chosen.insert(final_name.clone());
        }
        if letters.contains(entry.old_name.as_str()) {
            asked.insert(entry.old_name.clone());
        }
    }
    for report in reports {
        for (name, outcome) in report.outcomes.iter() {
            if !letters.contains(name.as_str()) {
                continue;
            }
            if let Status::Renamed { new_name, .. } = &outcome.status
                && letters.contains(new_name.as_str())
            {
                chosen.insert(new_name.clone());
            } else {
                asked.insert(name.clone());
            }
        }
    }
    let model_chosen = survivors
        .iter()
        .filter(|n| chosen.contains(n.as_str()))
        .count();
    let asked_kept = survivors
        .iter()
        .filter(|n| !chosen.contains(n.as_str()) && asked.contains(n.as_str()))
        .count();
    SingleLetterSplit {
        total: total as f64,
        model_chosen: model_chosen as f64,
        asked_kept: asked_kept as f64,
        never_asked: (total - model_chosen - asked_kept) as f64,
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
            "Never processed:",
            sl.never_asked,
            sl.total,
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
