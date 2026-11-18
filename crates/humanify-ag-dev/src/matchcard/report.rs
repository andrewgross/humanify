//! The `match score` verb's report and card.

use std::path::Path;

use humanify_ag_core::matching::decisions::MATCH_DECISIONS_PATH;
use humanify_ag_core::provenance::PROVENANCE_PATH;

use serde::Deserialize;
use serde_json::{Value, json};

use super::join::{NAMED_TEXT, locate, locate_init_wrappers};
use super::score::{Card, Matcher, score};
use crate::difflabel::{Tree, Truth, read_input};

/// A run's `.humanify/match-decisions.json`.
#[derive(Deserialize)]
pub struct Decisions {
    fresh: Vec<u32>,
    prior: Vec<u32>,
    exact: Vec<(usize, usize)>,
    close: Vec<(usize, usize)>,
    ambiguous: Vec<usize>,
    unmatched: Vec<usize>,
}

pub struct Scored {
    pub report: String,
    pub card: Value,
}

fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

fn rate(name: &str, part: u64, whole: u64) -> String {
    format!(
        "{name} {:.1}% ({part} of {whole})",
        100.0 * share(part, whole)
    )
}

fn card_json(card: &Card) -> Value {
    json!({
        "unchanged": card.unchanged, "correct": card.correct, "wrong": card.wrong,
        "missedClose": card.missed_close, "missedCloseOther": card.missed_close_other,
        "missedAmbiguous": card.missed_ambiguous, "missedUnmatched": card.missed_unmatched,
        "notJudged": card.not_judged,
        "otherMatched": card.other_matched, "otherUnmatched": card.other_unmatched,
        "precision": share(card.correct, card.correct + card.wrong),
        "recall": share(card.correct, card.unchanged),
    })
}

/// The linker's init wrappers, each paired through its input file's pair.
fn init_wrappers(truth: &Truth, from: &Tree, to: &Tree, decisions: &Decisions) -> (Card, u64) {
    let of_to = |name: &str| truth.paired_file(true, name).and(to.input_index(name));
    let of_from = |name: &str| to.input_index(truth.paired_file(false, name)?);
    let fresh = locate_init_wrappers(to, &decisions.fresh, &of_to);
    let prior = locate_init_wrappers(from, &decisions.prior, &of_from);
    let unpaired = wrapper_count(to, &decisions.fresh)
        - fresh.iter().filter(|f| f.pair.is_some()).count() as u64;
    (score(&fresh, &prior, &matcher_of(decisions)), unpaired)
}

fn wrapper_count(tree: &Tree, lines: &[u32]) -> u64 {
    let all = locate_init_wrappers(tree, lines, &|_| Some(0));
    all.iter().filter(|f| f.pair.is_some()).count() as u64
}

fn matcher_of(decisions: &Decisions) -> Matcher {
    Matcher {
        exact: decisions.exact.clone(),
        close: decisions.close.clone(),
        ambiguous: decisions.ambiguous.iter().copied().collect(),
    }
}

fn block(title: &str, card: &Card) -> String {
    let missed =
        card.missed_close + card.missed_close_other + card.missed_ambiguous + card.missed_unmatched;
    let rows = [
        ("matched to their partner", card.correct),
        ("matched to another function", card.wrong),
        ("not matched", missed),
        ("  close-matched to their partner", card.missed_close),
        ("  close-matched elsewhere", card.missed_close_other),
        ("  partner left ambiguous", card.missed_ambiguous),
        ("  partner unmatched", card.missed_unmatched),
    ];
    let mut out = format!(
        "{title}: {}\n  {}; {}\n",
        card.unchanged,
        rate("precision", card.correct, card.correct + card.wrong),
        rate("recall", card.correct, card.unchanged),
    );
    for (name, count) in rows {
        out.push_str(&format!(
            "  {name:<34}{count:>9}{:>7.1}%\n",
            100.0 * share(count, card.unchanged)
        ));
    }
    out
}

pub fn score_decisions(truth: &Truth, from: &Tree, to: &Tree, decisions: &Decisions) -> Scored {
    let prior = locate(from, &truth.from, &decisions.prior).functions;
    let located = locate(to, &truth.to, &decisions.fresh);
    let (fresh, standings) = (located.functions, located.standings);
    let card = score(&fresh, &prior, &matcher_of(decisions));
    let (wrappers, unpaired_wrappers) = init_wrappers(truth, from, to, decisions);
    let mut json = card_json(&card);
    json["functions"] = json!({
        "fresh": decisions.fresh.len(), "prior": decisions.prior.len(),
        "exact": decisions.exact.len(), "close": decisions.close.len(),
        "ambiguous": decisions.ambiguous.len(), "unmatched": decisions.unmatched.len(),
    });
    json["freshStandings"] = json!(standings);
    json["freshByOwnByte"] = json!(located.by_own_byte);
    json["initWrappers"] = card_json(&wrappers);
    json["initWrappers"]["inputFileUnpaired"] = json!(unpaired_wrappers);
    let mut report = block("functions in unchanged input with a known partner", &card);
    report.push_str(&format!(
        "in unchanged input, function counts differ between the versions (not judged): {}\n\
         not in unchanged input: {} matched, {} not matched\n\n",
        card.not_judged, card.other_matched, card.other_unmatched,
    ));
    report.push_str(&block(
        "linker init wrappers whose input file is paired, one wrapper on each side",
        &wrappers,
    ));
    report.push_str(&format!(
        "init wrappers not judged: {} with another number of wrappers in the paired file, {} in an unpaired input file\n\n\
         the matcher saw {} new and {} prior functions: {} exact pairs, {} close pairs, {} ambiguous, {} unmatched\n",
        wrappers.not_judged,
        unpaired_wrappers,
        decisions.fresh.len(),
        decisions.prior.len(),
        decisions.exact.len(),
        decisions.close.len(),
        decisions.ambiguous.len(),
        decisions.unmatched.len(),
    ));
    report.push_str("where the new functions sit in the input:");
    for (name, count) in &standings {
        report.push_str(&format!(" {name} {count};"));
    }
    report.push_str(&format!(
        "\nnew functions located by their own first byte (the rest by their statement's): {}\n",
        located.by_own_byte
    ));
    Scored { report, card: json }
}

fn read_text(tree: &Path, file: &str) -> Result<String, String> {
    std::fs::read_to_string(tree.join(file)).map_err(|e| format!("{}/{file}: {e}", tree.display()))
}

fn named_text_only(tree: &Path) -> Result<Tree, String> {
    let text = read_text(tree, PROVENANCE_PATH)?;
    let named = std::fs::read(tree.join(NAMED_TEXT))
        .map_err(|e| format!("{}/{NAMED_TEXT}: {e}", tree.display()))?;
    Tree::from_parts([(NAMED_TEXT.to_string(), named)].into(), Some(&text))
}

fn decisions_of(tree: &Path) -> Result<Decisions, String> {
    serde_json::from_str(&read_text(tree, MATCH_DECISIONS_PATH)?)
        .map_err(|e| format!("{}/{MATCH_DECISIONS_PATH}: {e}", tree.display()))
}

/// The `match score` verb: the report text.
pub fn run(
    trees: (&Path, &Path),
    inputs: (&Path, &Path),
    out: Option<&Path>,
) -> Result<String, String> {
    let decisions = decisions_of(trees.1)?;
    if let Ok(earlier) = decisions_of(trees.0)
        && earlier.fresh.len() != decisions.prior.len()
    {
        return Err(format!(
            "{} is not the prior {} was matched against: it has {} functions, the prior had {}",
            trees.0.display(),
            trees.1.display(),
            earlier.fresh.len(),
            decisions.prior.len()
        ));
    }
    let (from, to) = (named_text_only(trees.0)?, named_text_only(trees.1)?);
    let truth = Truth::compare(read_input(inputs.0)?, read_input(inputs.1)?);
    let scored = score_decisions(&truth, &from, &to, &decisions);
    if let Some(out) = out {
        std::fs::write(out, format!("{:#}\n", scored.card))
            .map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(format!(
        "match score: {} -> {}\n{}",
        trees.0.display(),
        trees.1.display(),
        scored.report
    ))
}

#[cfg(test)]
#[path = "report_test.rs"]
mod report_test;
