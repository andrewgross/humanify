//! The `match score` verb's report and card.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use humanify_ag_core::layout_record::{FunctionId, Layout, Match, checksum};
use serde_json::{Value, json};

use super::join::{functions, is_init_wrapper, locate, locate_init_wrappers};
use super::score::{Card, Matcher, score};
use crate::difflabel::{Truth, read_input};
use crate::layoutcheck::read_layout;

/// What the matcher decided, by position in the two layouts' functions.
struct Decisions {
    fresh: usize,
    prior: usize,
    matcher: Matcher,
}

/// The matcher's pairs as the new layout records them against the
/// previous one.
fn decisions_of(from: &Layout, to: &Layout) -> Result<Decisions, String> {
    let prior = functions(from);
    let at: HashMap<(u32, &FunctionId), usize> = prior
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.input, f.id), i))
        .collect();
    let position = |partner: &Option<(u32, FunctionId)>| {
        let (input, id) = partner
            .as_ref()
            .ok_or("the new tree was matched against a tree with no layout record")?;
        at.get(&(*input, id))
            .copied()
            .ok_or_else(|| format!("the previous layout lists no function {id:?} of input {input}"))
    };
    let fresh = functions(to);
    let mut matcher = Matcher {
        exact: Vec::new(),
        close: Vec::new(),
        ambiguous: BTreeSet::new(),
    };
    for (i, function) in fresh.iter().enumerate() {
        match function.matched {
            Match::New => {}
            Match::Exact(partner) => matcher.exact.push((position(partner)?, i)),
            Match::Close(partner) => matcher.close.push((position(partner)?, i)),
        }
    }
    for partner in &to.prior_ambiguous {
        matcher.ambiguous.insert(position(&Some(partner.clone()))?);
    }
    Ok(Decisions {
        fresh: fresh.len(),
        prior: prior.len(),
        matcher,
    })
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
fn init_wrappers(truth: &Truth, from: &Layout, to: &Layout, matcher: &Matcher) -> (Card, u64) {
    let index = |name: &str| to.inputs.iter().position(|n| n == name).map(|i| i as u32);
    let of_to = |name: &str| truth.paired_file(true, name).and(index(name));
    let of_from = |name: &str| index(truth.paired_file(false, name)?);
    let fresh = locate_init_wrappers(to, &of_to);
    let prior = locate_init_wrappers(from, &of_from);
    let wrappers = functions(to).iter().filter(|f| is_init_wrapper(f)).count();
    let unpaired = wrappers - fresh.iter().filter(|f| f.pair.is_some()).count();
    (score(&fresh, &prior, matcher), unpaired as u64)
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

pub fn score_layouts(truth: &Truth, from: &Layout, to: &Layout) -> Result<Scored, String> {
    let decisions = decisions_of(from, to)?;
    let matcher = &decisions.matcher;
    let prior = locate(from, &truth.from).functions;
    let located = locate(to, &truth.to);
    let (fresh, standings) = (located.functions, located.standings);
    let card = score(&fresh, &prior, matcher);
    let (wrappers, unpaired_wrappers) = init_wrappers(truth, from, to, matcher);
    let paired: BTreeSet<usize> = (matcher.exact.iter().chain(&matcher.close))
        .map(|pair| pair.0)
        .collect();
    let unmatched = decisions.prior - paired.union(&matcher.ambiguous).count();
    let mut json = card_json(&card);
    json["functions"] = json!({
        "fresh": decisions.fresh, "prior": decisions.prior,
        "exact": matcher.exact.len(), "close": matcher.close.len(),
        "ambiguous": matcher.ambiguous.len(), "unmatched": unmatched,
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
        decisions.fresh,
        decisions.prior,
        matcher.exact.len(),
        matcher.close.len(),
        matcher.ambiguous.len(),
        unmatched,
    ));
    report.push_str("where the new functions sit in the input:");
    for (name, count) in &standings {
        report.push_str(&format!(" {name} {count};"));
    }
    report.push_str(&format!(
        "\nnew functions with an input byte (the linker wrote the rest): {}\n",
        located.by_own_byte
    ));
    Ok(Scored { report, card: json })
}

/// The `match score` verb: the report text.
pub fn run(
    trees: (&Path, &Path),
    inputs: (&Path, &Path),
    out: Option<&Path>,
) -> Result<String, String> {
    let ((from_text, from), (_, to)) = (read_layout(trees.0)?, read_layout(trees.1)?);
    if to.prior_layout.as_deref() != Some(checksum(&from_text).as_str()) {
        return Err(format!(
            "{} is not the previous tree {} was matched against: the layout checksums differ",
            trees.0.display(),
            trees.1.display(),
        ));
    }
    let truth = Truth::compare(read_input(inputs.0)?, read_input(inputs.1)?);
    let scored = score_layouts(&truth, &from, &to)?;
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
