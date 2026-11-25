//! Where the runners meet the diff labeller: one pair's card and the
//! label's summary.

use std::path::Path;

use super::Ctx;
use crate::eval::card::SCORER_VERSION;

/// The trees and inputs one card is made from.
pub struct PairInputs<'a> {
    pub fresh_tree: &'a Path,
    pub prior_tree: &'a Path,
    /// The folders holding each version's input files.
    pub prior_input: &'a Path,
    pub fresh_input: &'a Path,
    pub pair: &'a str,
}

/// Label one pair's diff: the card's text, with the report said.
pub fn analyze(ctx: &Ctx, p: &PairInputs) -> Result<String, String> {
    let trees = (p.prior_tree, p.fresh_tree);
    let mut labelled = crate::difflabel::label_trees(trees, (p.prior_input, p.fresh_input))?;
    for line in labelled.report.lines() {
        ctx.say(line);
    }
    if !labelled.adds_up {
        ctx.say(crate::difflabel::NOT_ADDING_UP);
    }
    if let Some(card) = labelled.card.as_object_mut() {
        card.insert("pair".into(), p.pair.into());
        card.insert("scorerVersion".into(), SCORER_VERSION.into());
    }
    Ok(format!("{:#}\n", labelled.card))
}

/// The label's summary, printed; the exit code.
pub fn summarize(ctx: &Ctx, label: &str) -> i32 {
    let dir = ctx.repo.join(crate::paths::EVAL_RESULTS).join(label);
    let summary = crate::committed::eval::self_hop_reference(&ctx.repo)
        .and_then(|r| crate::eval::summarize::summarize(&dir, label, &r.value));
    match summary {
        Ok(text) => {
            for line in text.lines() {
                ctx.say(line);
            }
            0
        }
        Err(e) => {
            ctx.say(format!("SUMMARY FAILED: {e}"));
            1
        }
    }
}
