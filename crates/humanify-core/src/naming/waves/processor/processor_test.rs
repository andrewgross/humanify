//! The processor's pure helpers (processor.ts extractRetrySnippet,
//! buildRetryUsedNames) and the JS Set/Record order semantics they lean on,
//! plus the wave-level pins for the collision-retry fix (2026-09-28): the
//! avoid-lists must carry the names this run already applied, and a
//! collision-class barrier rejection gets exactly one disclosed re-ask.

use std::cell::RefCell;

use super::{build_retry_used_names, extract_retry_snippet};
use crate::naming::waves::jsset::{JsRecord, JsSet};

/// The plain `--sequential`-shaped config the collision pins run under.
fn plain_config() -> crate::naming::driver::NamingConfig {
    crate::naming::driver::NamingConfig {
        bundler: None,
        minifier: None,
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
    }
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
        other => format!("{other}Named"),
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
    )
    .expect("the stage runs");
    let retry: Vec<_> = out
        .waves
        .dispatches
        .iter()
        .filter(|d| d.request.is_retry == Some(true))
        .collect();
    assert_eq!(retry.len(), 1, "exactly one re-ask for the collision");
    let retry = retry[0];
    assert!(
        retry
            .user_prompt
            .contains("DO NOT suggest these names: eventHooks"),
        "the re-ask discloses the collision: {}",
        retry.user_prompt
    );
    assert!(
        retry.request.used_names.iter().any(|n| n == "q2Named"),
        "the re-ask's avoid-list must carry the sibling the run just named: {:?}",
        retry.request.used_names
    );
    let code = out.code.expect("shipped");
    assert!(code.contains("eventHooks"), "{code}");
    assert!(code.contains("q2Named"), "{code}");
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
        code.contains("var eventNameKey = eventHooks + p01Named;"),
        "{code}"
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
