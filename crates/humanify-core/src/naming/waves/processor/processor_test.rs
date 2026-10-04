//! The processor's pure helpers (processor.ts extractRetrySnippet,
//! buildRetryUsedNames) and the JS Set/Record order semantics they lean on,
//! plus the wave-level pins for the collision-retry fix (2026-09-28): the
//! avoid-lists must carry the names this run already applied, and a
//! collision-class barrier rejection gets exactly one disclosed re-ask.
//! The ask-trace pins (the `--dump-asks` record): every re-ask records its
//! reask class, and the record is bounded — exactly one retry per scope.

use std::cell::RefCell;

use super::{build_retry_used_names, extract_retry_snippet};
use crate::naming::ask_trace::RetryCause;
use crate::naming::waves::jsset::{JsRecord, JsSet};
use crate::naming::waves::processor::DEFAULT_PROMPT_WINDOW;

/// The plain `--sequential`-shaped config the collision pins run under.
fn plain_config() -> crate::naming::driver::NamingConfig {
    crate::naming::driver::NamingConfig {
        layout: crate::toolchain::BundleLayout::SingleWrapperFunction,
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        never_rename: crate::rename::eligibility::NeverRename::UNIVERSAL,
        module_group_size: 10,
        skip_libraries: true,
        reconcile_prior_diff: false,
        naming_floor: false,
        naming_floor_sweep: false,
        source_map: false,
        emit_rename_ledger: false,
        family_permute_disabled: false,
        params: humanify_model::llm::CacheKeyParams {
            model: "m".into(),
            temperature: Some(0.0),
            max_tokens: None,
            reasoning_effort: None,
        },
        capture_dump: false,
        tunables: Default::default(),
        shingle_probe: false,
        fast: crate::fast::FastTier::Off,
        prompt_window: DEFAULT_PROMPT_WINDOW,
    }
}

/// The retaining test log (finding #65): the full records stay readable by
/// the pins, and the streamed rows land in its in-memory oracle.
fn retain_log() -> crate::artifact_dump::DispatchLog {
    crate::artifact_dump::DispatchLog::retain_for_tests(plain_config().params)
}

/// A provider answering every requested identifier through `name_of`
/// (defaults to `<id>Named`).
struct MapProvider {
    a_asks: RefCell<usize>,
}

impl MapProvider {
    fn new() -> MapProvider {
        MapProvider {
            a_asks: RefCell::new(0),
        }
    }
}

fn name_of(id: &str) -> String {
    match id {
        "e0" => "eventHooks".to_string(),
        "q" => "qBase".to_string(),
        "q1" => "eventHooks".to_string(),
        "e" => "eventHooks".to_string(),
        "a" => "eventHooks".to_string(),
        other => plain_name(other),
    }
}

/// The stub's descriptive answer for an id: `<id>Named` — with its digits
/// spelled as letters when the id is itself a borrowable minified name
/// (`p01` → `pABNamed`): its own name as a word would be refused as a
/// borrowed stem (Fix A, 2026-10-03; case-insensitively since round 2, so
/// upper-casing no longer dodges it), which these pins are not about.
fn plain_name(id: &str) -> String {
    if crate::rename::floor::is_borrowable_stem(crate::rename::name_profile::NameProfile::Bun, id) {
        let spelled: String = id
            .chars()
            .map(|c| match c.to_digit(10) {
                Some(d) => char::from(b'A' + d as u8),
                None => c,
            })
            .collect();
        format!("{spelled}Named")
    } else {
        format!("{id}Named")
    }
}

impl humanify_model::llm::NameProvider for MapProvider {
    fn run_wave(
        &self,
        calls: Vec<humanify_model::llm::LlmCall>,
    ) -> Vec<Result<humanify_model::llm::BatchRenameResponse, humanify_model::llm::LlmError>> {
        calls
            .into_iter()
            .map(|c| {
                let entries: Vec<(String, Option<String>)> = c
                    .request
                    .identifiers
                    .iter()
                    .map(|id| {
                        let mut s = name_of(id);
                        // On any RE-ASK of `a`, offer a fresh name.
                        if id == "a"
                            && c.request.is_retry == Some(true)
                            && *self.a_asks.borrow() >= 1
                        {
                            s = "eventNameKey".to_string();
                        }
                        (id.clone(), Some(s))
                    })
                    .collect();
                if c.request.identifiers.iter().any(|i| i == "a") {
                    *self.a_asks.borrow_mut() += 1;
                }
                Ok(humanify_model::llm::BatchRenameResponse {
                    renames: humanify_model::llm::Renames::from_entries(entries),
                    finish_reason: None,
                    usage: None,
                })
            })
            .collect()
    }
}

/// Fix 1, function path: a function asked in a LATER wave must be told, in
/// its "Names already in use" avoid-list, the names earlier waves' renames
/// left in its scope chain. `eventHooks` and `qBase` look eligible (they
/// are plain words) but are TAKEN — the old `!isEligible` filter dropped
/// them, so the model was never told they were in use.
#[test]
fn a_later_waves_prompt_lists_the_names_earlier_waves_applied() {
    let fresh = "var q = 4;\n\
                 function e0(p) {\n  return p + q;\n}\n\
                 function late(z) {\n  return e0(z) + q;\n}\n\
                 console.log(late(e0(q)));\n";
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &plain_config(),
        &MapProvider::new(),
        &mut retain_log(),
    )
    .expect("the stage runs");
    let late = out
        .waves
        .dispatches
        .iter()
        .find(|d| d.request.identifiers.iter().any(|i| i == "z"))
        .expect("the late function was asked");
    assert!(late.wave >= 1, "late is asked after its callee settled");
    let line = late
        .user_prompt
        .lines()
        .find(|l| l.starts_with("Names already in use (MUST avoid these):"))
        .expect("an avoid-list");
    assert!(
        line.contains("eventHooks") && line.contains("qBase"),
        "the avoid-list must name the siblings this run renamed: {line}"
    );
    let code = out.code.expect("shipped");
    assert!(
        code.contains("eventHooks") && code.contains("qBase"),
        "{code}"
    );
}

/// Fix 1, module path + the wave retry pin: two nodes suggest the same
/// name in one round; the barrier rejects the loser, and its ONE re-ask
/// must (a) exist and disclose the collision, and (b) carry the OTHER
/// names this run applied in the retry's avoid-list (the model needs more
/// than the previous suggestion — the sibling `q2Base` is taken too).
#[test]
fn a_module_collision_retry_discloses_and_lists_the_names_the_run_applied() {
    let fresh = "var q1 = 1;\n\
                 var q2 = 2;\n\
                 function e0(p) {\n  return p + q1;\n}\n\
                 console.log(e0(q2), q1, q2);\n";
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &plain_config(),
        &MapProvider::new(),
        &mut retain_log(),
    )
    .expect("the stage runs");
    let retry: Vec<_> = out
        .waves
        .dispatches
        .iter()
        .filter(|d| d.request.is_retry == Some(true))
        .collect();
    assert_eq!(
        retry.len(),
        2,
        "the stubborn loser runs the default budget out (two re-asks)"
    );
    let first = retry[0];
    assert!(
        first
            .user_prompt
            .contains("DO NOT suggest these names: eventHooks"),
        "the re-ask discloses the collision: {}",
        first.user_prompt
    );
    assert!(
        first.request.used_names.iter().any(|n| n == "q2Named"),
        "the re-ask's avoid-list must carry the sibling the run just named: {:?}",
        first.request.used_names
    );
    // The SECOND re-ask re-discloses the whole accumulated history: the
    // MapProvider suggested `eventHooks` twice, so both disclosure lines
    // are present (the do-not list dedups the repeated name).
    let second = retry[1];
    assert!(
        second
            .user_prompt
            .contains("DO NOT suggest these names: eventHooks"),
        "the second re-ask still blocklists the collided name: {}",
        second.user_prompt
    );
    assert_eq!(
        second
            .user_prompt
            .matches(
                "- \"q1\" was suggested as \"eventHooks\" but that name is already used by another function in the same scope"
            )
            .count(),
        2,
        "every prior suggestion gets its own disclosure line: {}",
        second.user_prompt
    );
    let code = out.code.expect("shipped");
    assert!(code.contains("eventHooks"), "{code}");
    assert!(code.contains("q2Named"), "{code}");
    // The budget exhausted, the loser kept the deterministic decoration.
    assert!(code.contains("eventHooksVal"), "{code}");
    // Exactly one binding won eventHooks; the loser got a fresh name.
    assert_ne!(code.matches("eventHooks").count(), 0);
}

/// The wave barrier pin: two LANES of one function answer in the same
/// round against frozen state; the loser's collision rejection seeds ONE
/// disclosed re-ask at the next wave step, and the re-asked suggestion
/// applies. (Discovered green 2026-09-28 — the wave machinery already
/// had the re-ask; this pins it.)
#[test]
fn a_cross_lane_collision_gets_exactly_one_disclosed_reask() {
    let mut params = String::new();
    for i in 0..20 {
        params.push_str(&format!("p{i:02}, "));
    }
    params.push_str("p19x");
    let mut vars = String::new();
    for i in 0..15 {
        if i == 2 {
            vars.push_str("  var e = f(p00);\n");
        } else {
            vars.push_str(&format!("  var v{i:02} = p{i:02};\n"));
        }
    }
    vars.push_str("  var a = e + p01;\n");
    let fresh = format!(
        "function hooks({params}) {{\n{vars}  return a + e + v00;\n}}\nconsole.log(hooks(1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1));\n"
    );
    let provider = MapProvider::new();
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh: &fresh,
            prior: None,
            library: None,
        },
        &plain_config(),
        &provider,
        &mut retain_log(),
    )
    .expect("the stage runs");
    let a_asks = *provider.a_asks.borrow();
    assert_eq!(a_asks, 2, "`a` is asked exactly twice (ask + one re-ask)");
    let retry = out
        .waves
        .dispatches
        .iter()
        .find(|d| d.request.is_retry == Some(true) && d.request.identifiers == ["a"])
        .expect("the disclosed re-ask");
    assert!(
        retry
            .user_prompt
            .contains("DO NOT suggest these names: eventHooks"),
        "disclosure: {}",
        retry.user_prompt
    );
    assert!(
        retry
            .user_prompt
            .contains("Names already in use (MUST avoid ALL of these): eventHooks"),
        "the re-ask's avoid-list names the collision: {}",
        retry.user_prompt
    );
    let code = out.code.expect("shipped");
    assert!(
        code.contains("var eventNameKey = eventHooks + pABNamed;"),
        "{code}"
    );
}

/// The ask-trace pin: every disclosed re-ask of the collision fix records
/// `retryCause: NameTaken` (the barrier's used-set collision had no
/// applier code; the cause is the reask class verbatim) and the series is
/// BOUNDED — no scope re-asks more than `reask::REASK_LIMIT` (2) times.
/// The MapProvider is stubborn (it answers `eventHooks` on every ask of
/// the colliding pair), so the loser runs the budget to exhaustion.
#[test]
fn the_collision_reask_records_its_cause_and_is_bounded_in_the_ask_log() {
    let fresh = "var q1 = 1;\n\
                 var q2 = 2;\n\
                 function e0(p) {\n  return p + q1;\n}\n\
                 console.log(e0(q2), q1, q2);\n";
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &plain_config(),
        &MapProvider::new(),
        &mut retain_log(),
    )
    .expect("the stage runs");
    let retry: Vec<_> = out
        .waves
        .dispatches
        .iter()
        .filter(|d| d.request.is_retry == Some(true))
        .collect();
    assert_eq!(
        retry.len(),
        2,
        "the two re-asks of the default budget (REASK_LIMIT is 2)"
    );
    for ask in retry.iter().map(|d| &d.ask) {
        assert_eq!(ask.cause, Some(RetryCause::NameTaken));
        assert!(
            ask.detail.is_none(),
            "a used-set collision has no applier code"
        );
    }
    // The lane-path re-asks (a round-2 `failures.duplicates` ask, not a
    // barrier seed) leave the cause for the writer to derive.
    let first_round = out
        .waves
        .dispatches
        .iter()
        .find(|d| d.request.is_retry != Some(true))
        .expect("the first-round ask exists");
    assert_eq!(first_round.ask.cause, None);
    assert_eq!(first_round.ask.phase, 0);
}

/// Finding #66's taken-set fix, red test: the two late functions are
/// asked in the SAME round, so their contexts are built while the
/// program scope's table is unchanged — they must retain ONE shared
/// taken snapshot (the #56 pattern on the renamed-name field), not two
/// private clones of the same names. Before the sharing fix this reads
/// 4 (each context's own copy); after it, 2 (one snapshot, two names).
#[test]
fn the_late_contexts_share_one_taken_snapshot() {
    let fresh = "var q = 4;\n\
                 function e0(p) {\n  return p + q;\n}\n\
                 function late1(z) {\n  return e0(z) + q;\n}\n\
                 function late2(y) {\n  return e0(y) * q;\n}\n\
                 console.log(late1(e0(q)) + late2(q));\n";
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &plain_config(),
        &MapProvider::new(),
        &mut retain_log(),
    )
    .expect("the stage runs");
    let g = &out.waves.gauges;
    assert_eq!(
        g.taken_set_names, 2,
        "one shared {{eventHooks, qBase}} snapshot, not a clone per context"
    );
    // No private clone remains: the shared snapshot's bytes are the
    // renamed-layers map's copy (charged to usedSetBytes), so the
    // strategy split's taken term holds only the per-context layer
    // lists (2 Arc pointers per fn context — under one set's 65 bytes).
    assert!(
        g.strategy_taken_bytes < 65,
        "no private taken clone: {}",
        g.strategy_taken_bytes
    );
}

/// Finding #66's taken-set sub-gauge, pinned on the collision fixture's
/// shape: two functions asked in the SAME later wave each cover the
/// program scope's already-applied names in their taken sets (the probe
/// that will flip when the private clones become shared snapshots —
/// see `the_late_contexts_share_one_taken_snapshot`).
#[test]
fn the_strategy_split_pins_the_taken_sets_on_a_same_wave_pair() {
    let fresh = "var q = 4;\n\
                 function e0(p) {\n  return p + q;\n}\n\
                 function late1(z) {\n  return e0(z) + q;\n}\n\
                 function late2(y) {\n  return e0(y) * q;\n}\n\
                 console.log(late1(e0(q)) + late2(q));\n";
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &plain_config(),
        &MapProvider::new(),
        &mut retain_log(),
    )
    .expect("the stage runs");
    let g = &out.waves.gauges;
    // The six constituents sum to the total.
    assert_eq!(
        g.strategy_bindings_bytes
            + g.strategy_taken_bytes
            + g.strategy_callee_bytes
            + g.strategy_callsite_bytes
            + g.strategy_context_var_bytes
            + g.strategy_module_bytes,
        g.strategy_bytes,
        "the split partitions the strategy bytes"
    );
    // The run applied q->qBase and e0->eventHooks before the late pair
    // was asked: each late context's taken set holds them.
    assert!(
        g.taken_set_names >= 2,
        "names are held: {}",
        g.taken_set_names
    );
}

#[test]
fn short_code_is_sent_whole_on_retries() {
    let code = "function f(a) {\n  return a;\n}";
    assert_eq!(extract_retry_snippet(code, &["a".to_string()]), code);
}

#[test]
fn long_code_keeps_the_signature_and_referencing_lines_with_context() {
    let mut lines = vec!["function f(a) {".to_string()];
    for i in 0..40 {
        lines.push(format!("  x{i}();"));
    }
    lines[20] = "  use(Qr);".to_string();
    lines.push("}".to_string());
    let code = lines.join("\n");
    let out = extract_retry_snippet(&code, &["Qr".to_string()]);
    assert_eq!(
        out,
        "function f(a) {\n  // …\n  x17();\n  x18();\n  use(Qr);\n  x20();\n  x21();\n  // …"
    );
    // `$` is an identifier character: `$Qr` does not hold `Qr`.
    assert!(
        !extract_retry_snippet(&code.replace("use(Qr)", "use($Qr)"), &["Qr".to_string()])
            .contains("use(")
    );
}

#[test]
fn retry_used_names_lead_with_the_collided_suggestions_capped_at_25() {
    let mut prev = JsRecord::default();
    prev.set("a", "taken");
    prev.set("b", "taken");
    let windowed: Vec<String> = (0..40).map(|i| format!("n{i}")).collect();
    let out = build_retry_used_names(&windowed, &prev);
    assert_eq!(out[0], "taken");
    assert_eq!(out.len(), 25);
    assert_eq!(out[1], "n0");
}

#[test]
fn a_js_set_moves_a_renamed_member_to_the_end() {
    let mut s = JsSet::new();
    for n in ["a", "b", "c"] {
        s.add(n);
    }
    s.add("a");
    assert_eq!(s.to_vec(), ["a", "b", "c"]);
    s.delete("a");
    s.add("z");
    assert_eq!(s.to_vec(), ["b", "c", "z"]);
    let mut r = JsRecord::default();
    r.set("x", "1");
    r.set("y", "2");
    r.set("x", "3");
    assert_eq!(
        r.0,
        vec![("x".into(), "3".into()), ("y".into(), "2".into())]
    );
}

/// A provider for the late-rejection scenario: `e0` renames cleanly, and
/// `p` suggests `taken` on the first ask — a name that shadows the child
/// function's own binding, a class the lane's used-set check cannot see, so
/// only the scope-safety check rejects it at claim time.
struct LateProvider {
    p_asks: RefCell<u32>,
}

impl LateProvider {
    fn new() -> LateProvider {
        LateProvider {
            p_asks: RefCell::new(0),
        }
    }
}

impl humanify_model::llm::NameProvider for LateProvider {
    fn run_wave(
        &self,
        calls: Vec<humanify_model::llm::LlmCall>,
    ) -> Vec<Result<humanify_model::llm::BatchRenameResponse, humanify_model::llm::LlmError>> {
        calls
            .into_iter()
            .map(|c| {
                let entries: Vec<(String, Option<String>)> = c
                    .request
                    .identifiers
                    .iter()
                    .map(|id| {
                        let s = match id.as_str() {
                            "e0" => "outerFn".to_string(),
                            "p" => {
                                let mut n = self.p_asks.borrow_mut();
                                *n += 1;
                                if *n == 1 {
                                    "taken".to_string()
                                } else {
                                    "paramValue".to_string()
                                }
                            }
                            other => format!("{other}Named"),
                        };
                        (id.clone(), Some(s))
                    })
                    .collect();
                Ok(humanify_model::llm::BatchRenameResponse {
                    renames: humanify_model::llm::Renames::from_entries(entries),
                    finish_reason: None,
                    usage: None,
                })
            })
            .collect()
    }
}

/// The lane round-2 ask trace (2026-09-29): a suggestion the scope-safety
/// check rejects at claim time (`shadows-child` — invisible to the used-set
/// collision check) keeps its disclosed round-2 (the 2026-09-28 fix), and
/// that retry's ask record now carries the REJECTION's class and code
/// instead of the generic duplicate-failure derivation the writer falls
/// back to. Recording only: the retry itself is byte-unchanged.
#[test]
fn a_late_rejected_suggestions_round2_records_the_rejections_cause_and_code() {
    let fresh = "function e0(p) {\n\
                 return function inner(taken) {\n  return taken + p;\n};\n\
                 }\n\
                 console.log(e0(1));\n";
    let provider = LateProvider::new();
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &plain_config(),
        &provider,
        &mut retain_log(),
    )
    .expect("the stage runs");
    let retry = out
        .waves
        .dispatches
        .iter()
        .find(|d| d.request.is_retry == Some(true) && d.request.identifiers == ["p"])
        .expect("p's disclosed round-2");
    assert!(
        retry
            .user_prompt
            .contains("DO NOT suggest these names: taken"),
        "the retry flow is unchanged — the duplicate preamble: {}",
        retry.user_prompt
    );
    assert_eq!(retry.ask.cause, Some(RetryCause::NameTaken));
    assert_eq!(retry.ask.detail.as_deref(), Some("shadows-child"));
    assert_eq!(*provider.p_asks.borrow(), 2, "ask + one round-2, bounded");
    let code = out.code.expect("shipped");
    assert!(code.contains("paramValue"), "{code}");
}

/// The accumulation pin (2026-09-29): a collision loser whose re-asks
/// keep colliding gets the DEFAULT TWO disclosed re-asks, and the second
/// re-ask discloses EVERY prior suggestion and why it was rejected — both
/// failed names, each with its own conflict line, in the do-not-suggest
/// block. After the budget exhausts, the deterministic decoration applies.
struct RoundProvider {
    /// Retry dispatches served so far (the answers differ per round).
    retries: RefCell<usize>,
}

impl RoundProvider {
    fn new() -> RoundProvider {
        RoundProvider {
            retries: RefCell::new(0),
        }
    }
}

impl humanify_model::llm::NameProvider for RoundProvider {
    fn run_wave(
        &self,
        calls: Vec<humanify_model::llm::LlmCall>,
    ) -> Vec<Result<humanify_model::llm::BatchRenameResponse, humanify_model::llm::LlmError>> {
        calls
            .into_iter()
            .map(|c| {
                let entries: Vec<(String, Option<String>)> = c
                    .request
                    .identifiers
                    .iter()
                    .map(|id| {
                        // First round: the q1/e0 collision. Re-ask 1
                        // offers the sibling's applied name (taken);
                        // re-ask 2 offers the winner's name again.
                        let s = if c.request.is_retry != Some(true) {
                            name_of(id)
                        } else if *self.retries.borrow() == 0 {
                            "q2Named".to_string()
                        } else {
                            "eventHooks".to_string()
                        };
                        (id.clone(), Some(s))
                    })
                    .collect();
                if c.request.is_retry == Some(true) {
                    *self.retries.borrow_mut() += 1;
                }
                Ok(humanify_model::llm::BatchRenameResponse {
                    renames: humanify_model::llm::Renames::from_entries(entries),
                    finish_reason: None,
                    usage: None,
                })
            })
            .collect()
    }
}

#[test]
fn the_second_reask_discloses_every_prior_suggestion_and_is_bounded() {
    let fresh = "var q1 = 1;\n\
                 var q2 = 2;\n\
                 function e0(p) {\n  return p + q1;\n}\n\
                 console.log(e0(q2), q1, q2);\n";
    let provider = RoundProvider::new();
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &plain_config(),
        &provider,
        &mut retain_log(),
    )
    .expect("the stage runs");
    let retry: Vec<_> = out
        .waves
        .dispatches
        .iter()
        .filter(|d| d.request.is_retry == Some(true))
        .collect();
    assert_eq!(retry.len(), 2, "exactly the two re-asks of the budget");
    assert_eq!(*provider.retries.borrow(), 2, "bounded: no third re-ask");
    let second = retry[1];
    let id = &second.request.identifiers[0];
    // Every prior suggestion, oldest first, each with its reason — and,
    // since 2026-10-04, WHO holds it.
    for (failed, holder) in [
        ("eventHooks", "another function in the same scope"),
        ("q2Named", "another variable in the same scope"),
    ] {
        assert!(
            second.user_prompt.contains(&format!(
                "- \"{id}\" was suggested as \"{failed}\" but that name is already used by {holder}"
            )),
            "the {failed} failure disclosed: {}",
            second.user_prompt
        );
    }
    assert!(
        second
            .user_prompt
            .contains("DO NOT suggest these names: eventHooks, q2Named"),
        "the accumulated do-not-suggest block: {}",
        second.user_prompt
    );
    // The first re-ask discloses only the one failure it knows about.
    assert!(
        retry[0]
            .user_prompt
            .contains("DO NOT suggest these names: eventHooks\n"),
        "the first re-ask blocklists just the collided name: {}",
        retry[0].user_prompt
    );
    assert!(
        !retry[0].user_prompt.contains("q2Named\", "),
        "the first re-ask cannot disclose a failure that has not happened: {}",
        retry[0].user_prompt
    );
    let code = out.code.expect("shipped");
    assert!(
        code.contains("eventHooksVal"),
        "the exhausted budget falls back to the decoration: {code}"
    );
}

#[test]
fn a_single_reask_budget_restores_the_old_bounded_behavior() {
    let fresh = "var q1 = 1;\n\
                 var q2 = 2;\n\
                 function e0(p) {\n  return p + q1;\n}\n\
                 console.log(e0(q2), q1, q2);\n";
    let mut config = plain_config();
    config.tunables.reask_limit = 1;
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &config,
        &MapProvider::new(),
        &mut retain_log(),
    )
    .expect("the stage runs");
    let retry: Vec<_> = out
        .waves
        .dispatches
        .iter()
        .filter(|d| d.request.is_retry == Some(true))
        .collect();
    assert_eq!(retry.len(), 1, "a single-reask budget: exactly one re-ask");
    let code = out.code.expect("shipped");
    assert!(
        code.contains("eventHooksVal"),
        "the spent budget falls back to the decoration: {code}"
    );
}

#[test]
fn a_zero_reask_budget_gives_up_on_the_ladder_and_stays_counted() {
    let fresh = "var q1 = 1;\n\
                 var q2 = 2;\n\
                 function e0(p) {\n  return p + q1;\n}\n\
                 console.log(e0(q2), q1, q2);\n";
    let mut config = plain_config();
    config.tunables.reask_limit = 0;
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &config,
        &MapProvider::new(),
        &mut retain_log(),
    )
    .expect("the stage runs");
    let retry: Vec<_> = out
        .waves
        .dispatches
        .iter()
        .filter(|d| d.request.is_retry == Some(true))
        .collect();
    assert!(
        retry.is_empty(),
        "a zero reask budget: a rejection never re-asks"
    );
    // The collision still resolves deterministically — the loser keeps a
    // decorated variant of its suggestion — and the collision stays
    // COUNTED: the ladder's repair is a recorded contention event, never
    // an unrecoverable rejection.
    let code = out.code.expect("shipped");
    assert!(code.contains("eventHooks"), "{code}");
    assert!(code.contains("eventHooksVal"), "{code}");
    assert_eq!(
        out.processor.unrecoverable_rejections, 0,
        "a collision is not an unrecoverable rejection"
    );
    assert!(
        out.processor
            .contention
            .iter()
            .any(|e| e.requested == "eventHooks"
                && e.resolved_to == "eventHooksVal"
                && e.site == "wave"),
        "the disabled re-ask still records the collision's repair: {:?}",
        out.processor.contention
    );
}

/// A provider answering through a closure over (identifier, the request):
/// the borrowed-stem and do-not-list pins script their answers per round.
struct ScriptProvider<F: Fn(&str, &humanify_model::llm::BatchRenameRequest) -> String> {
    answer: F,
}

impl<F: Fn(&str, &humanify_model::llm::BatchRenameRequest) -> String>
    humanify_model::llm::NameProvider for ScriptProvider<F>
{
    fn run_wave(
        &self,
        calls: Vec<humanify_model::llm::LlmCall>,
    ) -> Vec<Result<humanify_model::llm::BatchRenameResponse, humanify_model::llm::LlmError>> {
        calls
            .into_iter()
            .map(|c| {
                let entries: Vec<(String, Option<String>)> = c
                    .request
                    .identifiers
                    .iter()
                    .map(|id| (id.clone(), Some((self.answer)(id, &c.request))))
                    .collect();
                Ok(humanify_model::llm::BatchRenameResponse {
                    renames: humanify_model::llm::Renames::from_entries(entries),
                    finish_reason: None,
                    usage: None,
                })
            })
            .collect()
    }
}

fn run_scripted(
    fresh: &str,
    answer: impl Fn(&str, &humanify_model::llm::BatchRenameRequest) -> String,
) -> crate::naming::driver::NamingOutcome {
    crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &plain_config(),
        &ScriptProvider { answer },
        &mut retain_log(),
    )
    .expect("the stage runs")
}

/// The barrier re-asks (the disclosed ones carry `prior_rejects`).
fn barrier_reasks(out: &crate::naming::driver::NamingOutcome) -> Vec<&super::DispatchRecord> {
    out.waves
        .dispatches
        .iter()
        .filter(|d| d.request.prior_rejects.is_some())
        .collect()
}

const BORROWING_PROGRAM: &str = "var H6t = 1;\n\
     function RHe(p) {\n  return p + H6t;\n}\n\
     console.log(RHe(2), H6t);\n";

/// Fix A (2026-10-03): an answer that wears ANOTHER binding's minified
/// name as a word (`H6tClass` borrows `H6t`) is refused like an invalid
/// answer — a disclosed re-ask that names the borrowed stem — and the
/// re-asked descriptive name lands. Before: the junk applied and, since
/// the provenance sweep no longer re-targets decided names, stayed.
#[test]
fn a_borrowed_minified_stem_answer_is_refused_and_reasked() {
    let out = run_scripted(BORROWING_PROGRAM, |id, r| match id {
        "RHe" if r.prior_rejects.is_some() => "addBaseCount".to_string(),
        "RHe" => "H6tClass".to_string(),
        "H6t" => "baseCount".to_string(),
        other => format!("{other}Named"),
    });
    let code = out.code.as_deref().expect("shipped");
    assert!(!code.contains("H6tClass"), "the junk never lands: {code}");
    assert!(code.contains("function addBaseCount("), "{code}");
    let reasks = barrier_reasks(&out);
    assert_eq!(reasks.len(), 1, "one disclosed re-ask");
    let prompt = &reasks[0].user_prompt;
    assert!(
        prompt.contains(
            "- \"RHe\" was suggested as \"H6tClass\" which reuses the minified name \"H6t\""
        ),
        "the re-ask names the borrowed stem: {prompt}"
    );
    assert!(
        prompt.contains("DO NOT suggest these names: H6tClass"),
        "{prompt}"
    );
}

/// Fix A, the budget: a model that keeps borrowing runs the re-ask budget
/// out and the binding stays UNRENAMED — never the junk, never a suffix
/// ladder built on the junk (`H6tClassVal`).
#[test]
fn a_stubborn_borrowed_stem_exhausts_and_stays_unrenamed() {
    let out = run_scripted(BORROWING_PROGRAM, |id, _| match id {
        "RHe" => "H6tClass".to_string(),
        "H6t" => "baseCount".to_string(),
        other => format!("{other}Named"),
    });
    let code = out.code.as_deref().expect("shipped");
    assert!(!code.contains("H6tClass"), "no junk, no ladder: {code}");
    assert!(code.contains("function RHe("), "left unrenamed: {code}");
    assert_eq!(
        barrier_reasks(&out).len(),
        2,
        "the default budget: two re-asks"
    );
}

const ECHO_PROGRAM: &str = "function Qz(yl, i) {\n  return yl + i;\n}\n\
     console.log(Qz(2, 3));\n";

/// Round 2, the IDENTITY ECHO (ref r2 2.1.216: `{"yl":"yl","zf":"zf",...}`):
/// a multi-letter minified name answered with itself is refused like a
/// borrowed answer — a disclosed re-ask saying it IS the minified name —
/// and the descriptive re-ask lands. Before: the echo settled as an
/// identity KEEP the sweep never revisits. A single letter keeps its echo
/// (a loop counter may stay `i`) and is never re-asked.
#[test]
fn an_echoed_minified_name_is_refused_and_reasked() {
    let out = run_scripted(ECHO_PROGRAM, |id, r| match id {
        "yl" if r.prior_rejects.is_some() => "addend".to_string(),
        "yl" | "i" => id.to_string(),
        other => format!("{other}Named"),
    });
    let code = out.code.as_deref().expect("shipped");
    assert!(code.contains("function QzNamed(addend, i)"), "{code}");
    let reasks = barrier_reasks(&out);
    assert_eq!(reasks.len(), 1, "one disclosed re-ask, for yl only");
    assert_eq!(reasks[0].request.identifiers, ["yl"]);
    let prompt = &reasks[0].user_prompt;
    assert!(
        prompt.contains("- \"yl\" is the minified name; suggest a descriptive name"),
        "the re-ask says the echo IS the minified name: {prompt}"
    );
}

/// The echo's budget: a model that keeps echoing runs `--rename-retries`
/// out, and the binding stays unrenamed and EXHAUSTED — a sweep target,
/// not a decided keep.
#[test]
fn a_stubborn_echo_exhausts_and_stays_a_sweep_target() {
    let mut config = plain_config();
    config.naming_floor = true;
    config.naming_floor_sweep = true;
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh: ECHO_PROGRAM,
            prior: None,
            library: None,
        },
        &config,
        &ScriptProvider {
            answer: |id: &str, _: &humanify_model::llm::BatchRenameRequest| match id {
                "yl" | "i" => id.to_string(),
                other => format!("{other}Named"),
            },
        },
        &mut retain_log(),
    )
    .expect("the stage runs");
    let code = out.code.as_deref().expect("shipped");
    assert!(code.contains("function QzNamed(yl, i)"), "{code}");
    assert_eq!(barrier_reasks(&out).len(), 2, "the default budget");
    let sweep = out.pre_sweep.as_ref().expect("the in-era sweep ran");
    let swept: Vec<&String> = sweep
        .dispatches
        .iter()
        .flat_map(|d| &d.request.identifiers)
        .collect();
    assert!(
        swept.iter().any(|n| *n == "yl"),
        "the exhausted echo is a sweep target: {swept:?}"
    );
    assert!(
        !swept.iter().any(|n| *n == "i"),
        "a letter's echo is a decided keep: {swept:?}"
    );
    assert!(
        sweep.exhausted_names.iter().any(|n| n == "yl"),
        "still echoing in the sweep: exhausted again, {:?}",
        sweep.exhausted_names
    );
}

/// Fix B (2026-10-03): the do-not list discloses the word the MODEL said.
/// `a`'s lane hears `eventHooks` (taken by the function renamed in an
/// earlier wave), exhausts, and its resolution tail decorates it to
/// `eventHooksVal` — which a sibling lane's answer wins at the barrier.
/// The re-ask must disclose `eventHooks`, the model's own word: before
/// the fix it showed `eventHooksVal`, so the model re-offered `eventHooks`
/// and collided again.
#[test]
fn the_reask_discloses_the_models_own_word_not_our_decoration() {
    let mut params = String::new();
    for i in 0..20 {
        params.push_str(&format!("p{i:02}, "));
    }
    params.push_str("p19x");
    let mut vars = String::new();
    for i in 0..15 {
        vars.push_str(&format!("  var v{i:02} = p{i:02};\n"));
    }
    vars.push_str("  var a = e0(p01);\n");
    let fresh = format!(
        "function e0(z) {{\n  return z;\n}}\n\
         function hooks({params}) {{\n{vars}  return a + v00 + v05;\n}}\n\
         console.log(hooks(1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1));\n"
    );
    let out = run_scripted(&fresh, |id, r| match id {
        "e0" => "eventHooks".to_string(),
        "a" if r.prior_rejects.is_some() => "eventNameKey".to_string(),
        "a" => "eventHooks".to_string(),
        "v05" => "eventHooksVal".to_string(),
        other => plain_name(other),
    });
    let reask = barrier_reasks(&out)
        .into_iter()
        .find(|d| d.request.identifiers == ["a"])
        .unwrap_or_else(|| {
            panic!(
                "a's barrier re-ask: {:#?}",
                out.waves
                    .dispatches
                    .iter()
                    .map(|d| &d.user_prompt)
                    .collect::<Vec<_>>()
            )
        });
    assert!(
        reask
            .user_prompt
            .contains("- \"a\" was suggested as \"eventHooks\" but that conflicts"),
        "the model's own word is disclosed: {}",
        reask.user_prompt
    );
    assert!(
        reask
            .user_prompt
            .contains("DO NOT suggest these names: eventHooks\n"),
        "the do-not list carries the model's word, not our decoration: {}",
        reask.user_prompt
    );
    let code = out.code.expect("shipped");
    assert!(code.contains("var eventNameKey = "), "{code}");
}

/// Finding #70's residual (2.1.86 `renderPluginItem`, 2026-10-03): a
/// minifier reuses the same short name for locals of SIBLING blocks — here
/// three `Z`s. The close match carries the unchanged `indented` block's
/// `Z` to its prior name (`indentColor`, applied by binding), but the
/// prompt's "already renamed" section is keyed by the MINIFIED NAME, so
/// the prompt asking for the changed `plugin` block's `Z` — a different
/// binding — also told the model "`Z` → `indentColor`, already renamed, do
/// NOT rename it again, keep consistent". The model copied it: every
/// plugin-branch local took the indented block's names. An identifier the
/// call is ASKING for is by definition not already renamed; an entry under
/// its name belongs to another binding and must not be shown.
#[test]
fn an_already_renamed_entry_never_names_an_identifier_the_call_is_asking_for() {
    let prior = r#"function renderItem(item, isSelected) {
  if (item.type === "plugin") {
    let statusIcon;
    statusIcon = mk.icon(item, 1);
    return mk.row(statusIcon);
  }
  if (item.type === "failed") {
    let failedIcon = mk.fail(item);
    return mk.row(failedIcon);
  }
  if (item.indented) {
    let indentColor = isSelected ? "suggestion" : undefined;
    let indentPrefix = isSelected ? "> " : "  ";
    return mk.indent(indentColor, indentPrefix);
  }
  return null;
}
console.log(renderItem({ type: "plugin" }, true));
"#;
    let fresh = r#"function r(q, $) {
  if (q.type === "plugin") {
    let Z;
    Z = mk.icon(q, 1);
    let e = mk.plural(q.count, "error");
    return mk.row(Z, e);
  }
  if (q.type === "failed") {
    let Z = mk.fail(q, 2);
    return mk.row(Z);
  }
  if (q.indented) {
    let Z = $ ? "suggestion" : undefined;
    let k = $ ? "> " : "  ";
    return mk.indent(Z, k);
  }
  return null;
}
console.log(r({ type: "plugin" }, true));
"#;
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh,
            prior: Some(prior),
            library: None,
        },
        &plain_config(),
        &MapProvider::new(),
        &mut retain_log(),
    )
    .expect("the stage runs");
    let code = out.code.expect("shipped");
    assert!(
        code.contains("let indentColor = "),
        "precondition: the unchanged indented block's `Z` carried its prior name:\n{code}"
    );
    let asks: Vec<_> = out
        .waves
        .dispatches
        .iter()
        .filter(|d| d.request.identifiers.iter().any(|i| i == "Z"))
        .collect();
    assert!(
        !asks.is_empty(),
        "precondition: the changed blocks' `Z`s go to the model:\n{code}"
    );
    for d in asks {
        let asked = &d.request.identifiers;
        for (old, new) in d.request.already_renamed.iter().flat_map(|m| m.0.iter()) {
            assert!(
                !asked.contains(old),
                "the prompt asks for `{old}` AND lists `{old} → {new}` as already renamed \
                 (another binding's carry, by name):\n{}",
                d.user_prompt
            );
        }
    }
}

/// The axios http adapter of 2.1.215 (`ref-scratch-0f338ffa-r1`'s rebase,
/// 2026-10-03), reduced. An ENCLOSING scope already holds a binding named
/// `requestOptions` (the SDK's private-field WeakMap), so the name is in
/// every inner function's used set. A nested callback's param was already
/// decorated to `requestOptionsVal` in an earlier wave. The adapter's `t`
/// is asked alone and the model answers `requestOptions`:
///
/// - the one-id window's only answer collides, so the all-failed rule
///   exhausts it on the spot — no disclosed round-2;
/// - the resolution tail decorates it to `requestOptionsVal`, which the
///   nested callback holds and `t` is read inside it (`shadows-child`);
/// - the tail gave up and recorded IDENTITY: `t` shipped unrenamed, never
///   re-asked, with no trail row — the leftover meter called it
///   "model-chosen".
///
/// A valid answer must land (the ladder steps past a scope-unsafe
/// decoration, as it steps past a taken one) or be re-asked — never dropped.
#[test]
fn a_valid_answer_whose_decoration_is_scope_unsafe_still_lands() {
    let fresh = r#"var Sx = new WeakMap();
function g() {
  return Sx;
}
var h = function (t) {
  g();
  return run(async function (n) {
    n.pipe({
      transform(c, requestOptionsVal, b) {
        b(t.limit, c, requestOptionsVal);
      }
    });
    return t.url;
  });
};
console.log(h);
"#;
    let out = run_scripted(fresh, |id, _| match id {
        "Sx" | "t" => "requestOptions".to_string(),
        // the nested callback keeps the decoration it already holds
        "requestOptionsVal" => "requestOptionsVal".to_string(),
        other => plain_name(other),
    });
    let code = out.code.as_deref().expect("shipped");
    assert!(
        code.contains("var requestOptions = new WeakMap();")
            && code.contains("transform(cNamed, requestOptionsVal, bNamed)"),
        "precondition: an enclosing scope holds the answer, the nested callback its decoration:\n{code}"
    );
    assert!(
        !code.contains("function (t)"),
        "the adapter's `t` was answered `requestOptions` and silently kept:\n{code}"
    );
    assert!(
        code.contains("function (requestOptionsVar)"),
        "the answer lands on the next scope-safe decoration:\n{code}"
    );
    // Since 2026-10-04 the decoration is the LAST resort: the lone
    // collision first spends the whole disclosed re-ask budget.
    let t_reasks = barrier_reasks(&out)
        .into_iter()
        .filter(|d| d.request.identifiers == ["t"])
        .count();
    assert_eq!(t_reasks, 2, "both disclosed re-asks before the ladder");
}

/// The adapter's shape (the lone-collision fix, 2026-10-04 — finding #74's
/// open item).
const ADAPTER_PROGRAM: &str = r#"var Sx = new WeakMap();
function g() {
  return Sx;
}
var h = function (t) {
  g();
  return run(async function (n) {
    n.pipe({
      transform(c, requestOptionsVal, b) {
        b(t.limit, c, requestOptionsVal);
      }
    });
    return t.url;
  });
};
console.log(h);
"#;

/// A lone identifier whose answer an enclosing-scope binding holds gets
/// the disclosed re-ask every other conflict gets — and the re-ask says
/// WHO holds each rejected name. Re-ask 1's answer is the nested
/// callback's parameter (`shadows-child`); re-ask 2 is told so, and its
/// clean answer lands. On main: no re-ask at all, `requestOptionsVar`.
#[test]
fn a_lone_colliding_answer_is_reasked_with_its_holder_disclosed() {
    let out = run_scripted(ADAPTER_PROGRAM, |id, r| {
        let round = r
            .prior_rejects
            .as_ref()
            .and_then(|p| p.get(id))
            .map_or(0, <[humanify_model::llm::PriorReject]>::len);
        match (id, round) {
            ("Sx", _) | ("t", 0) => "requestOptions".to_string(),
            ("t", 1) | ("requestOptionsVal", _) => "requestOptionsVal".to_string(),
            ("t", _) => "adapterConfig".to_string(),
            (other, _) => plain_name(other),
        }
    });
    let code = out.code.as_deref().expect("shipped");
    assert!(
        code.contains("function (adapterConfig)"),
        "the re-asked clean name lands:\n{code}"
    );
    let reasks: Vec<_> = barrier_reasks(&out)
        .into_iter()
        .filter(|d| d.request.identifiers == ["t"])
        .collect();
    assert_eq!(reasks.len(), 2, "two disclosed re-asks of the lone `t`");
    assert!(
        reasks[0].user_prompt.contains(
            "- \"t\" was suggested as \"requestOptions\" but that name is already used by a variable in an enclosing scope"
        ),
        "re-ask 1 names the holder: {}",
        reasks[0].user_prompt
    );
    let second = &reasks[1].user_prompt;
    assert!(
        second.contains(
            "- \"t\" was suggested as \"requestOptionsVal\" but that name is already used by an inner function's parameter"
        ),
        "re-ask 2 says the nested parameter holds it: {second}"
    );
    assert!(
        second.contains("DO NOT suggest these names: requestOptions, requestOptionsVal"),
        "the accumulated do-not list: {second}"
    );
}

/// The sibling shape (r1 2.1.118 `w` → `error`): a lone identifier whose
/// answer a binding of the SAME scope holds: the inner function `error`
/// is named (kept) in an earlier wave, so `w` is asked alone.
const SIBLING_PROGRAM: &str = "var runTask = function (w) {\n  function error() {\n    return w.err;\n  }\n  return error() ? null : w();\n};\nconsole.log(runTask);\n";

/// Re-asked with the holder named; the second answer lands instead of
/// `errorVal`.
#[test]
fn a_lone_answer_held_by_a_sibling_is_reasked_not_decorated() {
    let out = run_scripted(SIBLING_PROGRAM, |id, r| match id {
        "w" if r.prior_rejects.is_some() => "taskFn".to_string(),
        "w" | "error" => "error".to_string(),
        other => plain_name(other),
    });
    let code = out.code.as_deref().expect("shipped");
    assert!(code.contains("function (taskFn)"), "{code}");
    assert!(!code.contains("errorVal"), "never decorated: {code}");
    let reasks = barrier_reasks(&out);
    assert_eq!(reasks.len(), 1, "one disclosed re-ask, then the clean name");
    assert!(
        reasks[0].user_prompt.contains(
            "- \"w\" was suggested as \"error\" but that name is already used by another function in the same scope"
        ),
        "{}",
        reasks[0].user_prompt
    );
    assert_eq!(out.processor.collision_handoffs, 1);
}

/// `--rename-retries 0`: no re-ask exists, so the lone collision settles
/// on the ladder exactly as before the fix.
#[test]
fn a_zero_reask_budget_ladders_a_lone_collision_as_before() {
    let mut config = plain_config();
    config.tunables.reask_limit = 0;
    let out = crate::naming::driver::run_naming(
        &crate::naming::driver::NamingInput {
            fresh: SIBLING_PROGRAM,
            prior: None,
            library: None,
        },
        &config,
        &ScriptProvider {
            answer: |id: &str, _: &humanify_model::llm::BatchRenameRequest| match id {
                "w" | "error" => "error".to_string(),
                other => plain_name(other),
            },
        },
        &mut retain_log(),
    )
    .expect("the stage runs");
    let code = out.code.as_deref().expect("shipped");
    assert!(code.contains("function (errorVal)"), "{code}");
    assert!(barrier_reasks(&out).is_empty());
}
