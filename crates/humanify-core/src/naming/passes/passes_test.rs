//! Native tests for the naming passes' re-ask policy. The WP4.4/4.5
//! TS-harvested corpus (test/parity/wp445-fixtures.json — every top-level
//! call the passes' own TS unit tests make, replayed against the Rust
//! port) was retired 2026-09-28 with the other TS-capture replays; the
//! re-ask policy it exercised end to end through the sweep is pinned here
//! natively, and the reconcile/permutation units keep their own module
//! tests.

use humanify_model::llm::{BatchRenameResponse, LlmCall, LlmError, NameProvider, Renames};
use oxc_allocator::Allocator;

use super::sweep::{DecidedNames, collect_sweep_targets, run_deferred_sweep, sweep_minted_names};
use crate::ingest::Ingest;
use crate::modules::soundness::collect_eval_with_taint;
use crate::naming::waves::render::render_program;
use crate::rename::eligibility::Eligibility;
use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::BScopeId;
use crate::trail::Anchor;

/// The scope a name is registered in (a helper for these tests' setup).
fn scope_holding(state: &RenameState, name: &str) -> BScopeId {
    (0..state.view().scopes.len())
        .map(|i| BScopeId(i as u32))
        .find(|s| state.binding_in(*s, name).is_some())
        .unwrap_or_else(|| panic!("no scope holds {name:?}"))
}

/// The coverage-sweep collision retry (2026-09-28 fix): a sweep suggestion
/// rejected for a name-collision class (`target-in-scope` here — the
/// sibling `used` holds the name) gets ONE re-ask that DISCLOSES the
/// previous suggestion, and the re-asked suggestion applies through the
/// same validation. Before the fix this exact shape of sweep recorded
/// the drop — the case the retired wp445 corpus (kind "sweep") carried:
/// `Kq_` kept its minted name forever.
#[test]
fn a_sweep_collision_gets_one_disclosed_reask() {
    let text = "function f() {\n  var used = one();\n  var Kq_ = two();\n  return used + Kq_;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let params = humanify_model::llm::CacheKeyParams::default();

    // First ask: decline `f`, suggest the sibling's name for `Kq_` (the
    // collision). Re-ask: a fresh name — and the re-ask's prompt must
    // disclose the collision.
    struct CollisionSweep {
        asks: std::cell::Cell<usize>,
    }
    impl NameProvider for CollisionSweep {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            calls
                .into_iter()
                .map(|c| {
                    let n = self.asks.get();
                    self.asks.set(n + 1);
                    if n > 0 {
                        assert_eq!(c.request.identifiers, ["Kq_"], "only the rejected id re-asks");
                        assert!(
                            c.user_prompt.contains(
                                "\"Kq_\" was suggested as \"used\" but that conflicts with an existing name"
                            ),
                            "the re-ask discloses the collision: {}",
                            c.user_prompt
                        );
                        assert!(
                            c.user_prompt.contains("DO NOT suggest these names: used"),
                            "the re-ask blocklists the rejected name: {}",
                            c.user_prompt
                        );
                    }
                    let suggestion = if n == 0 { "used" } else { "usedRunner" };
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(vec![(
                            "Kq_".to_string(),
                            Some(suggestion.to_string()),
                        )]),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }

    let provider = CollisionSweep {
        asks: std::cell::Cell::new(0),
    };
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());
    let r = sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        &provider,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        2,
        None,
        &crate::rename::floor::MinifiedStems::of_program(semantic),
    );
    assert_eq!(provider.asks.get(), 2, "exactly one re-ask (it applied)");
    assert_eq!(r.reasked, 1, "the retry is recorded");
    assert_eq!(r.named, 1, "the re-asked suggestion applied");
    // 2026-09-30 provenance targeting: the never-asked descriptive `used`
    // is a target too (the sweep no longer gates on the name's shape), and
    // the provider declines it — so `f` and `used` both stay skipped.
    assert_eq!(r.skipped, 2);
    // The ask trace (`--dump-asks`): the first round carries no cause; the
    // re-ask records the reask class and the applier's rejection code.
    assert_eq!(r.dispatches.len(), 2, "both asks recorded");
    assert_eq!(
        r.dispatches[0].ask.cause, None,
        "the first round is causeless"
    );
    assert_eq!(
        r.dispatches[1].ask.cause,
        Some(crate::naming::ask_trace::RetryCause::NameTaken)
    );
    assert_eq!(
        r.dispatches[1].ask.detail.as_deref(),
        Some("target-in-scope"),
        "the seeding rejection's code is recorded"
    );
    assert_eq!(r.dispatches[1].request.is_retry, Some(true));
    let code = render_program(semantic, &state);
    assert!(
        code.contains("var usedRunner = two();"),
        "the retried name landed: {code}"
    );
}

/// Bounded: answer the SAME colliding name on every ask and the sweep
/// gives up after the TWO re-asks of the default budget — never a loop,
/// and the give-up is recorded.
#[test]
fn a_stubborn_sweep_gives_up_after_the_default_two_reasks() {
    let text = "function f() {\n  var used = one();\n  var Kq_ = two();\n  return used + Kq_;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let params = humanify_model::llm::CacheKeyParams::default();
    struct Stubborn {
        asks: std::cell::Cell<usize>,
    }
    impl NameProvider for Stubborn {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            self.asks.set(self.asks.get() + calls.len());
            calls
                .into_iter()
                .map(|_| {
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(vec![(
                            "Kq_".to_string(),
                            Some("used".to_string()),
                        )]),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }
    let stubborn = Stubborn {
        asks: std::cell::Cell::new(0),
    };
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());
    let r = sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        &stubborn,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        2,
        None,
        &crate::rename::floor::MinifiedStems::of_program(semantic),
    );
    assert_eq!(
        stubborn.asks.get(),
        3,
        "the initial ask plus the two re-asks of the default budget"
    );
    assert_eq!(r.reasked, 2, "one re-ask target per round, two rounds");
    assert_eq!(r.named, 0);
    assert_eq!(r.reask_dropped, 1, "the give-up is recorded");
    assert_eq!(render_program(semantic, &state), text, "nothing applied");
}

/// The accumulation pin: a sweep target whose re-ask ALSO collides gets
/// the second re-ask of the default budget, and that re-ask's prompt
/// discloses EVERY prior suggestion with its rejection reason — both
/// failed names, both conflict lines, both in the do-not-suggest block.
#[test]
fn the_second_sweep_reask_discloses_every_prior_suggestion_and_is_bounded() {
    let text = "function f() {\n  var used = one();\n  var Kq_ = two();\n  return used + Kq_;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let params = humanify_model::llm::CacheKeyParams::default();
    // First ask: the sibling's name (a target-in-scope collision). Re-ask
    // 1: `console` — a global builtin, rejected as invalid-target (a
    // DIFFERENT reaskable class, so the second re-ask's disclosure must
    // carry both reasons). Re-ask 2: the sibling's name again; the budget
    // is spent and the target gives up.
    struct TwoCollisions {
        asks: std::cell::Cell<usize>,
    }
    impl NameProvider for TwoCollisions {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            self.asks.set(self.asks.get() + calls.len());
            calls
                .into_iter()
                .map(|c| {
                    let n = self.asks.get();
                    let suggestion = match n {
                        1 => "used",
                        2 => "console",
                        _ => "used",
                    };
                    if n >= 2 {
                        assert_eq!(c.request.identifiers, ["Kq_"]);
                    }
                    if n == 3 {
                        // The second re-ask carries the whole history —
                        // both suggestions, each with its own reason.
                        assert!(
                            c.user_prompt.contains(
                                "\"Kq_\" was suggested as \"used\" but that conflicts with an existing name"
                            ),
                            "the collision disclosed: {}",
                            c.user_prompt
                        );
                        assert!(
                            c.user_prompt.contains(
                                "\"Kq_\" was suggested as \"console\" which is not allowed (reserved word, global built-in, or invalid syntax)"
                            ),
                            "the invalid target disclosed: {}",
                            c.user_prompt
                        );
                        assert!(
                            c.user_prompt.contains("DO NOT suggest these names: used, console"),
                            "the accumulated blocklist: {}",
                            c.user_prompt
                        );
                    }
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(vec![(
                            "Kq_".to_string(),
                            Some(suggestion.to_string()),
                        )]),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }
    let provider = TwoCollisions {
        asks: std::cell::Cell::new(0),
    };
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());
    let r = sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        &provider,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        2,
        None,
        &crate::rename::floor::MinifiedStems::of_program(semantic),
    );
    assert_eq!(provider.asks.get(), 3, "bounded: no third re-ask");
    assert_eq!(r.reasked, 2, "one re-ask round each");
    assert_eq!(r.reask_dropped, 1, "the give-up is recorded");
    assert_eq!(r.named, 0);
    assert_eq!(render_program(semantic, &state), text, "nothing applied");
}

/// Andrew's 2026-09-30 decision: the single-letter exemption is GONE. A
/// never-asked `i` is a sweep TARGET (before, `SHORT_WORDS` made it
/// invisible to the minted-shape walk — exactly the pass whose job is
/// never-asked names, finding #62), so it gets asked and can be named.
/// Under the same day's provenance targeting the letter needs no shape
/// argument at all: no record, no rename — target.
#[test]
fn a_never_asked_single_letter_is_a_sweep_target_and_gets_asked() {
    let text = "function f() {\n  var i = 0;\n  return i + 1;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let params = humanify_model::llm::CacheKeyParams::default();
    struct Naming {
        asked: std::cell::RefCell<Vec<Vec<String>>>,
    }
    impl NameProvider for Naming {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            calls
                .into_iter()
                .map(|c| {
                    self.asked.borrow_mut().push(c.request.identifiers.clone());
                    let renames: Vec<(String, Option<String>)> = c
                        .request
                        .identifiers
                        .iter()
                        .map(|id| {
                            let answer = match id.as_str() {
                                "i" => "index",
                                "f" => "handler",
                                _ => panic!("unexpected sweep target {id:?}"),
                            };
                            (id.clone(), Some(answer.to_string()))
                        })
                        .collect();
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(renames),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }
    let provider = Naming {
        asked: std::cell::RefCell::new(Vec::new()),
    };
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());

    let r = sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        &provider,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        2,
        None,
        &crate::rename::floor::MinifiedStems::of_program(semantic),
    );
    let asked: Vec<String> = provider.asked.borrow().iter().flatten().cloned().collect();
    assert!(
        asked.iter().any(|id| id == "i"),
        "the loop counter `i` is asked: {asked:?}"
    );
    assert_eq!(r.named, 2, "both targets named");
    let code = render_program(semantic, &state);
    assert!(
        code.contains("var index = 0;"),
        "the letter's answer landed: {code}"
    );
    assert!(code.contains("function handler("), "{code}");
}

/// The other half of the decision: a single letter is an acceptable
/// ANSWER. The model answering `i` for a mint target lands (a loop
/// counter may keep its letter), and the exp066 carried rule now covers
/// the APPLIED letter — `is_below_floor_name("i")` is true after the
/// `SHORT_WORDS` change — so `validated` marks it carried and the sweep
/// cannot re-roll it later in the run.
#[test]
fn a_single_letter_answer_lands_and_is_marked_carried() {
    let text = "function f() {\n  var Kq_ = two();\n  return Kq_;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let params = humanify_model::llm::CacheKeyParams::default();
    struct LetterAnswer;
    impl NameProvider for LetterAnswer {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            calls
                .into_iter()
                .map(|c| {
                    let renames: Vec<(String, Option<String>)> = c
                        .request
                        .identifiers
                        .iter()
                        .map(|id| {
                            let answer = match id.as_str() {
                                "Kq_" => "i",
                                "f" => "handler",
                                _ => panic!("unexpected sweep target {id:?}"),
                            };
                            (id.clone(), Some(answer.to_string()))
                        })
                        .collect();
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(renames),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());

    let r = sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        &LetterAnswer,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        2,
        None,
        &crate::rename::floor::MinifiedStems::of_program(semantic),
    );
    assert_eq!(r.named, 2, "the letter answer applied");
    let code = render_program(semantic, &state);
    assert!(
        code.contains("var i = two();"),
        "the single-letter answer landed: {code}"
    );
    assert_eq!(
        state.carried_count(),
        1,
        "the deliberately applied `i` is carried — the sweep must not re-roll it"
    );
    // And the carried protection is real: a fresh target collection over
    // the SAME state (what a later sweep round in the run would see) must
    // not include the applied letter, and a second sweep dispatch asks
    // nothing at all.
    let again = collect_sweep_targets(semantic, &state, &eligible, &taint, None);
    assert!(
        again.iter().all(|t| t.name != "i"),
        "the carried `i` is not a target again: {:?}",
        again.iter().map(|t| t.name.clone()).collect::<Vec<_>>()
    );
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());

    let r2 = sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        &LetterAnswer,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        2,
        None,
        &crate::rename::floor::MinifiedStems::of_program(semantic),
    );
    assert_eq!(
        (r2.named, r2.dispatches.len()),
        (0, 0),
        "the applied letter is never re-asked within the run"
    );
}

/// The answer filter keeps its PURPOSE — refusing re-minted junk — now
/// that single letters pass: `a1b`-shaped mint heads and `_`-tails are
/// still refused as still-below-floor, never applied.
#[test]
fn sweep_junk_answers_are_still_refused() {
    let text = "function f() {\n  var Kq_ = two();\n  return Kq_;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let params = humanify_model::llm::CacheKeyParams::default();
    struct Junk;
    impl NameProvider for Junk {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            calls
                .into_iter()
                .map(|c| {
                    let renames: Vec<(String, Option<String>)> = c
                        .request
                        .identifiers
                        .iter()
                        .map(|id| {
                            let answer = match id.as_str() {
                                "Kq_" => "a1b",
                                "f" => "x_",
                                _ => panic!("unexpected sweep target {id:?}"),
                            };
                            (id.clone(), Some(answer.to_string()))
                        })
                        .collect();
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(renames),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());

    let r = sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        &Junk,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        2,
        None,
        &crate::rename::floor::MinifiedStems::of_program(semantic),
    );
    assert_eq!(r.named, 0, "no junk answer applied");
    assert_eq!(r.skipped, 2);
    assert_eq!(render_program(semantic, &state), text, "nothing applied");
    let reasons: Vec<&str> = state
        .trail()
        .entries()
        .iter()
        .flat_map(|e| &e.attempts)
        .filter_map(|a| a.reason.as_deref())
        .collect();
    assert_eq!(
        reasons,
        ["still-below-floor", "still-below-floor"],
        "both junk answers are refused as still-below-floor"
    );
}

/// Fix A (2026-10-03) in the sweep: the sweep asks the SAME answer-quality
/// question as the main pass. `setMethodH6t` wears the program's minified
/// `H6t` as a word — its shape alone passes the sweep's junk filter (the
/// stem is at the END), so before the fix it applied. Refused, it gets a
/// disclosed re-ask naming the stem, and the descriptive re-ask lands.
#[test]
fn a_sweep_answer_borrowing_a_minified_stem_is_refused_and_reasked() {
    let text = "var H6t = {};\nfunction f() {\n  var Kq_ = two(H6t);\n  return Kq_;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let params = humanify_model::llm::CacheKeyParams::default();
    struct Borrowing {
        reasks: std::cell::Cell<usize>,
    }
    impl NameProvider for Borrowing {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            calls
                .into_iter()
                .map(|c| {
                    let retry = c.request.prior_rejects.is_some();
                    if retry {
                        self.reasks.set(self.reasks.get() + 1);
                        assert!(
                            c.user_prompt.contains(
                                "- \"Kq_\" was suggested as \"setMethodH6t\" which reuses the minified name \"H6t\""
                            ),
                            "the re-ask names the borrowed stem: {}",
                            c.user_prompt
                        );
                    }
                    let renames: Vec<(String, Option<String>)> = c
                        .request
                        .identifiers
                        .iter()
                        .map(|id| {
                            let answer = match id.as_str() {
                                "Kq_" if retry => "methodSetter",
                                "Kq_" => "setMethodH6t",
                                // Everything else DECLINES (an empty
                                // answer): echoing `H6t` would now be a
                                // refused echo (round 2), not a decline.
                                _ => "",
                            };
                            (id.clone(), Some(answer.to_string()))
                        })
                        .collect();
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(renames),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }
    let provider = Borrowing {
        reasks: std::cell::Cell::new(0),
    };
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());
    let r = sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        &provider,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        2,
        None,
        &crate::rename::floor::MinifiedStems::of_program(semantic),
    );
    let code = render_program(semantic, &state);
    assert!(
        !code.contains("setMethodH6t"),
        "the junk never lands: {code}"
    );
    assert!(code.contains("var methodSetter = two(H6t);"), "{code}");
    assert_eq!(provider.reasks.get(), 1, "one disclosed re-ask");
    assert_eq!(r.reasked, 1);
}

/// Round 2's IDENTITY ECHO in the sweep — the same answer-quality rule as
/// the wave barrier: a multi-letter minified target answered with itself
/// (`yl` → `yl`) is refused (a disclosed re-ask saying it IS the minified
/// name), and a model that keeps echoing leaves it EXHAUSTED — still a
/// target — never a decided keep. A single letter's echo stays a keep.
#[test]
fn a_sweep_echo_of_a_minified_name_is_refused_and_reasked() {
    let text = "function f() {\n  var yl = two();\n  var i = one();\n  return yl + i;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let params = humanify_model::llm::CacheKeyParams::default();
    struct Echoing {
        reasks: std::cell::Cell<usize>,
        relent: bool,
    }
    impl NameProvider for Echoing {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            calls
                .into_iter()
                .map(|c| {
                    let retry = c.request.prior_rejects.is_some();
                    if retry {
                        self.reasks.set(self.reasks.get() + 1);
                        assert!(
                            c.user_prompt.contains(
                                "- \"yl\" is the minified name; suggest a descriptive name"
                            ),
                            "the re-ask says the echo IS the minified name: {}",
                            c.user_prompt
                        );
                    }
                    let renames: Vec<(String, Option<String>)> = c
                        .request
                        .identifiers
                        .iter()
                        .map(|id| {
                            let answer = match id.as_str() {
                                "yl" if retry && self.relent => "itemTotal",
                                other => other,
                            };
                            (id.clone(), Some(answer.to_string()))
                        })
                        .collect();
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(renames),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }
    for relent in [true, false] {
        let provider = Echoing {
            reasks: std::cell::Cell::new(0),
            relent,
        };
        let mut state = RenameState::new(semantic, Anchor::Fresh);
        let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());
        let r = sweep_minted_names(
            semantic,
            &mut state,
            &eligible,
            &taint,
            &provider,
            &mut log,
            Anchor::Fresh,
            &params,
            usize::MAX,
            2,
            None,
            &crate::rename::floor::MinifiedStems::of_program(semantic),
        );
        let code = render_program(semantic, &state);
        if relent {
            assert!(code.contains("var itemTotal = two();"), "{code}");
            assert_eq!(provider.reasks.get(), 1, "one disclosed re-ask");
        } else {
            assert!(code.contains("var yl = two();"), "{code}");
            assert_eq!(provider.reasks.get(), 2, "the budget");
            assert_eq!(r.exhausted_names, ["yl"], "still a target; `i` is a keep");
        }
        assert!(code.contains("var i = one();"), "{code}");
    }
}

/// Finding #64's never-asked class, generalized by Andrew's 2026-09-30
/// provenance decision: the sweep targets bindings that were never asked
/// ANYWHERE, whatever their name looks like. The real-tree exemplar is
/// `Ye` in `[Ye, setSelectedIndex]` (is-item-or-disabled-header.js, 2.1.213
/// walk trees) — a MULTI-LETTER destructure value slot no ask window ever
/// covered. `setSelected` here stands for the half that WAS decided: the
/// pass records a wave rename for it before the sweep runs, so the LEDGER
/// (not the shape) is what keeps it out.
#[test]
fn a_never_asked_destructure_value_slot_is_a_sweep_target_even_when_descriptive() {
    let text = "function f() {\n  const [valueSlot, setValueSlot] = use();\n  return valueSlot + setValueSlot;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    // The setter half was decided by a wave: the model renamed it.
    let scope = scope_holding(&state, "valueSlot");
    let setter = state.get_binding(scope, "setValueSlot").expect("setter");
    let applied = state.attempt_validated_rename(
        crate::rename::validated::RenameRequest {
            scope,
            old_name: "setValueSlot",
            new_name: "setSelected",
            expected: None,
        },
        crate::rename::validated::TrailSpec::CallerRecords {
            tier: crate::trail::Tier::Llm,
        },
    );
    assert!(applied.applied, "the wave rename landed");
    state.record(
        setter,
        "setValueSlot",
        crate::trail::Attempt::new(crate::trail::Tier::Llm, crate::trail::Outcome::Applied)
            .proposed("setSelected"),
        false,
    );
    let targets = collect_sweep_targets(semantic, &state, &eligible, &taint, None);
    let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    assert!(
        names.contains(&"valueSlot"),
        "the never-asked value slot reaches the sweep whatever its name looks like: {names:?}"
    );
    assert!(
        !names.contains(&"setSelected") && !names.contains(&"setValueSlot"),
        "the renamed half is out by the LEDGER, not by shape: {names:?}"
    );
}

/// Andrew's 2026-09-30 provenance targeting, the churn half: a binding the
/// model was already asked about is NOT re-targeted (the p2sBytes-class
/// re-roll ends), and neither is one renamed this run — while an identifier
/// whose retry budget EXHAUSTED still-unrenamed remains a target (it is
/// exactly the thing that is not properly renamed).
#[test]
fn decided_bindings_are_not_retargeted_but_exhausted_ones_are() {
    let text = "function f() {\n  var used = one();\n  var Kq_ = two();\n  var Och_ = three();\n  return used + Kq_ + Och_;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let f_scope = scope_holding(&state, "Kq_");
    // `used` was renamed this run.
    let used = state.get_binding(f_scope, "used").expect("used");
    let applied = state.attempt_validated_rename(
        crate::rename::validated::RenameRequest {
            scope: f_scope,
            old_name: "used",
            new_name: "usedValue",
            expected: None,
        },
        crate::rename::validated::TrailSpec::CallerRecords {
            tier: crate::trail::Tier::Llm,
        },
    );
    assert!(applied.applied);
    state.record(
        used,
        "used",
        crate::trail::Attempt::new(crate::trail::Tier::Llm, crate::trail::Outcome::Applied)
            .proposed("usedValue"),
        false,
    );
    // `Kq_` was asked and the model declined — a terminal keep.
    let kq = state.get_binding(f_scope, "Kq_").expect("Kq_");
    state.record(
        kq,
        "Kq_",
        crate::trail::Attempt::new(crate::trail::Tier::Llm, crate::trail::Outcome::Abstained)
            .reason("llm-declined"),
        false,
    );
    // `Och_` was asked, every suggestion collided, the budget died.
    let och = state.get_binding(f_scope, "Och_").expect("Och_");
    state.record(
        och,
        "Och_",
        crate::trail::Attempt::new(crate::trail::Tier::Llm, crate::trail::Outcome::Rejected)
            .proposed("taken")
            .reason("target-in-scope"),
        false,
    );
    state.mark_exhausted(och);
    let targets = collect_sweep_targets(semantic, &state, &eligible, &taint, None);
    let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    assert!(
        !names.contains(&"usedValue") && !names.contains(&"used"),
        "a rename applied this run is not re-asked: {names:?}"
    );
    assert!(
        !names.contains(&"Kq_"),
        "a model-kept name is not re-asked — the p2sBytes churn ends: {names:?}"
    );
    assert!(
        names.contains(&"Och_"),
        "a retry-exhausted identifier is STILL a target: {names:?}"
    );
}

/// The convention carve-outs stay carve-outs under provenance targeting:
/// all-underscore (`_`, `__`) and `$`-only names are deliberate
/// placeholders, never asked, never counted as missed.
#[test]
fn convention_carveouts_are_never_sweep_targets() {
    let text = "function f() {\n  var _ = one();\n  var __ = two();\n  var $ = three();\n  return _ + __ + $;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let state = RenameState::new(semantic, Anchor::Fresh);
    let targets = collect_sweep_targets(semantic, &state, &eligible, &taint, None);
    let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    for out in ["_", "__", "$"] {
        assert!(
            !names.contains(&out),
            "the {out:?} carve-out is never a target: {names:?}"
        );
    }
}

/// The deferred sweep parses a NEW text (reconciled-or-generated), so
/// per-binding identity from the naming era does not reach it. Its ledger
/// join is BY NAME — the one identity that survives the text boundary —
/// and it must respect the same rule: decided names are skipped, exhausted
/// and never-asked names are asked.
#[test]
fn the_deferred_sweep_skips_decided_names_and_keeps_exhausted_ones() {
    let text = "var Kq_ = one();\nvar kept_ = two();\nvar Och_ = three();\nvar renamed = four();\nfunction f() { return Kq_ + kept_ + Och_ + renamed; }";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let params = humanify_model::llm::CacheKeyParams::default();
    struct Declining {
        asked: std::cell::RefCell<Vec<String>>,
    }
    impl NameProvider for Declining {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            calls
                .into_iter()
                .map(|c| {
                    self.asked
                        .borrow_mut()
                        .extend(c.request.identifiers.iter().cloned());
                    Ok(BatchRenameResponse {
                        renames: Renames::default(),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }
    let provider = Declining {
        asked: std::cell::RefCell::new(Vec::new()),
    };
    let decided = DecidedNames {
        renamed_to: ["renamed".to_string()].into_iter().collect(),
        asked: ["kept_".to_string()].into_iter().collect(),
        exhausted: ["Och_".to_string()].into_iter().collect(),
    };
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());
    let out = run_deferred_sweep(
        text,
        Anchor::Generated,
        &eligible,
        &provider,
        &mut log,
        &params,
        usize::MAX,
        crate::trail::StrategyTrail::enabled(),
        false,
        2,
        &decided,
        &crate::rename::floor::MinifiedStems::default(),
    )
    .expect("the sweep parses its text");
    let asked = provider.asked.borrow().clone();
    assert!(
        asked.iter().any(|n| n == "Kq_"),
        "the never-asked name is asked: {asked:?}"
    );
    assert!(
        asked.iter().any(|n| n == "Och_"),
        "the retry-exhausted name is still a target: {asked:?}"
    );
    assert!(
        !asked.iter().any(|n| n == "kept_"),
        "the decided-kept name is skipped: {asked:?}"
    );
    assert!(
        !asked.iter().any(|n| n == "renamed"),
        "the renamed-by-era name is skipped: {asked:?}"
    );
    // The deferred sweep's classification declares the by-name join.
    let provenance = out.sweep.provenance.as_ref().expect("the classification");
    assert!(provenance.joined);
    assert_eq!(
        provenance.total, 5,
        "Kq_, kept_, Och_, renamed and f (the callees are free refs)"
    );
    assert_eq!(provenance.renamed, 1, "`renamed`, by name");
    // `kept_` joins as decided by name; `Kq_`, `Och_` and `f` were RE-ASKED
    // by this sweep's own dispatches and the decline terminal-keeps them —
    // the newest record this run wins over the join's class.
    assert_eq!(provenance.asked_kept, 4);
    assert_eq!(provenance.exhausted, 0);
    assert_eq!(
        provenance.never_asked, 0,
        "this sweep asked every target it had"
    );
}

/// `--rename-retries 1` restores the pre-2026-09-29 single-reask bound
/// for the sweep too; `--rename-retries 0` never re-asks.
#[test]
fn the_sweep_reask_budget_is_configurable() {
    let text = "function f() {\n  var used = one();\n  var Kq_ = two();\n  return used + Kq_;\n}";
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let params = humanify_model::llm::CacheKeyParams::default();
    struct Counting {
        asks: std::cell::Cell<usize>,
    }
    impl NameProvider for Counting {
        fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
            self.asks.set(self.asks.get() + calls.len());
            calls
                .into_iter()
                .map(|_| {
                    Ok(BatchRenameResponse {
                        renames: Renames::from_entries(vec![(
                            "Kq_".to_string(),
                            Some("used".to_string()),
                        )]),
                        finish_reason: None,
                        usage: None,
                    })
                })
                .collect()
        }
    }
    let one = Counting {
        asks: std::cell::Cell::new(0),
    };
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());
    let r = sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        &one,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        1,
        None,
        &crate::rename::floor::MinifiedStems::of_program(semantic),
    );
    assert_eq!(one.asks.get(), 2, "a single-reask budget is the old bound");
    assert_eq!(r.reasked, 1);
    assert_eq!(r.reask_dropped, 1, "the give-up is still recorded");

    let zero = Counting {
        asks: std::cell::Cell::new(0),
    };
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(params.clone());
    let r = sweep_minted_names(
        semantic,
        &mut state,
        &eligible,
        &taint,
        &zero,
        &mut log,
        Anchor::Fresh,
        &params,
        usize::MAX,
        0,
        None,
        &crate::rename::floor::MinifiedStems::of_program(semantic),
    );
    assert_eq!(zero.asks.get(), 1, "a zero budget never re-asks");
    assert_eq!(r.reasked, 0);
    assert_eq!(r.reask_dropped, 0);
    assert_eq!(
        r.skipped, 3,
        "the declined `f` and `used` plus the rejected `Kq_` stay skipped-but-counted"
    );
}
