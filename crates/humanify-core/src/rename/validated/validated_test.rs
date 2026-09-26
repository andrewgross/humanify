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
    RejectionReason, RenameAttempt, RenameRequest, RenameState, TrailSpec,
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

/// The probe scenarios where the Rust DELIBERATELY differs from the TS
/// since finding #55 (post-cutover): the TS renamed a named export's own
/// binding (`export function mitt` in place — the module's API changed;
/// `export const a` by splitting the declaration — #16's dead-name class).
/// The Rust refuses both (`exported-name`); the unit tests at the end of
/// this file hold the new behavior, every other scenario still replays.
const DIVERGES_FROM_THE_TS_55: &[&str] = &[
    "vr: preserves the external name when renaming an exported binding",
    "vr: keeps the named export declaration form when renaming its id",
    "x: export var restructure — the Babel renamer path, then the fast path",
];

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
    for label in DIVERGES_FROM_THE_TS_55 {
        assert!(
            scenarios.iter().any(|s| s["label"] == *label),
            "stale divergence label {label}"
        );
    }
    for scenario in scenarios {
        if DIVERGES_FROM_THE_TS_55.contains(&scenario["label"].as_str().unwrap_or("")) {
            continue;
        }
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

/// 16-findings-queue #15, FIXED TS-first (2026-09-25): renaming a catch
/// parameter to the name of an initialized `var` declared in its own body
/// was APPLIED — Annex B hoists the declaration, but its initializer runs in
/// the catch against the renamed param, so the outer `x` was never assigned
/// (`test/parity/wp31-catch-var-capture-repro.mjs`: 5 before, undefined
/// after). The guard now treats the outer binding's own initialized
/// declarator inside the block as a write; an UNinitialized `var` writes
/// nothing and stays allowed (the TS negative control).
#[test]
fn the_catch_var_capture_is_rejected() {
    let cases = [
        (
            "function f() { try { throw 5; } catch (err) { var x = err; } return x; }",
            false,
        ),
        (
            "function f() { try { throw 5; } catch (err) { var x; use(err); } return x; }",
            true,
        ),
    ];
    for (code, allowed) in cases {
        with_semantic(code, false, |semantic| {
            let mut state = RenameState::new(semantic, Anchor::Fresh);
            let catch_scope = state
                .view()
                .scopes
                .iter()
                .position(|s| s.ty == crate::rename::validated::scopes::ScopeType::CatchClause)
                .map(|i| BScopeId(i as u32))
                .expect("catch scope");
            let result = attempt(&mut state, catch_scope, "err", "x");
            assert_eq!(result.applied, allowed, "{code}: {result:?}");
            if !allowed {
                assert_eq!(
                    result.reason.map(RejectionReason::as_str),
                    Some("target-visible"),
                    "{code}"
                );
            }
        });
    }
}

/// The waves' fresh-era re-crawl (WP4.3): a rename moves the name to the
/// END of Babel's map; a re-crawl (a fresh Scope object after the
/// prior-match cache clear) lists registration order under current names.
#[test]
fn a_recrawl_restores_registration_order_under_current_names() {
    program_state("{ let a = 1; let b = 2; let c = 3; }", |state| {
        let block = (0..state.view().scopes.len())
            .map(|i| BScopeId(i as u32))
            .find(|&s| state.bindings_in(s).len() == 3)
            .expect("the block scope");
        assert!(attempt(state, block, "a", "renamed").applied);
        let names = |state: &RenameState| -> Vec<String> {
            state
                .bindings_in(block)
                .into_iter()
                .map(|(n, _)| n)
                .collect()
        };
        assert_eq!(names(state), ["b", "c", "renamed"]);
        state.recrawl_scopes(&[block]);
        assert_eq!(names(state), ["renamed", "b", "c"]);
        assert!(attempt(state, block, "b", "second").applied);
        assert_eq!(names(state), ["renamed", "c", "second"]);
    });
}

/// Babel's crawl adds free ASSIGNMENT targets to `globals` first, then the
/// unresolved references — the insertion order the waves' used names read.
#[test]
fn program_globals_keep_babels_insertion_order() {
    with_semantic("use(zeta); later = 1; alpha();", false, |semantic| {
        let state = RenameState::new(semantic, Anchor::Fresh);
        assert_eq!(
            state.view().globals_order,
            ["later", "use", "zeta", "alpha"]
        );
    });
}

// ---------------------------------------------------------------------------
// Finding #55: an export name is the module's API — it never changes
// ---------------------------------------------------------------------------

/// Attempt `old -> new` in the program scope.
fn program_attempt(code: &str, old: &str, new: &str) -> RenameAttempt {
    program_state(code, |state| {
        let p = state.view().program_scope();
        attempt(state, p, old, new)
    })
}

fn assert_exported_name_refused(code: &str, old: &str) {
    let got = program_attempt(code, old, "renamedLocal");
    assert_eq!(
        got,
        RenameAttempt {
            applied: false,
            reason: Some(RejectionReason::ExportedName)
        },
        "{old} in {code:?}"
    );
}

/// `export function f` / `export class C`: the id IS the export name.
#[test]
fn an_exported_function_or_class_declaration_keeps_its_name() {
    assert_exported_name_refused(
        "export function createStore(e) { return e; }",
        "createStore",
    );
    assert_exported_name_refused("export class Counter {}", "Counter");
    assert_exported_name_refused("export async function* gen() {}", "gen");
}

/// `export var/let/const`, single, multi-declarator (#16's zustand shape)
/// and destructured: every declared binding IS an export name.
#[test]
fn an_exported_variable_keeps_its_name() {
    assert_exported_name_refused("export const a = 1;", "a");
    assert_exported_name_refused("export let m = 1; m = 2;", "m");
    assert_exported_name_refused("export var v;", "v");
    assert_exported_name_refused("export const a = 1, b = 2;", "a");
    assert_exported_name_refused("export const a = 1, b = 2;", "b");
    let destructured = "export const { x, y: z } = o, [w] = p;";
    assert_exported_name_refused(destructured, "x");
    assert_exported_name_refused(destructured, "z");
    assert_exported_name_refused(destructured, "w");
}

/// The scope that binds `name` (the one binding of that crawl name).
fn scope_binding(state: &RenameState, name: &str) -> BScopeId {
    let owners: Vec<BScopeId> = state
        .view()
        .bindings
        .iter()
        .filter(|b| b.name == name)
        .map(|b| b.owner)
        .collect();
    assert_eq!(owners.len(), 1, "one binding named {name}");
    owners[0]
}

/// The export's OWN bindings only: a param, a local, a nested function of
/// an exported declaration are ordinary bindings.
#[test]
fn bindings_inside_an_exported_declaration_stay_renameable() {
    program_state(
        "export function f(p) { const q = p; function g() {} return g(q); }\n\
         export const h = (r) => { let s = r; return s; };\n\
         export class K { m(t) { return t; } }",
        |state| {
            for old in ["p", "q", "g", "r", "s", "t"] {
                let scope = scope_binding(state, old);
                let got = attempt(state, scope, old, &format!("{old}Renamed"));
                assert!(got.applied, "{old}: {:?}", got.reason);
            }
        },
    );
}

/// `export default function f` / `class C`: the external name is
/// `default`, so the local id may be renamed.
#[test]
fn an_export_default_declaration_id_is_renameable() {
    assert!(program_attempt("export default function f(e) { return e; }", "f", "make").applied);
    assert!(program_attempt("export default class C {}", "C", "Store").applied);
}

/// A specifier's local (`export { a }`, `export { a as b }`) may be
/// renamed: the specifier keeps the external name (the render's forms).
#[test]
fn a_specifier_local_is_renameable() {
    assert!(program_attempt("const a = 1; export { a };", "a", "count").applied);
    assert!(program_attempt("const a = 1; export { a as b };", "a", "count").applied);
}

/// A re-export binds nothing locally: there is nothing to rename.
#[test]
fn a_reexport_binds_no_local() {
    let no_binding = RenameAttempt {
        applied: false,
        reason: Some(RejectionReason::NoBinding),
    };
    assert_eq!(
        program_attempt("export { x } from \"m\";", "x", "y"),
        no_binding
    );
    assert_eq!(
        program_attempt("export { x as z } from \"m\";", "z", "y"),
        no_binding
    );
    assert_eq!(
        program_attempt("export * as ns from \"m\";", "ns", "y"),
        no_binding
    );
}
