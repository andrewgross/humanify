//! A BARREL's name from what it loads (finding #94, 2026-10-10).
//!
//! A barrel is a module that declares nothing: its setup code only loads
//! other modules (an `index` file re-exporting its folder). The prompt
//! has nothing of its own to show, and left to the naming waves it drew
//! filler (`setupModulesOnceVar`, `bootstrapModuleVal`, `moduleGamma`) —
//! a name its FILE then took, and kept forever once inherited. After the
//! module step its loaded modules carry purpose names, so the barrel is
//! named from THEM, by rule:
//!
//! - the words its loaded modules' names share — the longest phrase (up
//!   to three words) that a strict majority of them contain, the most
//!   widely shared first (`theme-modal`, `theme-picker`, `theme-store` →
//!   `theme`; `event-stream-plugin`, `event-stream-header-middleware`,
//!   `event-stream-payload-middleware` → `event-stream`);
//! - a single word that ENDS every name that holds it names a kind, and
//!   the barrel groups things of that kind: plural (`confirm-prompt`,
//!   `input-prompt`, `select-prompt` → `prompts`);
//! - one meaningful loaded module: its name (`error-classes`).
//!
//! The candidates are that phrase, then `<phrase>-index`, then
//! `<phrase>-exports` (the phrase is often its loaded module's own name:
//! `error-classes-index`; a second barrel over it `error-classes-exports`). Loaded names
//! that say nothing — filler words only ([`is_filler_module_name`]),
//! numbered — are not evidence, and a loaded barrel's own suffix is not
//! a word of the phrase (a chain of barrels names the same thing). No
//! shared phrase: the model is asked, with the loaded names as evidence.
//!
//! Measured on the run-2 eval trees (docs/rust-port/16-findings-queue.md
//! #94): the rule names ~45% of the barrels of a fresh tree; on its
//! barrels the model, given the same evidence, echoed a loaded module's
//! name (a collision) or appended `-loader`.

use std::collections::HashMap;

use crate::place::stems::{accept_proposed_name, is_filler_module_name, is_filler_module_word};

/// The longest shared phrase considered, in words.
const MAX_PHRASE_WORDS: usize = 3;

/// The suffixes a barrel's name takes when its phrase is taken, in order
/// (a chain of barrels re-exporting one module takes the second).
pub const BARREL_SUFFIXES: [&str; 2] = ["index", "exports"];

/// A barrel's name without its barrel suffix (`otlp-exporter-index` →
/// `otlp-exporter`): a loaded barrel names the same thing it loads.
pub fn base_name(name: &str) -> &str {
    BARREL_SUFFIXES
        .iter()
        .find_map(|s| {
            name.strip_suffix(s)
                .and_then(|rest| rest.strip_suffix('-'))
                .filter(|rest| !rest.is_empty())
        })
        .unwrap_or(name)
}

/// Words that never start or end a phrase.
const STOP_WORDS: &[&str] = &[
    "to", "and", "or", "of", "for", "the", "a", "an", "in", "on", "with", "by", "from", "is",
];

fn is_stop(word: &str) -> bool {
    STOP_WORDS.contains(&word)
}

/// A loaded module's name as rule evidence: its words, a loaded barrel's
/// own `-index` dropped; None for a name that says nothing (filler, a
/// no-op or a counter — what [`accept_proposed_name`] refuses).
pub fn evidence_words(name: &str) -> Option<Vec<String>> {
    accept_proposed_name(name)?;
    let words: Vec<String> = base_name(&name.to_ascii_lowercase())
        .split('-')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let joined = words.join("-");
    let meaningful = words
        .iter()
        .any(|w| !is_stop(w) && !is_filler_module_word(w));
    (meaningful && !is_filler_module_name(&joined)).then_some(words)
}

/// The phrase a barrel's loaded modules share (kebab), or None. `loaded`
/// are the loaded modules' names in load order (duplicates and names
/// that say nothing are dropped here).
pub fn shared_phrase(loaded: &[String]) -> Option<String> {
    let mut names: Vec<Vec<String>> = Vec::new();
    for words in loaded.iter().filter_map(|n| evidence_words(n)) {
        if !names.contains(&words) {
            names.push(words);
        }
    }
    match names.len() {
        0 => None,
        1 => Some(names[0].join("-")),
        _ => majority_phrase(&names).map(|p| p.join("-")),
    }
}

/// Per phrase: how many names hold it, and where it first appears.
fn phrase_support(names: &[Vec<String>]) -> HashMap<Vec<String>, (usize, (usize, usize))> {
    let mut support: HashMap<Vec<String>, (usize, (usize, usize))> = HashMap::new();
    for (i, words) in names.iter().enumerate() {
        let mut seen: Vec<&[String]> = Vec::new();
        for n in 1..=MAX_PHRASE_WORDS.min(words.len()) {
            for k in 0..=words.len() - n {
                let phrase = &words[k..k + n];
                if seen.contains(&phrase) || !is_phrase(phrase) {
                    continue;
                }
                seen.push(phrase);
                let entry = support.entry(phrase.to_vec()).or_insert((0, (i, k)));
                entry.0 += 1;
            }
        }
    }
    support
}

/// A phrase may name: no stop word at either end, one real word in it.
fn is_phrase(phrase: &[String]) -> bool {
    let (Some(first), Some(last)) = (phrase.first(), phrase.last()) else {
        return false;
    };
    !is_stop(first) && !is_stop(last) && !is_filler_module_name(&phrase.join("-"))
}

/// The phrase a strict majority (and at least two) of `names` hold: the
/// most widely held, then the longest, then the first to appear; a single
/// word ending every name that holds it is pluralized (a kind).
fn majority_phrase(names: &[Vec<String>]) -> Option<Vec<String>> {
    let support = phrase_support(names);
    let best = support
        .iter()
        .filter(|(_, (count, _))| *count >= 2 && 2 * count > names.len())
        .max_by(|(pa, (ca, fa)), (pb, (cb, fb))| {
            ca.cmp(cb).then(pa.len().cmp(&pb.len())).then(fb.cmp(fa))
        })
        .map(|(p, _)| p.clone())?;
    if let [word] = best.as_slice() {
        let kind = names
            .iter()
            .filter(|ws| ws.contains(word))
            .all(|ws| ws.last() == Some(word));
        if kind && !word.ends_with('s') {
            return Some(vec![format!("{word}s")]);
        }
    }
    Some(best)
}

/// The names the rule offers a barrel, in order: the phrase, then the
/// phrase with each barrel suffix (`error-classes-index`,
/// `error-classes-exports`).
pub fn rule_candidates(phrase: &str) -> Vec<String> {
    std::iter::once(phrase.to_string())
        .chain(BARREL_SUFFIXES.iter().map(|s| format!("{phrase}-{s}")))
        .collect()
}

#[cfg(test)]
mod barrels_test;
