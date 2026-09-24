//! Validated rename's tests, ported from `src/rename/validated-rename.test.ts`
//! and `src/rename/scope-era.test.ts`.
//!
//! The behavior layer is the TS probe (`test/parity/wp31-rename-probe.mjs`
//! over `wp31-rename-scenarios.mjs`, frozen at `test/parity/wp31-rename.json`):
//! every TS test case is a scenario there — the same program, the same
//! calls, run on the REAL TS functions — plus probes of the subtle
//! predicates (class-declaration aliases, catch params, `arguments`,
//! function/class expression names, Annex-B block functions, the export
//! restructure, pattern params, the switch discriminant, computed method
//! keys). `rename_scenarios_match_the_ts_probe` replays each against the
//! overlay and asserts every verdict AND the final name of every binding.
//!
//! What the replay cannot express is ported as unit tests below, each
//! quoting its TS case.

use serde_json::{Value, json};

use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::rename::validated::test_support::with_semantic;
use crate::rename::validated::{
    RejectionReason, RenameAttempt, RenameMode, RenameRequest, RenameState, TrailSpec,
};
use crate::trail::{Anchor, Outcome, Tier};

// ---------------------------------------------------------------------------
// The scenario replay
// ---------------------------------------------------------------------------

struct Replay<'s> {
    state: &'s mut RenameState,
    captured: Vec<(String, Option<BindingId>)>,
}

impl Replay<'_> {
    /// Bindings with crawl name `name`, by declaration position.
    fn by_crawl_name(&self, name: &str, nth: usize) -> BindingId {
        let view = self.state.view();
        let mut ids: Vec<BindingId> = (0..view.bindings.len() as u32)
            .map(BindingId)
            .filter(|b| view.binding(*b).name == name)
            .collect();
        ids.sort_by_key(|b| view.binding(*b).id_span.start);
        *ids.get(nth).unwrap_or_else(|| {
            let all: Vec<&str> = view.bindings.iter().map(|b| b.name.as_str()).collect();
            panic!("no binding {name:?} #{nth} among {all:?}")
        })
    }

    /// Scopes in the probe's order: block start ascending, end descending.
    fn sorted_scopes(&self) -> Vec<BScopeId> {
        let view = self.state.view();
        let mut ids: Vec<BScopeId> = (0..view.scopes.len() as u32).map(BScopeId).collect();
        ids.sort_by_key(|s| {
            let span = view.scope(*s).span;
            (span.start, std::cmp::Reverse(span.end))
        });
        ids
    }

    fn scope_of(&self, sel: &Value) -> BScopeId {
        if sel == "program" {
            return self.state.view().program_scope();
        }
        let nth = sel["nth"].as_u64().unwrap_or(0) as usize;
        if let Some(i) = sel["fn"].as_u64() {
            let view = self.state.view();
            return self
                .sorted_scopes()
                .into_iter()
                .filter(|s| view.scope(*s).ty.is_function())
                .nth(i as usize)
                .expect("function scope");
        }
        let owner = sel["owner"].as_str().expect("owner selector");
        self.state.scope_of_binding(self.by_crawl_name(owner, nth))
    }

    fn binding_of(&self, sel: &Value) -> Option<BindingId> {
        if let Some(key) = sel["captured"].as_str() {
            return self
                .captured
                .iter()
                .find(|(k, _)| k == key)
                .and_then(|(_, b)| *b);
        }
        let nth = sel["nth"].as_u64().unwrap_or(0) as usize;
        Some(self.by_crawl_name(sel["binding"].as_str().expect("binding"), nth))
    }

    fn verdict(attempt: RenameAttempt) -> Value {
        json!({ "applied": attempt.applied, "reason": attempt.reason.map(|r| r.as_str()) })
    }

    fn apply(&mut self, op: &Value) -> Value {
        let s = |k: &str| op[k].as_str().expect("string field").to_string();
        match op["op"].as_str().expect("op") {
            "capture" => {
                let scope = self.scope_of(&op["scope"]);
                let b = self.state.binding_in(scope, &s("name"));
                self.captured.push((s("as"), b));
                json!({ "op": "capture" })
            }
            "attempt" => {
                let scope = self.scope_of(&op["scope"]);
                let expected = if op["expected"].is_null() {
                    None
                } else {
                    self.binding_of(&op["expected"])
                };
                let (old, new) = (s("old"), s("new"));
                let request = RenameRequest {
                    scope,
                    old_name: &old,
                    new_name: &new,
                    expected,
                };
                Self::verdict(
                    self.state
                        .attempt_validated_rename(request, TrailSpec::settling(Tier::ExactMatch)),
                )
            }
            "rejection" => {
                let scope = self.scope_of(&op["scope"]);
                let r = self.state.get_rename_rejection(scope, &s("old"), &s("new"));
                json!({ "rejection": r.map(|r| r.as_str()) })
            }
            "shadow" => {
                let inner = self.binding_of(&op["inner"]).expect("inner");
                let owner = self.binding_of(&op["owner"]).expect("owner");
                Self::verdict(self.state.attempt_shadowing_rename(
                    inner,
                    owner,
                    &s("new"),
                    TrailSpec::post_pass(Tier::ClassIdFloor),
                ))
            }
            "exportFlags" => {
                let b = self.binding_of(&op["binding"]).expect("binding");
                json!({
                    "involved": self.state.is_export_involved(b),
                    "declarationId": self.state.is_export_declaration_id(b),
                })
            }
            other => panic!("unknown op {other}"),
        }
    }
}

fn final_names(state: &RenameState) -> Value {
    let view = state.view();
    let mut ids: Vec<BindingId> = (0..view.bindings.len() as u32).map(BindingId).collect();
    ids.sort_by_key(|b| view.binding(*b).id_span.start);
    Value::Array(
        ids.into_iter()
            .map(|b| {
                let span = view.binding(b).id_span;
                json!({ "id": [span.start, span.end], "name": state.name_of(b) })
            })
            .collect(),
    )
}

/// Every TS test case of validated-rename.test.ts and scope-era.test.ts,
/// plus the extra predicate probes: each verdict and every binding's final
/// name, exactly as the real TS functions produced them.
#[test]
fn rename_scenarios_match_the_ts_probe() {
    let raw = include_str!("../../../../../test/parity/wp31-rename.json");
    let probe: Value = serde_json::from_str(raw).expect("fixture parses");
    let scenarios = probe["scenarios"].as_array().expect("scenarios");
    assert!(scenarios.len() >= 59, "the scenario set shrank");
    let mut failures = Vec::new();
    for scenario in scenarios {
        let spec = scenario;
        assert!(scenario["error"].is_null(), "TS threw: {scenario}");
        let module = spec["sourceType"].as_str().unwrap_or("module") == "module";
        let code = spec["code"].as_str().expect("code");
        let (results, names) = with_semantic(code, module, |semantic| {
            let mut state = RenameState::new(semantic, Anchor::Fresh);
            let mut replay = Replay {
                state: &mut state,
                captured: Vec::new(),
            };
            let results: Vec<Value> = spec["ops"]
                .as_array()
                .expect("ops")
                .iter()
                .map(|op| replay.apply(op))
                .collect();
            (Value::Array(results), final_names(&state))
        });
        if results != scenario["results"] || names != scenario["finalNames"] {
            failures.push(format!(
                "--- {}\n  ts:   {} {}\n  rust: {} {}",
                scenario["label"], scenario["results"], scenario["finalNames"], results, names
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// ---------------------------------------------------------------------------
// Unit tests the replay cannot express
// ---------------------------------------------------------------------------

fn program_state<R>(code: &str, f: impl FnOnce(&mut RenameState) -> R) -> R {
    with_semantic(code, true, |semantic| {
        let mut state = RenameState::new(semantic, Anchor::Fresh);
        f(&mut state)
    })
}

fn attempt(state: &mut RenameState, scope: BScopeId, old: &str, new: &str) -> RenameAttempt {
    state.attempt_validated_rename(
        RenameRequest {
            scope,
            old_name: old,
            new_name: new,
            expected: None,
        },
        TrailSpec::settling(Tier::ExactMatch),
    )
}

/// TS: "fastRenameBinding input guard — throws when handed a builtin target
/// — callers must validate first". The mutation primitive refuses an
/// unvalidated target loudly.
#[test]
#[should_panic(expected = "invalid rename target")]
fn the_mutation_primitive_refuses_a_builtin_target() {
    program_state("var a = 1;", |state| {
        let p = state.view().program_scope();
        let b = state.binding_in(p, "a").expect("a");
        state.rebind(p, "a", "document", b);
    });
}

/// TS: "fastRenameBinding input guard — throws when handed a reserved word
/// target".
#[test]
#[should_panic(expected = "invalid rename target")]
fn the_mutation_primitive_refuses_a_reserved_word_target() {
    program_state("var a = 1;", |state| {
        let p = state.view().program_scope();
        let b = state.binding_in(p, "a").expect("a");
        state.rebind(p, "a", "class", b);
    });
}

/// TS: scope-era "stays at zero for ordinary renames — no eras, nothing to
/// flip": one applied rename records one claim; the ledger-only counter is
/// zero (in the Rust, structurally — one scope table).
#[test]
fn claims_count_applied_renames_and_the_ledger_counter_stays_zero() {
    program_state("function f() { let aa = 1; use(aa); }", |state| {
        let view_scope = state
            .view()
            .scopes
            .iter()
            .position(|s| s.ty.is_function())
            .expect("function scope");
        let scope = BScopeId(view_scope as u32);
        assert!(attempt(state, scope, "aa", "dirPath").applied);
        let stats = state.claim_stats();
        assert_eq!(stats.ledger_only_rejections, 0);
        assert_eq!(stats.claims_recorded, 1);
    });
}

/// TS: scope-era "attributes the flip to the guard that made it". The TS
/// needs the claim ledger because TWO scope trees coexist; the headline
/// capture's verdict (the outer rename rejected as shadows-child — asserted
/// by the replay's era scenario) is what the ledger restores. In the Rust
/// there is one table, so the verdict comes from the maps themselves and
/// no guard is ever attributed to a ledger: every by-guard counter is zero.
#[test]
fn the_era_capture_is_rejected_without_any_ledger_attribution() {
    let code = "\nfunction getFileWriter() {\n  let outerDir = null;\n  register({ writeFn: (task) => {\n    let innerDir = dirname(getPath());\n    let changed = outerDir !== innerDir;\n    outerDir = innerDir;\n    return changed;\n  }});\n}";
    with_semantic(code, false, |semantic| {
        let mut state = RenameState::new(semantic, Anchor::Fresh);
        let owner_of = |state: &RenameState, name: &str| {
            let view = state.view();
            let b = (0..view.bindings.len() as u32)
                .map(BindingId)
                .find(|b| view.binding(*b).name == name)
                .expect("binding");
            state.scope_of_binding(b)
        };
        let inner = owner_of(&state, "innerDir");
        let outer = owner_of(&state, "outerDir");
        assert!(attempt(&mut state, inner, "innerDir", "dirPath").applied);
        let second = attempt(&mut state, outer, "outerDir", "dirPath");
        assert_eq!(second.reason, Some(RejectionReason::ShadowsChild));
        let stats = state.claim_stats();
        assert_eq!(stats.ledger_only_rejections, 0);
        assert_eq!(stats.by_guard.shadows_child, 0);
        assert_eq!(stats.by_guard.target_in_scope, 0);
        assert_eq!(stats.by_guard.target_visible, 0);
    });
}

/// The applier records the TS's standard row (`transferOwnedPair`): the
/// binding under the old name, `{tier, applied|rejected, reason, newName}`;
/// a missing binding (no-binding) records nothing, as `if (trailBinding)`.
#[test]
fn the_applier_records_the_standard_trail_row() {
    program_state("var a = 1; var b = 2;", |state| {
        let p = state.view().program_scope();
        assert!(!attempt(state, p, "a", "b").applied);
        assert!(attempt(state, p, "a", "loaded").applied);
        assert!(!attempt(state, p, "missing", "x").applied);
        let entries = state.trail().entries();
        assert_eq!(entries.len(), 1, "no-binding has no trail binding");
        let e = &entries[0];
        assert_eq!(e.old_name, "a");
        assert_eq!(e.settled_by, Some(Tier::ExactMatch));
        assert_eq!(e.final_name.as_deref(), Some("loaded"));
        assert_eq!(e.attempts[0].outcome, Outcome::Rejected);
        assert_eq!(e.attempts[0].reason.as_deref(), Some("target-in-scope"));
        assert_eq!(e.attempts[0].proposed_name.as_deref(), Some("b"));
        assert_eq!(e.attempts[1].outcome, Outcome::Applied);
        assert_eq!(e.attempts[1].reason, None);
    });
}

/// Opting out of the standard row is counted (02 §5: a tier cannot run
/// untrailed unseen).
#[test]
fn trail_opt_outs_are_counted() {
    program_state("var a = 1; var b = 2;", |state| {
        let p = state.view().program_scope();
        let r = state.attempt_validated_rename(
            RenameRequest {
                scope: p,
                old_name: "a",
                new_name: "x",
                expected: None,
            },
            TrailSpec::Untrailed { why: "uniquify" },
        );
        assert!(r.applied);
        let r = state.attempt_validated_rename(
            RenameRequest {
                scope: p,
                old_name: "b",
                new_name: "y",
                expected: None,
            },
            TrailSpec::CallerRecords {
                tier: Tier::ClassIdFloor,
            },
        );
        assert!(r.applied);
        assert!(state.trail().entries().is_empty());
        assert_eq!(state.opt_outs().untrailed.get("uniquify"), Some(&1));
        assert_eq!(
            state.opt_outs().caller_records.get(&Tier::ClassIdFloor),
            Some(&1)
        );
    });
}

/// exp066 provenance rule (`carriedNames.record` in attemptValidatedRename):
/// a below-floor name APPLIED is carried; a descriptive one is not.
#[test]
fn below_floor_applied_names_are_carried() {
    program_state("var a = 1, b = 2, c = 3;", |state| {
        let p = state.view().program_scope();
        assert!(attempt(state, p, "a", "q7").applied);
        assert!(attempt(state, p, "b", "fsPromises_").applied);
        assert!(attempt(state, p, "c", "count").applied);
        let carried = |state: &RenameState, n: &str| {
            let b = state.binding_in(p, n).expect("binding");
            state.is_carried(b)
        };
        assert!(carried(state, "q7"));
        assert!(
            !carried(state, "fsPromises_"),
            "decorated-descriptive is exempt"
        );
        assert!(!carried(state, "count"));
        assert_eq!(state.carried_count(), 1);
    });
}

/// Babel's map order is a decision input downstream: a renamed name moves
/// to the END of `Object.keys(scope.bindings)`.
#[test]
fn a_rename_moves_the_name_to_the_end_of_the_map_order() {
    program_state("var a = 1, b = 2, c = 3;", |state| {
        let p = state.view().program_scope();
        assert!(attempt(state, p, "a", "x").applied);
        let names: Vec<String> = state.bindings_in(p).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["b", "c", "x"]);
    });
}

/// The apply log carries the mode the render needs: in place, or Babel's
/// renamer (splitting `export const` the first time).
#[test]
fn the_apply_log_records_the_render_mode() {
    program_state(
        "export const a = 1, b = 2; export function f(p) { return p; }",
        |state| {
            let p = state.view().program_scope();
            assert!(attempt(state, p, "a", "first").applied);
            assert!(attempt(state, p, "b", "second").applied);
            assert!(attempt(state, p, "f", "fn1").applied);
            let modes: Vec<RenameMode> = state.applied_renames().iter().map(|a| a.mode).collect();
            assert_eq!(
                modes,
                [
                    RenameMode::BabelRenamer {
                        splits_export: true
                    },
                    RenameMode::InPlace,
                    RenameMode::InPlace,
                ]
            );
        },
    );
}

/// The overlay is the only name source: `finish` hands the render every
/// renamed symbol with its final name.
#[test]
fn finish_hands_the_render_every_renamed_symbol() {
    program_state("var a = 1; function f(p) { return p + a; }", |state| {
        let p = state.view().program_scope();
        assert!(attempt(state, p, "a", "count").applied);
        let f_scope = state.scope_of_binding(state.get_binding(p, "f").expect("f"));
        assert_eq!(f_scope, p);
    });
    with_semantic(
        "var a = 1; function f(p) { return p + a; }",
        true,
        |semantic| {
            let mut state = RenameState::new(semantic, Anchor::Fresh);
            let p = state.view().program_scope();
            assert!(attempt(&mut state, p, "a", "count").applied);
            assert!(attempt(&mut state, p, "a", "x").reason == Some(RejectionReason::NoBinding));
            let outcome = state.finish();
            let names: Vec<(&str, &str)> = outcome
                .symbol_names
                .iter()
                .map(|(s, n)| (semantic.scoping().symbol_name(*s), n.as_str()))
                .collect();
            assert_eq!(names, [("a", "count")]);
            assert_eq!(outcome.claims.claims_recorded, 1);
        },
    );
}

/// The rename freeze's consumer (`isBindingEvalTaintFrozen`, soundness.ts)
/// against the TS probe (`test/parity/wp31-soundness-probe.mjs`, frozen at
/// `test/parity/wp31-soundness.json`): every binding of every snippet —
/// with / direct eval / locally bound eval / class methods / static blocks
/// (the TS taints FUNCTIONS via the path's getFunctionParent but asks the
/// binding's SCOPE getFunctionParent, which stops at a static block — so a
/// static-block binding is never frozen) / object methods / arrows.
#[test]
fn eval_taint_freeze_matches_the_ts_probe() {
    let raw = include_str!("../../../../../test/parity/wp31-soundness.json");
    let probe: Value = serde_json::from_str(raw).expect("fixture parses");
    let mut failures = Vec::new();
    for case in probe["cases"].as_array().expect("cases") {
        let code = case["code"].as_str().expect("code");
        let mine = with_semantic(code, false, |semantic| {
            let taint = crate::modules::soundness::collect_eval_with_taint(semantic);
            let state = RenameState::new(semantic, Anchor::Fresh);
            let view = state.view();
            let mut ids: Vec<BindingId> = (0..view.bindings.len() as u32).map(BindingId).collect();
            ids.sort_by_key(|b| view.binding(*b).id_span.start);
            let rows: Vec<Value> = ids
                .into_iter()
                .map(|b| {
                    let span = view.binding(b).id_span;
                    json!({
                        "id": [span.start, span.end],
                        "name": state.name_of(b),
                        "frozen": state.is_eval_taint_frozen(b, &taint),
                    })
                })
                .collect();
            (taint.site_count, Value::Array(rows))
        });
        if json!(mine.0) != case["siteCount"] || mine.1 != case["frozen"] {
            failures.push(format!(
                "{code}\n  ts:   {}\n  rust: {}",
                case["frozen"], mine.1
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A REAL TS bug, reproduced on purpose (the port matches the oracle; the
/// fix is a TS-side decision — `test/parity/wp31-catch-var-capture-repro.mjs`
/// executes both versions: 5 before, undefined after). Renaming a catch
/// parameter to the name of a `var` declared in its own body is APPLIED:
/// Annex B lets `var x` redeclare the param, the declaration hoists out, but
/// its initializer runs inside the catch block against the param — the
/// outer `x` is never assigned. `wouldCaptureOuterReference` sees only the
/// outer binding's references and violations, and a binding's own first
/// declaration is neither.
#[test]
fn the_catch_var_capture_is_reproduced_not_fixed() {
    let code = "function f() { try { throw 5; } catch (err) { var x = err; } return x; }";
    with_semantic(code, false, |semantic| {
        let mut state = RenameState::new(semantic, Anchor::Fresh);
        let catch_scope = state
            .view()
            .scopes
            .iter()
            .position(|s| s.ty == crate::rename::validated::scopes::ScopeType::CatchClause)
            .map(|i| BScopeId(i as u32))
            .expect("catch scope");
        let applied = attempt(&mut state, catch_scope, "err", "x");
        assert!(applied.applied, "the TS applies it: {applied:?}");
    });
}
