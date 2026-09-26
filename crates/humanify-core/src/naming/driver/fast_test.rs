//! `--fast` (docs/rust-port/20-fast-mode.md): the pipelined LLM dispatch
//! must be DETERMINISTIC under any completion order. The provider here
//! completes calls in the worst order it can (last started, first done),
//! so a lane's follow-up races the rest of its round; the run must ship
//! the bytes — and record the dispatches — of the turn-by-turn driver.

use std::cell::Cell;

use humanify_model::llm::{
    BatchRenameResponse, LlmCall, LlmError, NameProvider, OnCallDone, Renames,
};

use super::{NamingConfig, NamingInput, NamingOutcome, run_naming};

/// Answers every identifier with one of three words (by its first
/// letter), so names collide inside a function and lanes retry. Pipelined
/// runs complete calls LIFO and record whether a follow-up call ever
/// started while an earlier-started call was still in flight.
#[derive(Default)]
struct LifoProvider {
    overlapped: Cell<bool>,
    calls: Cell<usize>,
}

fn answer(call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
    const WORDS: [&str; 3] = ["item", "value", "node"];
    Ok(BatchRenameResponse {
        renames: Renames::from_entries(call.request.identifiers.iter().map(|i| {
            let w = WORDS[usize::from(i.as_bytes()[0]) % WORDS.len()];
            (i.clone(), Some(format!("{w}{}", i.len())))
        })),
        finish_reason: None,
        usage: None,
    })
}

impl NameProvider for LifoProvider {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        self.calls.set(self.calls.get() + calls.len());
        calls.iter().map(answer).collect()
    }

    fn run_pipelined(&self, initial: Vec<(usize, LlmCall)>, on_done: &mut OnCallDone<'_>) {
        // In flight, as a stack: the newest call completes first.
        let mut in_flight: Vec<(usize, LlmCall)> = initial;
        let initial_len = in_flight.len();
        let mut done = 0usize;
        while let Some((id, call)) = in_flight.pop() {
            self.calls.set(self.calls.get() + 1);
            done += 1;
            let follow = on_done(id, answer(&call));
            if !follow.is_empty() && done < initial_len && !in_flight.is_empty() {
                self.overlapped.set(true);
            }
            in_flight.extend(follow);
        }
    }
}

fn config(fast: bool) -> NamingConfig {
    NamingConfig {
        bundler: None,
        minifier: None,
        skip_libraries: true,
        reconcile_prior_diff: true,
        naming_floor: true,
        naming_floor_sweep: true,
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
        // Small windows: every lane needs several calls.
        tunables: crate::naming::waves::batch::WaveTunables {
            batch_size: 3,
            ..Default::default()
        },
        shingle_probe: false,
        fast,
    }
}

/// A program with one function big enough for four lanes, a few small
/// functions calling each other (several waves) and module bindings.
fn fixture() -> String {
    let names: Vec<String> = (0..40)
        .map(|i| format!("{}{}", char::from(b'a' + (i % 26) as u8), i / 26))
        .collect();
    let decls: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(i, n)| format!("  var {n} = q + {i};"))
        .collect();
    let sum = names.join(" + ");
    format!(
        "var m0 = 1, m1 = 2, m2 = 3;\nfunction h(q) {{\n{}\n  return {sum};\n}}\n\
         function g(x, y) {{\n  var z = x * y;\n  return h(z) + m0;\n}}\n\
         function f(p) {{\n  var r = g(p, m1);\n  var s = g(r, m2);\n  return r + s;\n}}\n\
         console.log(f(4));\n",
        decls.join("\n")
    )
}

fn run(fresh: &str, fast: bool, provider: &LifoProvider) -> NamingOutcome {
    run_naming(
        &NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        &config(fast),
        provider,
    )
    .expect("the stage runs")
}

/// What a run decided and recorded, in comparable form.
fn fingerprint(out: &NamingOutcome) -> (String, Vec<String>, String) {
    let dispatches = out
        .waves
        .dispatches
        .iter()
        .map(|d| {
            format!(
                "{} {} {} {} {}",
                d.seq, d.function_id, d.round, d.wave, d.cache_key
            )
        })
        .collect();
    (
        out.code.clone().expect("shipped"),
        dispatches,
        format!("{:?}", out.processor),
    )
}

#[test]
fn pipelined_dispatch_ships_the_turn_drivers_bytes_under_lifo_completion() {
    let fresh = fixture();
    let parity = run(&fresh, false, &LifoProvider::default());
    let provider = LifoProvider::default();
    let fast = run(&fresh, true, &provider);
    assert!(
        provider.overlapped.get(),
        "the fast run never pipelined: no follow-up started while its round was in flight"
    );
    assert_eq!(fingerprint(&fast), fingerprint(&parity));
    assert!(
        parity.waves.dispatches.len() > 10,
        "the fixture exercises lanes"
    );
}

#[test]
fn a_fast_run_is_byte_identical_to_itself() {
    let fresh = fixture();
    let a = run(&fresh, true, &LifoProvider::default());
    let b = run(&fresh, true, &LifoProvider::default());
    assert_eq!(fingerprint(&a), fingerprint(&b));
}

/// With a prior: the prior match, the close contexts (mapped on the pool
/// under `--fast`) and the speculative reconcile beside the verdict must
/// ship the parity bytes too.
#[test]
fn with_a_prior_fast_ships_the_parity_bytes() {
    let fresh = fixture();
    // The prior release: humanified names, one function body different
    // (a close match), one extra statement.
    let prior = fixture()
        .replace(
            "function g(x, y) {\n  var z = x * y;",
            "function combine(left, right) {\n  var product = left * right + 0;",
        )
        .replace("return h(z) + m0;", "return h(product) + m0;")
        .replace(
            "var r = g(p, m1);\n  var s = g(r, m2);",
            "var r = combine(p, m1);\n  var s = combine(r, m2);",
        )
        .replace("console.log(f(4));", "console.log(f(4));\nconsole.log(1);");
    let run_with = |fast: bool| {
        run_naming(
            &NamingInput {
                fresh: &fresh,
                prior: Some(&prior),
                library: None,
            },
            &config(fast),
            &LifoProvider::default(),
        )
        .expect("the stage runs")
    };
    let parity = run_with(false);
    let fast = run_with(true);
    assert!(parity.reconcile.is_some(), "the reconcile ran");
    let counts = parity.prior.as_ref().expect("a prior run").counts;
    assert!(counts.close_match_count > 0, "a close match: {counts:?}");
    assert_eq!(fingerprint(&fast), fingerprint(&parity));
    assert_eq!(
        fast.reconcile.as_ref().map(|r| &r.code),
        parity.reconcile.as_ref().map(|r| &r.code)
    );
}
