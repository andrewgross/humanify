//! The sweep reads its answers through the ONE answer-key owner
//! (`naming::answer_keys`, finding #85): a key the model mangled
//! (`y$_` answered as `y$` — the 2.1.85 eval's six leftovers) lands on the
//! one asked identifier it can belong to, recorded on the trail; a key
//! that could belong to two asked identifiers lands on neither, and the
//! targets are RE-ASKED with the bad key disclosed — never a silent
//! "declined".

use std::cell::RefCell;

use humanify_model::llm::{BatchRenameResponse, LlmCall, LlmError, NameProvider, Renames};
use oxc_allocator::Allocator;

use super::sweep_minted_names;
use crate::ingest::Ingest;
use crate::modules::soundness::collect_eval_with_taint;
use crate::naming::waves::render::render_program;
use crate::rename::eligibility::{Eligibility, NeverRename};
use crate::rename::floor::MinifiedStems;
use crate::rename::name_profile::NameProfile;
use crate::rename::validated::RenameState;
use crate::trail::{Anchor, Outcome, StrategyTrail};

/// The stub's word for an id: `named` + the id with `$` -> D, `_` -> U
/// (distinct per id, so two answers never collide).
fn word_for(id: &str) -> String {
    let tail: String = id
        .chars()
        .map(|ch| match ch {
            '$' => 'D',
            '_' => 'U',
            ch => ch,
        })
        .collect();
    format!("named{tail}")
}

/// A stub that answers every asked id under the key `key_of(round, id)`,
/// and keeps every call's identifiers and user prompt.
struct Keyed<F: Fn(usize, &str) -> String> {
    key_of: F,
    calls: RefCell<Vec<(Vec<String>, String)>>,
}

impl<F: Fn(usize, &str) -> String> NameProvider for Keyed<F> {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        calls
            .into_iter()
            .map(|c| {
                let round = self.calls.borrow().len();
                self.calls
                    .borrow_mut()
                    .push((c.request.identifiers.clone(), c.user_prompt.clone()));
                let entries: Vec<(String, Option<String>)> = c
                    .request
                    .identifiers
                    .iter()
                    .map(|id| ((self.key_of)(round, id), Some(word_for(id))))
                    .collect();
                Ok(BatchRenameResponse {
                    renames: Renames::from_entries(entries),
                    finish_reason: None,
                    usage: None,
                })
            })
            .collect()
    }
}

/// Run the sweep over `text` and hand the rendered code and the state to
/// `check`.
fn sweep(text: &str, provider: &impl NameProvider, check: impl FnOnce(&str, &RenameState)) {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let eligible = Eligibility::new(NeverRename::UNIVERSAL);
    let params = humanify_model::llm::CacheKeyParams::default();
    let mut state = RenameState::with_trail(
        semantic,
        Anchor::Fresh,
        StrategyTrail::enabled(),
        NameProfile::Bun,
    );
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());
    sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        provider,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        2,
        None,
        &MinifiedStems::of_program(NameProfile::Bun, semantic),
    );
    check(&render_program(semantic, &state), &state);
}

/// The eval's case: asked `y$_`, the model keyed its answer `y$`. On main
/// the sweep looked `y$_` up exactly, found nothing, recorded
/// "llm-declined" and never asked again.
#[test]
fn a_dollar_mangled_key_lands_on_its_one_asked_identifier() {
    let provider = Keyed {
        key_of: |_, id: &str| {
            if id == "y$_" {
                "y$".to_string()
            } else {
                id.to_string()
            }
        },
        calls: RefCell::new(Vec::new()),
    };
    sweep(
        "function f() {\n  var y$_ = new Map();\n  return y$_;\n}",
        &provider,
        |code, state| {
            assert!(code.contains("var namedyDU = new Map();"), "{code}");
            let row = state
                .trail()
                .entries()
                .iter()
                .find(|e| e.old_name == "y$_")
                .expect("the target is on the trail");
            let last = row.attempts.last().expect("an attempt");
            assert_eq!(last.outcome, Outcome::Applied);
            assert_eq!(
                last.answer_key.as_deref(),
                Some("y$"),
                "the tolerant match is recorded with the model's own key"
            );
        },
    );
    assert_eq!(provider.calls.borrow().len(), 1, "no re-ask needed");
}

/// `a$_` and `a_$` both read as `a$` once the `$`/`_` run is normalised:
/// the key `a$` could be either, so it lands on NEITHER — both targets
/// are re-asked, the re-ask names the bad key, and the exact-keyed second
/// answer applies.
#[test]
fn an_ambiguous_key_lands_nowhere_and_both_targets_are_reasked() {
    let provider = Keyed {
        key_of: |round, id: &str| {
            if round == 0 && (id == "a$_" || id == "a_$") {
                "a$".to_string()
            } else {
                id.to_string()
            }
        },
        calls: RefCell::new(Vec::new()),
    };
    let mut code = String::new();
    sweep(
        "function f() {\n  var a$_ = one();\n  var a_$ = two();\n  return a$_ + a_$;\n}",
        &provider,
        |c, _| code = c.to_string(),
    );
    let calls = provider.calls.borrow();
    assert_eq!(calls.len(), 2, "the first ask, then one re-ask: {calls:?}");
    let (ids, prompt) = &calls[1];
    assert_eq!(
        ids,
        &["a$_".to_string(), "a_$".to_string()],
        "only the two unmatched targets re-ask"
    );
    assert!(
        prompt.contains("These identifiers were MISSING from your response: a$_, a_$"),
        "{prompt}"
    );
    assert!(
        prompt.contains("Your response used keys that are not listed identifiers: \"a$\""),
        "the bad key is disclosed: {prompt}"
    );
    assert!(code.contains("var namedaDU = one();"), "{code}");
    assert!(code.contains("var namedaUD = two();"), "{code}");
}
