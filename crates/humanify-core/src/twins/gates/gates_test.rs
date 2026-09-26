//! Tests for the gate ladder ([`super`]) — ports the meaningful cases of
//! TS `src/prior-version/statement-twin.test.ts` (21 tests; the AST-apply
//! and entry-point-shape tests belong to WP3.2's transfer port and are not
//! reproducible at this layer).
//!
//! Two harnesses:
//! - [`with_twin_sides`] — the REAL match stage runs (`match_prior_version`)
//!   and the [`TwinInputs`] come from their one owner,
//!   `rename::transfer::statement_twins` (the settled lifecycle states +
//!   the full binding-rename list the transfer stage gates over). The
//!   fixtures are built to defeat the function matcher but not the
//!   statement tier (the TS file header's design).
//! - [`with_direct_sides`] — the TS `computeStatementTwinTransfers` direct
//!   tests: the cascade results are SUPPLIED by hand (empty matches, chosen
//!   claims), every row Pending. This is the only way to exercise the
//!   conflict override and the module tier (the TS tests drive them the
//!   same way), and the private-name gates (whose derived-mode twin pairs
//!   the binding cascade's slot-blind fingerprints claim first — see the
//!   private-drift test's note).

use std::collections::{HashMap, HashSet};

use oxc_allocator::Allocator;

use super::{
    GateSide, RowState, TwinInputs, TwinOutcome, compute_gated_statement_twins, gate_dump,
};
use crate::graph::{GraphFunction, UnifiedGraph, build_unified_graph};
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::matching::alternation::GraphSide;
use crate::modules::wrapper::find_wrapper_function;
use crate::prior::{PriorMatchInput, match_prior_version};
use crate::rename::transfer::statement_twins;
use crate::twins::gates::TwinGateOutput;
use crate::twins::{FRESH_ANCHOR, PRIOR_ANCHOR, statement_inventory_with_values};

// ---------------------------------------------------------------------------
// Fixtures (TS statement-twin.test.ts)
// ---------------------------------------------------------------------------

/// Two lazy-init arrows whose fingerprints collide (the blind structural
/// hash reads 111/222/333 as the same magnitude) with UNEQUAL counts
/// (2 prior vs 3 fresh) — every function tier abstains, the arrows stay
/// pending, and the statements stay unique 1:1 by their literal magnitudes.
const PRIOR_LAZY: &str = "\
var loadAlphaService = (alphaRetries) => { var alphaEndpoint = 111; return alphaEndpoint + alphaRetries; };
var loadBetaService = (betaRetries) => { var betaEndpoint = 222; return betaEndpoint + betaRetries; };
";
const FRESH_LAZY: &str = "\
var x1 = (r1) => { var e1 = 111; return e1 + r1; };
var x2 = (r2) => { var e2 = 222; return e2 + r2; };
var x3 = (r3) => { var e3 = 333; return e3 + r3; };
";

/// The 85→86-style reorder: the same-shaped siblings swap source order so
/// the equal-count ordinal tier exact-matches them CROSSED; the statements
/// stay unique 1:1 by literals and the twin must restore each statement's
/// own prior names.
const FRESH_SWAPPED: &str = "\
var x2 = (r2) => { var e2 = 222; return e2 + r2; };
var x1 = (r1) => { var e1 = 111; return e1 + r1; };
";

const PRIOR_DUPLICATE: &str = "\
var loadOne = (oneRetries) => { var oneEndpoint = 555; return oneEndpoint + oneRetries; };
var loadTwo = (twoRetries) => { var twoEndpoint = 555; return twoEndpoint + twoRetries; };
";
const FRESH_DUPLICATE: &str = "\
var x1 = (r1) => { var e1 = 555; return e1 + r1; };
";

/// The callee-veto fixture: execAlpha/execBeta exact-match across versions;
/// the wrapped statements hash equal (callee names masked, "task" both) and
/// are unique per side — but the prior twin calls execAlpha while the fresh
/// one calls execBeta. The extra fresh-only sibling ("other") keeps the
/// arrow bucket unequal-count so the arrows stay pending.
/// The callee-veto fixture. The fresh twin renames a PROPERTY in its init
/// (onRun → onCall): the statement hash MASKS property names (so the
/// unique tier still proposes rt↔runTask) but the binding fingerprint
/// keeps them (the structural fixture's b1 is pending for the same
/// reason), so the cascade leaves module:rt PENDING and unclaimed — arm 2
/// of needsBridging fires, where the earlier draft reached candidacy only
/// through the harness's session-id claimed-set bug. The callees then
/// contradict: the fresh statement calls {w, g2} where the prior's
/// {wrap, execAlpha} map through the fn matches to {w, g1} — a mismatch,
/// not an ambiguity: vetoed:callee.
const PRIOR_CALLEE: &str = "\
function execAlpha(taskName) { return taskName + \"a\"; }
function execBeta(taskName) { return taskName + \"b\" + \"b\"; }
function wrap(o) { return o; }
var runTask = wrap({ onRun: (taskInput) => { var taskResult = execAlpha(\"task\"); return taskResult + taskInput; } });
";
const FRESH_CALLEE: &str = "\
function g1(t) { return t + \"a\"; }
function g2(t) { return t + \"b\" + \"b\"; }
function w(o) { return o; }
var rt = w({ onCall: (ti) => { var tr = g2(\"task\"); return tr + ti; } });
";

/// The structural-gate fixture: statementHash masks property names, so
/// {onFoo:…} and {onBar:…} twin at the coarse level — the canonical hash
/// keeps property names verbatim and must refuse the bridge. The "alpha" /
/// "other" strings differ so the fresh statements stay distinct.
const PRIOR_STRUCTURAL: &str = "\
function util(x) { return x + 1; }
var registerAlpha = reg({ onFoo: (alphaCb) => { var alphaVal = util(alphaCb); return alphaVal; } }, \"alpha\");
";
const FRESH_STRUCTURAL: &str = "\
function u2(x) { return x + 9; }
var b1 = reg({ onBar: (cb1) => { var v1 = u2(cb1); return v1; } }, \"alpha\");
var b2 = reg({ onBar: (cb2) => { var v2 = u2(cb2); return v2; } }, \"other\");
";

/// The bucket-identity fixture: same-shaped lazy statements whose only
/// distinguishing feature is WHICH matched helper they reference. The fresh
/// side swaps the two lazy statements (a source-order pairing would cross
/// them; the reference keys must not).
const PRIOR_BUCKET: &str = "\
function libAlpha() { return 1 + 1; }
function libBeta() { return 2 * 3; }
var alphaCache;
var betaCache;
var initAlphaModule = (alphaReady) => { alphaCache = libAlpha(); return alphaReady; };
var initBetaModule = (betaReady) => { betaCache = libBeta(); return betaReady; };
var readAlphaTwice = () => alphaCache + alphaCache + 7777;
var readBetaTwice = () => betaCache + betaCache + 8888;
";
const FRESH_BUCKET: &str = "\
function fA() { return 1 + 1; }
function fB() { return 2 * 3; }
var c1;
var c2;
var k2 = (p2) => { c2 = fB(); return p2; };
var k1 = (p1) => { c1 = fA(); return p1; };
var r1 = () => c1 + c1 + 7777;
var r2 = () => c2 + c2 + 8888;
";
/// A third same-shape sibling on the fresh side only: ANY key claim could
/// hand a prior member's names to new code — abstain (orphan-claim guard).
const FRESH_BUCKET_UNEQUAL: &str = "\
function fA() { return 1 + 1; }
function fB() { return 2 * 3; }
function fG() { return 9 - 4; }
var c1;
var c2;
var c3;
var k1 = (p1) => { c1 = fA(); return p1; };
var k2 = (p2) => { c2 = fB(); return p2; };
var k3 = (p3) => { c3 = fG(); return p3; };
var r1 = () => c1 + c1 + 7777;
var r2 = () => c2 + c2 + 8888;
";

/// The private-name fixtures (masked structural gate + private bridge).
const PRIOR_PRIVATE_DRIFT: &str = "\
function helperOne(x) { return x + 1; }
class BaseCommandModel { #registryCache; run(commandInput) { this.#registryCache = helperOne(commandInput) + 4321; return this.#registryCache; } }
var wireCommand = (cmdArg) => { var cmdSlot = new BaseCommandModel(); return cmdSlot.run(cmdArg) + 9999; };
";
const FRESH_PRIVATE_DRIFT: &str = "\
function h1(x) { return x + 1; }
class C1 { #a; run(ci) { this.#a = h1(ci) + 4321; return this.#a; } }
var w1 = (ca) => { var cs = new C1(); return cs.run(ca) + 9999; };
";
const PRIOR_PRIVATE_SWAP: &str = "\
class SwapModel { #alpha; #beta; go(swapInput) { this.#alpha = swapInput + 111; this.#beta = swapInput + 222; return this.#alpha + this.#beta; } }
";
const FRESH_PRIVATE_SWAP: &str = "\
class S1 { #beta; #alpha; go(si) { this.#beta = si + 111; this.#alpha = si + 222; return this.#beta + this.#alpha; } }
";

/// exp073 module fixtures. Module A holds one `var <ident>;` and its init;
/// module B holds two and a two-statement init — `var <ident>;` occurs
/// THREE times tree-wide (the unique tier abstains on every one) while the
/// two module SIGNATURES differ, making both unique on both sides.
const ESM_HELPER: &str = "var __esm = (fn, res) => () => (fn, res);";
const PRIOR_MODULES: &str = "\
var __esm = (fn, res) => () => (fn, res);
var alphaValue;
var alphaInit = __esm(() => { alphaValue = readConfig(1); });
var betaValue;
var betaSpare;
var betaInit = __esm(() => { betaValue = readConfig(2); betaSpare = 7; });
";
const FRESH_MODULES: &str = "\
var __esm = (fn, res) => () => (fn, res);
var q0;
var q1 = __esm(() => { q0 = readConfig(1); });
var q2;
var q3;
var q4 = __esm(() => { q2 = readConfig(2); q3 = 7; });
";

/// The ambiguous (twin) modules: a leading seed module absorbs the helper
/// declaration, so the two modules that follow have IDENTICAL segments —
/// genuine twins, unique on neither side. Nothing may be paired from them.
fn twin_src(a: &str, b: &str) -> String {
    format!(
        "\
{helper}
var seedValue;
var seedInit = __esm(() => {{ seedValue = boot(9); }});
var {a};
var {a}Init = __esm(() => {{ {a} = readConfig(1); }});
var {b};
var {b}Init = __esm(() => {{ {b} = readConfig(1); }});
",
        helper = ESM_HELPER
    )
}

/// A wrapper bundle: 55 filler `var` statements trip the wrapper gate, the
/// given statements live INSIDE the wrapper body (so their bindings are
/// function-scoped, not module bindings).
fn wrapper_bundle(statements: &[&str], filler_prefix: &str) -> String {
    let filler: Vec<String> = (0..55)
        .map(|i| format!("var {filler_prefix}{i} = 7;"))
        .collect();
    let mut code = String::from("(function(){\n");
    code.push_str(&filler.join("\n"));
    code.push('\n');
    for statement in statements {
        code.push_str(statement);
        code.push('\n');
    }
    code.push_str("})();\n");
    code
}

// ---------------------------------------------------------------------------
// Harnesses
// ---------------------------------------------------------------------------

/// What the assertions get from [`with_twin_sides`]: the fresh side's
/// handles and the gated output.
struct TwinSides<'a> {
    fresh_graph: &'a UnifiedGraph,
    fresh_text: &'a str,
    output: &'a TwinGateOutput,
    claimed: &'a HashSet<String>,
    fn_matches: &'a HashMap<String, String>,
}

/// The REAL match stage over both texts (`match_prior_version` — the flow
/// the dumps run), then the twins through their one owner
/// (`rename::transfer::statement_twins`: the settled states + the full
/// binding-rename list, exactly what the transfer stage gates over). The
/// closure runs inside the scope that owns both arenas. `claimed` is the
/// binding cascade's matched fresh names.
fn with_twin_sides(prior_code: &str, fresh_code: &str, run: impl FnOnce(TwinSides<'_>)) {
    with_twin_sides_classified(prior_code, fresh_code, None, run);
}

/// [`with_twin_sides`] under a library classification: the freeze goes
/// through its one owner (`rename::transfer::library_freeze`), as the
/// driver and the dump verbs apply it.
fn with_twin_sides_classified(
    prior_code: &str,
    fresh_code: &str,
    library: Option<&crate::libdetect::function_carry::LibraryClassification>,
    run: impl FnOnce(TwinSides<'_>),
) {
    let input = PriorMatchInput {
        fresh: fresh_code,
        prior: prior_code,
        bundler: None,
        minifier: None,
        fast: false,
    };
    match_prior_version(input, |stage| {
        let freeze = crate::rename::transfer::library_freeze(stage, library, true)?;
        let output = statement_twins(stage, &freeze)?;
        let fn_matches: HashMap<String, String> = stage.function_result.matches.to_hash_map();
        let matched_ids: HashSet<&String> = stage
            .binding_result
            .map(|r| r.matches.values().collect())
            .unwrap_or_default();
        let claimed: HashSet<String> = stage
            .fresh
            .graph
            .module_bindings
            .iter()
            .filter(|b| matched_ids.contains(&b.session_id))
            .map(|b| b.name.clone())
            .collect();
        run(TwinSides {
            fresh_graph: stage.fresh.graph,
            fresh_text: fresh_code,
            output: &output,
            claimed: &claimed,
            fn_matches: &fn_matches,
        });
        Ok(())
    })
    .expect("the match stage and the twins must run");
}

/// The direct variant (TS `computeStatementTwinTransfers` tests): the
/// cascade's results are SUPPLIED — every fresh row Pending, the given
/// fn matches / claims / identity pairs. No cascade runs.
fn with_direct_sides(
    prior_code: &str,
    fresh_code: &str,
    fn_matches: HashMap<String, String>,
    claimed: &[&str],
    identity_pairs: &[(&str, &str)],
    run: impl FnOnce(TwinGateOutput),
) {
    with_gate_sides(
        prior_code,
        fresh_code,
        fn_matches,
        claimed,
        identity_pairs,
        |output, _, _| run(output),
    );
}

/// The same harness, handing the gate sides too — the dump tests read the
/// stats bag through `gate_dump`.
fn with_gate_sides(
    prior_code: &str,
    fresh_code: &str,
    fn_matches: HashMap<String, String>,
    claimed: &[&str],
    identity_pairs: &[(&str, &str)],
    run: impl FnOnce(TwinGateOutput, &GateSide<'_, '_>, &GateSide<'_, '_>),
) {
    let prior_allocator = Allocator::default();
    let fresh_allocator = Allocator::default();
    let prior_ingest = Ingest::parse(&prior_allocator, prior_code, "prior.js");
    let fresh_ingest = Ingest::parse(&fresh_allocator, fresh_code, "fresh.js");
    assert!(prior_ingest.errors.is_empty(), "prior must parse");
    assert!(fresh_ingest.errors.is_empty(), "fresh must parse");
    let prior_tables = SymbolTables::build(prior_ingest.semantic());
    let fresh_tables = SymbolTables::build(fresh_ingest.semantic());
    let prior_graph = build_unified_graph(
        prior_ingest.semantic(),
        prior_ingest.program,
        "prior.js",
        &[],
        None,
        None,
    );
    let fresh_graph = build_unified_graph(
        fresh_ingest.semantic(),
        fresh_ingest.program,
        "fresh.js",
        &[],
        None,
        None,
    );
    let prior_side = GraphSide::build(&prior_graph, prior_ingest.semantic());
    let fresh_side = GraphSide::build(&fresh_graph, fresh_ingest.semantic());
    let (prior_inventory, prior_values) =
        statement_inventory_with_values(prior_code, PRIOR_ANCHOR, Some(&prior_graph))
            .expect("prior inventory");
    let (fresh_inventory, fresh_values) =
        statement_inventory_with_values(fresh_code, FRESH_ANCHOR, Some(&fresh_graph))
            .expect("fresh inventory");

    let claimed: HashSet<String> = claimed.iter().map(|s| s.to_string()).collect();
    let identity_pairs: Vec<(String, String)> = identity_pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    let fn_states: HashMap<String, RowState> = fresh_graph
        .functions
        .iter()
        .map(|f| (f.session_id.clone(), RowState::Pending))
        .collect();
    let binding_states: HashMap<String, RowState> = fresh_graph
        .module_bindings
        .iter()
        .map(|b| (b.session_id.clone(), RowState::Pending))
        .collect();
    let input = TwinInputs {
        fn_matches: &fn_matches,
        claimed_old_names: &claimed,
        binding_identity_pairs: &identity_pairs,
        fn_states: &fn_states,
        binding_states: &binding_states,
    };

    let prior_wrapper = find_wrapper_function(prior_ingest.program, prior_ingest.semantic());
    let fresh_wrapper = find_wrapper_function(fresh_ingest.program, fresh_ingest.semantic());
    let prior_gate = GateSide::build(
        &prior_graph,
        prior_ingest.semantic(),
        &prior_tables,
        &prior_inventory,
        &prior_values,
        &prior_side,
        prior_wrapper.as_ref().map(|w| w.span),
    );
    let fresh_gate = GateSide::build(
        &fresh_graph,
        fresh_ingest.semantic(),
        &fresh_tables,
        &fresh_inventory,
        &fresh_values,
        &fresh_side,
        fresh_wrapper.as_ref().map(|w| w.span),
    );
    let output = compute_gated_statement_twins(&prior_gate, &fresh_gate, &input)
        .expect("the gate run must not hit a fossil anomaly");
    run(output, &prior_gate, &fresh_gate);
}

// ---------------------------------------------------------------------------
// Assertion helpers
// ---------------------------------------------------------------------------

/// oldName → newName over the gated transfer pairs.
fn pair_map(output: &TwinGateOutput) -> HashMap<String, String> {
    output
        .pairs
        .iter()
        .map(|p| (p.old_name.clone(), p.new_name.clone()))
        .collect()
}

fn has_outer(output: &TwinGateOutput, old: &str, new: &str) -> bool {
    output
        .outer_refs
        .iter()
        .any(|p| p.old_name == old && p.new_name == new)
}

/// The function row whose source text contains `needle` (session ids are
/// positional; the text slice is the stable handle).
fn fn_row_with<'g>(graph: &'g UnifiedGraph, text: &str, needle: &str) -> &'g GraphFunction {
    // The SMALLEST row containing the needle: in a wrapper bundle the
    // wrapper row contains every inner function's text.
    graph
        .functions
        .iter()
        .filter(|f| text[f.span.start as usize..f.span.end as usize].contains(needle))
        .min_by_key(|f| f.span.end - f.span.start)
        .unwrap_or_else(|| panic!("no function row containing {needle:?}"))
}

// ---------------------------------------------------------------------------
// The unique tier through the real cascade
// ---------------------------------------------------------------------------

/// The name pairs the lazy fixture must bridge (shared by the plain and
/// the swapped-order test).
fn assert_lazy_name_pairs(m: &HashMap<String, String>) {
    assert_eq!(m.get("r1").map(String::as_str), Some("alphaRetries"));
    assert_eq!(m.get("e1").map(String::as_str), Some("alphaEndpoint"));
    assert_eq!(m.get("r2").map(String::as_str), Some("betaRetries"));
    assert_eq!(m.get("e2").map(String::as_str), Some("betaEndpoint"));
    // heads are fn-expression inits the binding cascade excludes — the
    // twin tier is what recovers them
    assert_eq!(m.get("x1").map(String::as_str), Some("loadAlphaService"));
    assert_eq!(m.get("x2").map(String::as_str), Some("loadBetaService"));
    // the gamma statement has no prior twin — genuinely new, untouched
    assert!(!m.contains_key("r3"));
    assert!(!m.contains_key("e3"));
    assert!(!m.contains_key("x3"));
}

/// The stats + the apply half's per-pair owner fields for the lazy fixture.
fn assert_lazy_stats_and_owners(sides: &TwinSides<'_>) {
    let stats = &sides.output.stats;
    assert_eq!(stats.unique_twins, 2, "x3 has no prior twin");
    assert_eq!(stats.candidates, 2);
    assert_eq!(stats.transferred_twins, 2);
    assert_eq!(stats.pairs, 6);
    assert_eq!(
        stats.vetoed_callee + stats.vetoed_role + stats.vetoed_structural,
        0
    );
    // module-level head vs the arrow-owned locals
    let x1 = sides
        .output
        .pairs
        .iter()
        .find(|p| p.old_name == "x1")
        .unwrap();
    assert!(x1.owner_fn_session.is_none(), "x1 is module level");
    assert!(!x1.is_function_declaration);
    let arrow = fn_row_with(sides.fresh_graph, sides.fresh_text, "r1");
    let r1 = sides
        .output
        .pairs
        .iter()
        .find(|p| p.old_name == "r1")
        .unwrap();
    assert_eq!(
        r1.owner_fn_session.as_deref(),
        Some(arrow.session_id.as_str()),
        "r1's owner is the arrow row"
    );
}

#[test]
fn bridges_pending_arrows_through_unique_twins() {
    with_twin_sides(PRIOR_LAZY, FRESH_LAZY, |sides| {
        assert_lazy_name_pairs(&pair_map(sides.output));
        assert_lazy_stats_and_owners(&sides);
    });
}

#[test]
fn abstains_when_the_hash_is_not_unique_on_both_sides() {
    with_twin_sides(PRIOR_DUPLICATE, FRESH_DUPLICATE, |sides| {
        assert!(sides.output.pairs.is_empty(), "2:1 counts — no twin");
        assert_eq!(sides.output.stats.unique_twins, 0);
    });
}

#[test]
fn vetoes_a_same_shaped_twin_whose_callee_identity_differs() {
    with_twin_sides(PRIOR_CALLEE, FRESH_CALLEE, |sides| {
        // The fixture's whole point: module:rt is PENDING and unclaimed —
        // arm 2 of needsBridging is the only route to candidacy here.
        assert!(
            !sides.claimed.contains("rt"),
            "the cascade must leave rt unclaimed, got {:?}",
            sides.claimed
        );
        // The mapped-vs-fresh premise: both helpers fn-matched, so the
        // veto is a contradiction between matched identities, not an
        // unmatched-callee ambiguity.
        assert_eq!(
            sides.fn_matches.get("prior.js:1:0").map(String::as_str),
            Some("input.js:1:0"),
            "execAlpha→g1"
        );
        assert_eq!(
            sides.fn_matches.get("prior.js:2:0").map(String::as_str),
            Some("input.js:2:0"),
            "execBeta→g2"
        );
        let m = pair_map(sides.output);
        assert!(
            !m.contains_key("ti") && !m.contains_key("tr"),
            "the different-callee twin must be vetoed, got {m:?}"
        );
        assert_eq!(sides.output.stats.vetoed_callee, 1);
        assert!(
            sides
                .output
                .gated
                .iter()
                .any(|g| g.outcome == TwinOutcome::VetoedCallee)
        );
    });
}

#[test]
fn is_a_no_op_on_a_quiet_hop() {
    // Names already match the prior — every slot's prior name equals its
    // fresh name, so nothing transfers even though the tiers propose.
    with_twin_sides(PRIOR_LAZY, PRIOR_LAZY, |sides| {
        assert!(sides.output.pairs.is_empty());
        assert_eq!(sides.output.stats.pairs, 0);
        assert_eq!(
            sides.output.stats.vetoed_callee + sides.output.stats.vetoed_role,
            0,
            "a quiet hop is not a veto — the slots just agree"
        );
    });
}

#[test]
fn vetoes_when_property_name_content_differs() {
    // statementHash masks property names, so {onFoo:…} and {onBar:…} twin
    // at the coarse level. The ROLE gate lets it through — the shingles'
    // binding-identifier blinding makes the two contents identical, so the
    // overlap is 1.0 — and the STRUCTURAL gate fires: its canonical hash
    // keeps property names verbatim (TS :185, the same shape).
    with_twin_sides(PRIOR_STRUCTURAL, FRESH_STRUCTURAL, |sides| {
        let m = pair_map(sides.output);
        assert!(!m.contains_key("cb1"));
        assert!(!m.contains_key("v1"));
        assert_eq!(sides.output.stats.vetoed_structural, 1);
        assert!(
            sides
                .output
                .gated
                .iter()
                .any(|g| g.outcome == TwinOutcome::VetoedStructural)
        );
    });
}

// ---------------------------------------------------------------------------
// Cross-pair repair under statement reorder
// ---------------------------------------------------------------------------

#[test]
fn bridges_twins_whose_functions_were_exact_matched_across_statements() {
    with_twin_sides(PRIOR_LAZY, FRESH_SWAPPED, |sides| {
        let m = pair_map(sides.output);
        // the 111 statement must carry the alpha names, the 222 statement
        // beta — a crossed exact transfer would swap them
        assert_eq!(m.get("x1").map(String::as_str), Some("loadAlphaService"));
        assert_eq!(m.get("r1").map(String::as_str), Some("alphaRetries"));
        assert_eq!(m.get("e1").map(String::as_str), Some("alphaEndpoint"));
        assert_eq!(m.get("x2").map(String::as_str), Some("loadBetaService"));
        assert_eq!(m.get("r2").map(String::as_str), Some("betaRetries"));
        assert_eq!(m.get("e2").map(String::as_str), Some("betaEndpoint"));
        assert_eq!(sides.output.stats.pairs, 6);
        assert!(sides.output.stats.candidates >= 2);
    });
}

// ---------------------------------------------------------------------------
// Bucket identity pairing (non-unique hashes)
// ---------------------------------------------------------------------------

#[test]
fn pairs_equal_count_bucket_members_by_matched_reference_identity() {
    with_twin_sides(PRIOR_BUCKET, FRESH_BUCKET, |sides| {
        let m = pair_map(sides.output);
        assert_eq!(m.get("p1").map(String::as_str), Some("alphaReady"));
        assert_eq!(m.get("p2").map(String::as_str), Some("betaReady"));
        assert_eq!(m.get("k1").map(String::as_str), Some("initAlphaModule"));
        assert_eq!(m.get("k2").map(String::as_str), Some("initBetaModule"));
        // the read statements are unique 1:1 by their literals — the unique
        // tier owns them and their heads bridge too
        assert_eq!(m.get("r1").map(String::as_str), Some("readAlphaTwice"));
        assert_eq!(m.get("r2").map(String::as_str), Some("readBetaTwice"));
        assert_eq!(sides.output.stats.bucket_twins, 2);
        // FOUR unique 1:1 joins: fA↔libAlpha (1+1), fB↔libBeta (2*3) and
        // the two read statements — only the k1/k2 bucket members collide
        // (their hashes are masked-equal, count 2 per side).
        assert_eq!(sides.output.stats.unique_twins, 4);

        // outer-reference votes name the var-only cache roots AND the
        // matched helpers the paired statements reference — never applied
        // directly, they feed the vote propagation
        assert!(
            has_outer(sides.output, "c1", "alphaCache"),
            "outer refs: {:?}",
            sides.output.outer_refs
        );
        assert!(has_outer(sides.output, "c2", "betaCache"));
        assert!(has_outer(sides.output, "fA", "libAlpha"));
        assert!(has_outer(sides.output, "fB", "libBeta"));
        assert!(
            !sides.output.pairs.iter().any(|p| p.old_name == "c1"),
            "outer refs are votes, never transfer pairs"
        );
    });
}

#[test]
fn abstains_from_a_bucket_whose_counts_changed() {
    with_twin_sides(PRIOR_BUCKET, FRESH_BUCKET_UNEQUAL, |sides| {
        let m = pair_map(sides.output);
        assert!(!m.contains_key("p1") && !m.contains_key("k1"));
        assert!(!m.contains_key("p3") && !m.contains_key("k3"));
        assert_eq!(sides.output.stats.bucket_twins, 0);
        // the unique-tier read statements still bridge
        assert_eq!(m.get("r1").map(String::as_str), Some("readAlphaTwice"));
    });
}

// ---------------------------------------------------------------------------
// Twin-over-cascade conflict override (direct cascade results)
// ---------------------------------------------------------------------------

#[test]
fn emits_the_twin_pair_for_a_cascade_claimed_head_that_conflicts() {
    // The binding cascade is literal-blind: same-shape family members can
    // rotate. The gated twin pairing sees the literals — when both claim
    // the same head with different names, the twin emits its pair anyway
    // (it applies first; the crossed cascade rename then drops stale).
    with_direct_sides(
        PRIOR_LAZY,
        FRESH_LAZY,
        HashMap::new(),
        &["x1", "x2"],
        &[("x1", "loadBetaService"), ("x2", "loadAlphaService")],
        |output| {
            let m = pair_map(&output);
            assert_eq!(m.get("x1").map(String::as_str), Some("loadAlphaService"));
            assert_eq!(m.get("x2").map(String::as_str), Some("loadBetaService"));
            assert_eq!(output.stats.cascade_conflicts, 2);
            assert_eq!(output.conflicts.len(), 2);
            let x1 = output
                .conflicts
                .iter()
                .find(|c| c.old_name == "x1")
                .unwrap();
            assert_eq!(x1.cascade_name, "loadBetaService");
            assert_eq!(x1.twin_name, "loadAlphaService");
        },
    );
}

#[test]
fn still_defers_to_the_cascade_when_it_agrees_with_the_twin() {
    with_direct_sides(
        PRIOR_LAZY,
        FRESH_LAZY,
        HashMap::new(),
        &["x1"],
        &[("x1", "loadAlphaService")],
        |output| {
            let m = pair_map(&output);
            assert!(
                !m.contains_key("x1"),
                "an agreeing claim stays skipped, got {m:?}"
            );
            // the arrow-internal locals still bridge (their owner fn is
            // pending, their names unclaimed)
            assert_eq!(m.get("r1").map(String::as_str), Some("alphaRetries"));
            assert_eq!(m.get("e1").map(String::as_str), Some("alphaEndpoint"));
            assert!(output.conflicts.is_empty());
            assert_eq!(output.stats.cascade_conflicts, 0);
        },
    );
}

/// FIXED alongside the TS (2026-09-20): the dump's stats bag carries the
/// TRUE cascade-conflict count — the TS's flush-before-assignment
/// staleness was reproduced until Andrew's call (fix, don't reproduce).
#[test]
fn the_dumped_stats_carry_the_true_cascade_count() {
    with_gate_sides(
        PRIOR_LAZY,
        FRESH_LAZY,
        HashMap::new(),
        &["x1", "x2"],
        &[("x1", "loadBetaService"), ("x2", "loadAlphaService")],
        |output, prior_gate, fresh_gate| {
            assert_eq!(output.stats.cascade_conflicts, 2);
            let dump = gate_dump(&output, prior_gate, fresh_gate);
            assert_eq!(
                dump["stats"]["cascadeConflicts"],
                serde_json::json!(2),
                "the dump is honest now: the true count"
            );
        },
    );
}

// ---------------------------------------------------------------------------
// Module-scoped pairing (exp073)
// ---------------------------------------------------------------------------

#[test]
fn pairs_ambiguous_statements_inside_uniquely_matched_modules() {
    with_direct_sides(
        PRIOR_MODULES,
        FRESH_MODULES,
        HashMap::new(),
        &[],
        &[],
        |output| {
            let m = pair_map(&output);
            assert!(
                output.stats.module_scoped_twins > 0,
                "expected module-scoped pairs, stats: {:?}",
                output.stats
            );
            assert_eq!(m.get("q0").map(String::as_str), Some("alphaValue"));
            assert_eq!(m.get("q2").map(String::as_str), Some("betaValue"));
        },
    );
}

#[test]
fn never_pairs_across_ambiguous_twin_modules() {
    with_direct_sides(
        &twin_src("keptOne", "keptTwo"),
        &twin_src("z0", "z2"),
        HashMap::new(),
        &[],
        &[],
        |output| {
            // Both twin modules are SEEN and SKIPPED; the seed module ahead
            // of them is unique and may legitimately pair, so the safety
            // property is the AMBIGUOUS count, not zero pairs.
            assert_eq!(
                output.stats.module_scoped_ambiguous, 2,
                "both twin modules must be counted as skipped, stats: {:?}",
                output.stats
            );
            let m = pair_map(&output);
            for old in ["z0", "z2", "z0Init", "z2Init"] {
                assert!(
                    !m.contains_key(old),
                    "{old} sits in an ambiguous module and must not inherit"
                );
            }
        },
    );
}

// ---------------------------------------------------------------------------
// Private-name drift (masked structural gate + private bridge)
// ---------------------------------------------------------------------------

#[test]
fn bridges_a_class_twin_whose_only_structural_drift_is_private_ids() {
    // DIRECT cascade results (the TS test drives the full pipeline; the
    // Rust cascade's fingerprints keep private names verbatim exactly like
    // the TS's — P=#a vs P=#registryCache differ — so neither claims the
    // class head, and the DERIVED harness behaves the same way: see the
    // next test). The direct shape isolates this module's private bridge
    // from whatever the cascade does.
    with_direct_sides(
        PRIOR_PRIVATE_DRIFT,
        FRESH_PRIVATE_DRIFT,
        HashMap::new(),
        &[],
        &[],
        |output| {
            let m = pair_map(&output);
            assert_eq!(m.get("C1").map(String::as_str), Some("BaseCommandModel"));
            assert_eq!(m.get("ci").map(String::as_str), Some("commandInput"));
            // the echo statement's slots bridge too
            assert_eq!(m.get("w1").map(String::as_str), Some("wireCommand"));
            assert_eq!(m.get("cs").map(String::as_str), Some("cmdSlot"));
            assert_eq!(m.get("ca").map(String::as_str), Some("cmdArg"));
            // and the private id transfers, with every fresh node carrying it
            assert_eq!(
                output.private_renames.len(),
                1,
                "{:?}",
                output.private_renames
            );
            let set = &output.private_renames[0];
            assert_eq!(set.old_name, "a");
            assert_eq!(set.new_name, "registryCache");
            assert!(
                set.node_spans.len() >= 3,
                "the declaration key + both this.#a reads/writes: {:?}",
                set.node_spans
            );
        },
    );
}

#[test]
fn the_derived_cascade_leaves_the_class_head_to_the_twin() {
    // The premise this test used to pin ("the fingerprint-slot divergence —
    // the Rust cascade slots privates so it claims the class head") is
    // FALSE: the serializer's per-class private-slot map is clobbered
    // before any private token (the take-per-node restore pattern in
    // serialize.rs), so the binding fingerprints carry privates VERBATIM
    // exactly like the TS's — the cascade CANNOT claim C1 ↔
    // BaseCommandModel (#a vs #registryCache), and the twin owns the head,
    // derived mode included. FULL parity with the TS test's renames.
    with_twin_sides(PRIOR_PRIVATE_DRIFT, FRESH_PRIVATE_DRIFT, |sides| {
        let m = pair_map(sides.output);
        assert_eq!(m.get("C1").map(String::as_str), Some("BaseCommandModel"));
        assert_eq!(m.get("ci").map(String::as_str), Some("commandInput"));
        assert!(
            sides.output.conflicts.is_empty(),
            "the cascade never claimed the head — no conflicts"
        );
        // the echo statement still bridges its own slots
        assert_eq!(m.get("w1").map(String::as_str), Some("wireCommand"));
        assert_eq!(m.get("ca").map(String::as_str), Some("cmdArg"));
        // and the class reference inside it goes to the vote propagation
        assert!(
            has_outer(sides.output, "C1", "BaseCommandModel"),
            "the fresh class reference is an outer-ref vote for the head name"
        );
        // the private spelling transfers in derived mode too
        assert_eq!(sides.output.private_renames.len(), 1);
        assert_eq!(sides.output.private_renames[0].new_name, "registryCache");
    });
}

#[test]
fn abstains_from_private_transfer_when_the_target_id_already_exists() {
    // A private SWAP: #beta→#alpha collides with the existing #alpha —
    // abstain; the ordinary bindings transfer regardless.
    with_direct_sides(
        PRIOR_PRIVATE_SWAP,
        FRESH_PRIVATE_SWAP,
        HashMap::new(),
        &[],
        &[],
        |output| {
            let m = pair_map(&output);
            assert_eq!(m.get("S1").map(String::as_str), Some("SwapModel"));
            assert_eq!(m.get("si").map(String::as_str), Some("swapInput"));
            assert!(
                output.private_renames.is_empty(),
                "a swap collides with the surviving target id: {:?}",
                output.private_renames
            );
        },
    );
}

// ---------------------------------------------------------------------------
// The wrapper owner path
// ---------------------------------------------------------------------------

#[test]
fn wrapper_body_heads_bridge_and_the_arrow_locals_carry_the_owner_session() {
    // The same lazy fixture INSIDE the wrapper: the statement twins still
    // fire (the inventory reads the wrapper body), and the wrapper BODY is
    // module level for the owner gate (TS :390 — `fnPath.node ===
    // wrapperNode` counts as module level), so the declarator heads bridge
    // through the module-level half exactly like program-level heads. The
    // fn-internal slots' owner is the arrow row, which is pending → they
    // bridge too, carrying the arrow's session id for the apply half's
    // owner registration (a wrapper-body head's owner session is None —
    // owner_session_of skips the wrapper itself).
    let prior = wrapper_bundle(
        &[
            "var loadAlphaService = (alphaRetries) => { var alphaEndpoint = 111; return alphaEndpoint + alphaRetries; };",
            "var loadBetaService = (betaRetries) => { var betaEndpoint = 222; return betaEndpoint + betaRetries; };",
        ],
        "z",
    );
    let fresh = wrapper_bundle(
        &[
            "var x1 = (r1) => { var e1 = 111; return e1 + r1; };",
            "var x2 = (r2) => { var e2 = 222; return e2 + r2; };",
            "var x3 = (r3) => { var e3 = 333; return e3 + r3; };",
        ],
        "q",
    );
    with_twin_sides(&prior, &fresh, |sides| {
        let m = pair_map(sides.output);
        assert_eq!(m.get("x1").map(String::as_str), Some("loadAlphaService"));
        assert_eq!(m.get("x2").map(String::as_str), Some("loadBetaService"));
        assert_eq!(m.get("r1").map(String::as_str), Some("alphaRetries"));
        assert_eq!(m.get("e1").map(String::as_str), Some("alphaEndpoint"));
        assert_eq!(m.get("r2").map(String::as_str), Some("betaRetries"));
        assert_eq!(m.get("e2").map(String::as_str), Some("betaEndpoint"));
        assert_eq!(sides.output.stats.pairs, 6);
        let arrow = fn_row_with(sides.fresh_graph, sides.fresh_text, "e1 + r1");
        let r1 = sides
            .output
            .pairs
            .iter()
            .find(|p| p.old_name == "r1")
            .unwrap();
        assert_eq!(
            r1.owner_fn_session.as_deref(),
            Some(arrow.session_id.as_str())
        );
        let x1 = sides
            .output
            .pairs
            .iter()
            .find(|p| p.old_name == "x1")
            .unwrap();
        assert_eq!(
            x1.owner_fn_session.as_deref(),
            None,
            "a wrapper-body head is module level — no owner fn session"
        );
    });
}

// ---------------------------------------------------------------------------
// The twins' inputs as the TS reads them AT TWINS TIME (the three regimes
// built for the posture close-out, 2026-09-25 — each one's expected rows
// are the frozen-f7a707d TS dump of the same pair, /work/twins-posture)
// ---------------------------------------------------------------------------

/// Two CLOSE-matched (corroborated by alignment) var-declarator inits whose
/// holders feed two same-shaped statements. The ONLY identity the bucket
/// statements' ref keys can read is the corroborated close match's var
/// pair (`a`→computeTotal, `e`→computeCount): the TS appends it to
/// `bindingCascade.renames` (the SAME array `moduleBindingRenames` aliases)
/// before the twins run.
const PRIOR_CLOSE_VAR: &str = "\
var computeTotal = function (items) {
  let sum = 0;
  for (const item of items) {
    sum += item.price;
  }
  console.log(\"total computed\");
  return sum;
};
var computeCount = function (list) {
  let count = 0;
  for (const entry of list) {
    if (entry.active) count++;
  }
  console.log(\"count computed\");
  return count;
};
var totalHandler = register(\"aa\", computeTotal, 1);
var countHandler = register(\"aa\", computeCount, 1);
console.log(totalHandler, countHandler);
";
const FRESH_CLOSE_VAR: &str = "\
var a = function (b) {
  let c = 0;
  for (const d of b) {
    c += d.price * 2;
  }
  console.log(\"total computed\");
  return c;
};
var e = function (f) {
  let g = 0;
  for (const h of f) {
    if (h.active && h.visible) g++;
  }
  console.log(\"count computed\");
  return g;
};
var i = register(\"aa\", a, 1);
var j = register(\"aa\", e, 1);
console.log(i, j);
";

#[test]
fn a_corroborated_close_matchs_var_pair_keys_the_bucket_tier() {
    with_twin_sides(PRIOR_CLOSE_VAR, FRESH_CLOSE_VAR, |sides| {
        // TS: bucketTwins 2 (both register statements keyed by
        // bind:computeTotal / bind:computeCount), uniqueTwins 1.
        assert_eq!(sides.output.stats.bucket_twins, 2);
        assert_eq!(sides.output.stats.unique_twins, 1);
        assert!(
            sides
                .output
                .gated
                .iter()
                .all(|g| g.outcome == TwinOutcome::NoCandidacy),
            "the handlers are cascade-claimed — no candidacy"
        );
    });
}

/// Two eval-tainted functions the cascade CROSS-pairs relative to the bucket
/// tier's statement pairing (the reorder: `wrap(t, …)` first in fresh,
/// `wrap(one, …)` first in prior). Every fn on an eval site's scope chain is
/// frozen (markEvalWithTaintPreDone) BEFORE applyExactMatches, which then
/// leaves it frozen — so arm 3 (cross-paired exact match) cannot fire.
const PRIOR_EVAL_CROSS: &str = "\
var one = 1;
var two = 100;
wrap(one, function (source) {
  return eval(source);
});
wrap(two, function (source) {
  return eval(source);
});
console.log(one, two);
";
const FRESH_EVAL_CROSS: &str = "\
var o = 1;
var t = 100;
wrap(t, function (s) {
  return eval(s);
});
wrap(o, function (s) {
  return eval(s);
});
console.log(o, t);
";

#[test]
fn a_frozen_cross_paired_function_gives_no_candidacy() {
    with_twin_sides(PRIOR_EVAL_CROSS, FRESH_EVAL_CROSS, |sides| {
        assert_eq!(sides.output.stats.bucket_twins, 2);
        assert!(
            sides
                .output
                .gated
                .iter()
                .all(|g| g.outcome == TwinOutcome::NoCandidacy),
            "eval-tainted fns are frozen, not exact-matched: {:?}",
            sides
                .output
                .gated
                .iter()
                .map(|g| g.outcome)
                .collect::<Vec<_>>()
        );
        assert!(sides.output.pairs.is_empty());
    });
}

/// An eval site anywhere freezes every module binding (the module scope
/// ends every scope chain) and the tainted fn itself: the declarator
/// statement's twin has nothing pending to bridge.
const PRIOR_EVAL_BINDING: &str = "\
var runSource = function (source) {
  return eval(source);
};
var limit = 100;
console.log(runSource, limit);
";
const FRESH_EVAL_BINDING: &str = "\
var k = function (s) {
  return eval(s);
};
var l = 100;
console.log(k, l);
";

#[test]
fn an_eval_tainted_module_freezes_the_twins_bindings() {
    with_twin_sides(PRIOR_EVAL_BINDING, FRESH_EVAL_BINDING, |sides| {
        assert_eq!(sides.output.stats.unique_twins, 3);
        assert!(
            sides
                .output
                .gated
                .iter()
                .all(|g| g.outcome == TwinOutcome::NoCandidacy)
        );
        assert!(
            sides.output.pairs.is_empty(),
            "no rename inside an eval-tainted scope, got {:?}",
            pair_map(sides.output)
        );
    });
}

/// A LIBRARY-frozen function (findings #32/#33, the library-min regime):
/// the declarator statement still bridges its module binding, but the
/// frozen arrow's locals abstain — a settled owner transfers nothing. The
/// TS (fixed, 730eb99) dump of /work/lf/cases/library-min: `bridged
/// slots=1 pairs=1` per library declarator; before the freeze was wired
/// the Rust bridged all three slots.
const PRIOR_LIBRARY: &str = "\
var loadAlphaService = alphaRetries => {
  var alphaEndpoint = 111;
  return alphaEndpoint + alphaRetries;
};
var loadBetaService = betaRetries => {
  var betaEndpoint = 222;
  return betaEndpoint + betaRetries;
};
console.log(loadAlphaService, loadBetaService);
";
const FRESH_LIBRARY: &str = "\
var x1 = r1 => {
  var e1 = 111;
  return e1 + r1;
};
var x2 = r2 => {
  var e2 = 222;
  return e2 + r2;
};
console.log(x1, x2);
";

#[test]
fn a_library_frozen_function_bridges_its_binding_but_not_its_locals() {
    use crate::libdetect::function_carry::{LibraryClassification, LibraryFunctionKey};
    let arrow = |param: &str, line: u32| {
        let start = FRESH_LIBRARY.find(&format!("{param} =>")).unwrap();
        let end = start + FRESH_LIBRARY[start..].find("};").unwrap() + 1;
        LibraryFunctionKey {
            span: oxc_span::Span::new(start as u32, end as u32),
            session_id: format!("input.js:{line}:9"),
            library: "tinylib".into(),
        }
    };
    let library = LibraryClassification::Consumed(vec![arrow("r1", 1), arrow("r2", 5)]);
    let mut unfrozen = HashMap::new();
    with_twin_sides(PRIOR_LIBRARY, FRESH_LIBRARY, |sides| {
        unfrozen = pair_map(sides.output);
    });
    assert!(
        unfrozen.contains_key("r1") && unfrozen.contains_key("e1"),
        "without the freeze the locals bridge: {unfrozen:?}"
    );
    with_twin_sides_classified(PRIOR_LIBRARY, FRESH_LIBRARY, Some(&library), |sides| {
        let pairs = pair_map(sides.output);
        assert_eq!(
            pairs.get("x1").map(String::as_str),
            Some("loadAlphaService")
        );
        assert_eq!(pairs.get("x2").map(String::as_str), Some("loadBetaService"));
        for local in ["r1", "e1", "r2", "e2"] {
            assert!(
                !pairs.contains_key(local),
                "{local} is in a frozen function: {pairs:?}"
            );
        }
    });
}
