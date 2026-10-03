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
use crate::artifact_dump::{Dispatch, ask_rows, dispatch_rows};
use crate::naming::waves::processor::DEFAULT_PROMPT_WINDOW;

/// Answers every identifier with one of three words (by its first
/// letter), so names collide inside a function and lanes retry. Pipelined
/// runs complete calls LIFO and record whether a follow-up call ever
/// started while an earlier-started call was still in flight.
#[derive(Default)]
struct LifoProvider {
    overlapped: Cell<bool>,
    calls: Cell<usize>,
    /// Dispatch generations with at least one call (a round = one).
    rounds: Cell<usize>,
    /// Lane chains: the longest run of follow-ups from one initial call.
    longest_chain: Cell<usize>,
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
        if !calls.is_empty() {
            self.rounds.set(self.rounds.get() + 1);
        }
        calls.iter().map(answer).collect()
    }

    fn run_pipelined(&self, initial: Vec<(usize, LlmCall)>, on_done: &mut OnCallDone<'_>) {
        if !initial.is_empty() {
            self.rounds.set(self.rounds.get() + 1);
        }
        // In flight, as a stack: the newest call completes first. Each
        // entry carries its chain depth.
        let mut in_flight: Vec<(usize, LlmCall, usize)> =
            initial.into_iter().map(|(id, c)| (id, c, 1)).collect();
        let initial_len = in_flight.len();
        let mut done = 0usize;
        while let Some((id, call, depth)) = in_flight.pop() {
            self.calls.set(self.calls.get() + 1);
            self.longest_chain.set(self.longest_chain.get().max(depth));
            done += 1;
            let follow = on_done(id, answer(&call));
            if !follow.is_empty() && done < initial_len && !in_flight.is_empty() {
                self.overlapped.set(true);
            }
            in_flight.extend(follow.into_iter().map(|(i, c)| (i, c, depth + 1)));
        }
    }
}

fn config(fast: bool) -> NamingConfig {
    config_tier(if fast {
        crate::fast::FastTier::Exact
    } else {
        crate::fast::FastTier::Off
    })
}

fn config_tier(fast: crate::fast::FastTier) -> NamingConfig {
    NamingConfig {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        never_rename: crate::rename::eligibility::NeverRename::UNIVERSAL,
        tuning: crate::toolchain::BundlerTuning::Default,
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
        prompt_window: DEFAULT_PROMPT_WINDOW,
    }
}

/// The retaining test log (finding #65): the full records stay readable
/// for the fingerprints, and the streamed rows land in its in-memory
/// oracle (the bulk builders' byte-identity comparison).
fn retain_log() -> crate::artifact_dump::DispatchLog {
    crate::artifact_dump::DispatchLog::retain_for_tests(
        config_tier(crate::fast::FastTier::Off).params,
    )
}

/// One run over the fixture under `cfg`'s window, through the retaining
/// log.
fn run_cfg(fresh: &str, cfg: &NamingConfig, provider: &dyn NameProvider) -> NamingOutcome {
    run_naming(
        &NamingInput {
            fresh,
            prior: None,
            library: None,
        },
        cfg,
        &provider,
        &mut retain_log(),
    )
    .expect("the stage runs")
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
         function k(u) {{\n  {{\n    let t = u + 1;\n    console.log(t);\n  }}\n  {{\n    let t = u + 2;\n    console.log(t);\n  }}\n  return u;\n}}\n\
         function f(p) {{\n  var r = g(p, m1);\n  var s = g(k(r), m2);\n  return r + s;\n}}\n\
         console.log(f(4));\n",
        decls.join("\n")
    )
}

fn run(fresh: &str, fast: bool, provider: &LifoProvider) -> NamingOutcome {
    run_cfg(fresh, &config(fast), provider)
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
/// under the fast schedule) and the speculative reconcile beside the verdict must
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
        // A fresh log per leg (the reconcile test reads the outcomes, not
        // the accumulated rows).
        let mut log = retain_log();
        run_naming(
            &NamingInput {
                fresh: &fresh,
                prior: Some(&prior),
                library: None,
            },
            &config(fast),
            &LifoProvider::default(),
            &mut log,
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

/// The tier names this file's cases use, resolved directly (the CLI's tier
/// strings went with `--fast`; the schedule is the relaxed tier by default
/// and `--sequential` selects the exact one).
fn tier_of(name: &str) -> crate::fast::FastTier {
    use crate::fast::{FastTier, Levers};
    match name.strip_prefix("relaxed:") {
        Some(list) => FastTier::Relaxed(Levers::parse(list).expect("lever names")),
        None if name == "relaxed" => FastTier::Relaxed(Levers::all()),
        _ => FastTier::Exact,
    }
}

fn run_tier(fresh: &str, tier: &str, provider: &dyn NameProvider) -> NamingOutcome {
    run_cfg(fresh, &config_tier(tier_of(tier)), provider)
}

/// Answers like [`LifoProvider`] but runs every pipelined round
/// GENERATIONALLY (the trait default): the other extreme of completion
/// order.
struct Generational;

impl NameProvider for Generational {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        calls.iter().map(answer).collect()
    }
}

/// Every relaxed lever, alone and together: deterministic (twice -> same),
/// independent of completion order (LIFO vs generational -> same) and a
/// valid output.
#[test]
fn relaxed_levers_are_deterministic_and_order_independent() {
    let fresh = fixture();
    for tier in ["relaxed", "relaxed:window-lanes", "relaxed:defer-shadowed"] {
        let a = run_tier(&fresh, tier, &LifoProvider::default());
        let b = run_tier(&fresh, tier, &LifoProvider::default());
        assert_eq!(fingerprint(&a), fingerprint(&b), "{tier}: twice");
        let g = run_tier(&fresh, tier, &Generational);
        assert_eq!(
            fingerprint(&a),
            fingerprint(&g),
            "{tier}: LIFO vs generational"
        );
        assert!(a.output_valid, "{tier}: {:?}", a.verdict);
    }
}

/// window-lanes: no lane chain is as long as the exact tier's longest.
#[test]
fn window_lanes_shorten_the_longest_chain() {
    let fresh = fixture();
    let exact = LifoProvider::default();
    run_tier(&fresh, "exact", &exact);
    let lanes = LifoProvider::default();
    run_tier(&fresh, "relaxed:window-lanes", &lanes);
    assert!(
        lanes.longest_chain.get() < exact.longest_chain.get(),
        "window-lanes chain {} vs exact {}",
        lanes.longest_chain.get(),
        exact.longest_chain.get()
    );
}

/// defer-shadowed: the shadowed pass costs no round of its own.
#[test]
fn defer_shadowed_saves_rounds() {
    let fresh = fixture();
    let exact = LifoProvider::default();
    // `k`'s second `let t` is a shadowed block binding: a round-B call.
    run_tier(&fresh, "exact", &exact);
    let deferred = LifoProvider::default();
    run_tier(&fresh, "relaxed:defer-shadowed", &deferred);
    assert!(
        deferred.rounds.get() < exact.rounds.get(),
        "defer-shadowed rounds {} vs exact {}",
        deferred.rounds.get(),
        exact.rounds.get()
    );
}

// ---------------------------------------------------------------------------
// Finding #65: the bounded rendered-prompt window, and the per-dispatch
// log's modes
// ---------------------------------------------------------------------------

/// A two-slot window ships the unbounded round's bytes — and never has
/// more than two rendered prompts alive. The pipelined round is
/// completion-order-independent BY DESIGN, and the window only changes
/// when a lane's first call is STARTED (its material is frozen), so the
/// dispatch records, their canonical order and the output cannot move.
#[test]
fn a_bounded_prompt_window_ships_the_unbounded_rounds_bytes() {
    let fresh = fixture();
    let mut unbounded = config_tier(tier_of("relaxed"));
    unbounded.prompt_window = usize::MAX;
    let wide = run_cfg(&fresh, &unbounded, &LifoProvider::default());
    assert!(
        wide.waves.peak_live_dispatches > 10,
        "the fixture fills an unbounded round (peak {})",
        wide.waves.peak_live_dispatches
    );
    let mut bounded = config_tier(tier_of("relaxed"));
    bounded.prompt_window = 2;
    let narrow = run_cfg(&fresh, &bounded, &LifoProvider::default());
    assert_eq!(
        fingerprint(&narrow),
        fingerprint(&wide),
        "the window changes dispatch timing only"
    );
    assert!(
        narrow.waves.peak_live_dispatches <= 2,
        "the bound holds: peak {} > window 2",
        narrow.waves.peak_live_dispatches
    );
    // The fingerprint equality is also the backfill's proof: a refill
    // that never fired would silently drop every lane that never got a
    // slot, and the ask set — and the bytes — could not match.
}

/// The log's streamed rows are BYTE-IDENTICAL to the bulk builders over
/// the same dispatches (the frozen dump format), and the ask rows match
/// the bulk ask builder — the two halves of finding #65's fix.
#[test]
fn the_log_streams_the_bulk_builders_bytes() {
    let fresh = fixture();
    let mut log = retain_log();
    let out = run_naming(
        &NamingInput {
            fresh: &fresh,
            prior: None,
            library: None,
        },
        &config_tier(tier_of("relaxed")),
        &LifoProvider::default(),
        &mut log,
    )
    .expect("the stage runs");
    assert!(
        out.waves.dispatches.len() > 10,
        "the fixture dispatches enough to compare"
    );
    // The bulk form over the retained dispatches, in the dump's own
    // recording order (the waves, then the pre-generate sweep).
    let mut dispatches: Vec<Dispatch<'_>> =
        out.waves.dispatches.iter().map(Dispatch::Naming).collect();
    let sweep: Vec<Dispatch<'_>> = out
        .pre_sweep
        .as_ref()
        .map(|s| {
            s.dispatches
                .iter()
                .map(|d| Dispatch::Sweep(crate::trail::Anchor::Fresh, d))
                .collect()
        })
        .unwrap_or_default();
    dispatches.extend(sweep);
    let params = config_tier(crate::fast::FastTier::Off).params;
    let (prompts, keys) = dispatch_rows(&dispatches, &params);
    let mut streamed = (String::new(), String::new());
    for (p, k) in log.memory_rows() {
        streamed.0.push_str(p);
        streamed.0.push('\n');
        streamed.1.push_str(k);
        streamed.1.push('\n');
    }
    assert_eq!(streamed.0, prompts, "prompts.jsonl, row for row");
    assert_eq!(streamed.1, keys, "cache-keys.jsonl, row for row");
    let asks = ask_rows(&dispatches);
    assert_eq!(log.ask_rows(), asks, "asks.jsonl, row for row");
}

/// A run with neither dump flag retains NOTHING per ask (finding #65: the
/// accumulated records were the ~99GB holder) — no records, no ask rows,
/// no prompt rows; `--dump-asks` alone keeps the ask rows but never a
/// prompt.
#[test]
fn the_off_log_retains_nothing_and_the_asks_log_keeps_rows_only() {
    let fresh = fixture();
    let params = config_tier(crate::fast::FastTier::Off).params;
    let mut off = crate::artifact_dump::DispatchLog::off(params.clone());
    let out = run_naming(
        &NamingInput {
            fresh: &fresh,
            prior: None,
            library: None,
        },
        &config_tier(tier_of("relaxed")),
        &LifoProvider::default(),
        &mut off,
    )
    .expect("the stage runs");
    assert!(
        out.waves.dispatches.is_empty(),
        "no flag: no dispatch record survives its round"
    );
    assert!(off.ask_rows().is_empty(), "no ask rows either");
    assert!(off.memory_rows().is_empty(), "no prompt rows either");

    let mut asks = crate::artifact_dump::DispatchLog::asks(params);
    let out = run_naming(
        &NamingInput {
            fresh: &fresh,
            prior: None,
            library: None,
        },
        &config_tier(tier_of("relaxed")),
        &LifoProvider::default(),
        &mut asks,
    )
    .expect("the stage runs");
    assert!(
        out.waves.dispatches.is_empty(),
        "the asks mode never retains a record"
    );
    assert!(
        !asks.ask_rows().is_empty(),
        "the ask rows are there for the log"
    );
    assert!(
        asks.memory_rows().is_empty(),
        "the asks mode keeps no prompt material"
    );
    assert!(asks.ask_rows().len() > 10, "the fixture dispatched");
}

/// A dump-streaming run (`--dump-artifacts`, real part files) ships the
/// SAME prompts.jsonl + cache-keys.jsonl bytes as the bulk builders over
/// a retaining run's dispatches — the pipelined rounds' records were
/// REBUILT at their canonical commit ([`Prepared::Replay`], finding #65),
/// so this is the rebuild's byte-identity proof.
#[test]
fn a_dump_streaming_run_replays_the_prepared_records_bytes() {
    let fresh = fixture();
    let params = config_tier(crate::fast::FastTier::Off).params;
    // The bulk oracle: the same fixture through the retaining log.
    let retained = run_cfg(
        &fresh,
        &config_tier(tier_of("relaxed")),
        &LifoProvider::default(),
    );
    let mut dispatches: Vec<Dispatch<'_>> = retained
        .waves
        .dispatches
        .iter()
        .map(Dispatch::Naming)
        .collect();
    if let Some(sweep) = &retained.pre_sweep {
        dispatches.extend(
            sweep
                .dispatches
                .iter()
                .map(|d| Dispatch::Sweep(crate::trail::Anchor::Fresh, d)),
        );
    }
    let (prompts, keys) = dispatch_rows(&dispatches, &params);

    // The streaming run: its pipelined records exist only as replay
    // sources, and the rows are written to real files at commit.
    let dir = std::env::temp_dir().join(format!("humanify-dump65-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let mut log = crate::artifact_dump::DispatchLog::dump(params, &dir).expect("open parts");
    let streamed = run_naming(
        &NamingInput {
            fresh: &fresh,
            prior: None,
            library: None,
        },
        &config_tier(tier_of("relaxed")),
        &LifoProvider::default(),
        &mut log,
    )
    .expect("the stage runs");
    assert!(
        streamed.waves.dispatches.is_empty(),
        "the dump mode retains no record"
    );
    log.close().expect("assemble");
    assert_eq!(
        std::fs::read_to_string(dir.join("prompts.jsonl")).expect("prompts.jsonl"),
        prompts,
        "the replayed rows are the prepared rows"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("cache-keys.jsonl")).expect("cache-keys.jsonl"),
        keys
    );
    for part in [
        "prompts.jsonl.vendorpart",
        "prompts.jsonl.namingpart",
        "cache-keys.jsonl.vendorpart",
        "cache-keys.jsonl.namingpart",
    ] {
        assert!(
            !dir.join(part).exists(),
            "close removes the part files ({part})"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
