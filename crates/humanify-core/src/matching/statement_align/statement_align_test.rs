//! `computeBodyLocalTransfers`'s tests, ported from
//! `src/prior-version/statement-align.test.ts` fixture-for-fixture (each
//! test's doc comment quotes the TS case). The harness resolves each
//! fixture's single top-level function the way the TS `fnOf` helper does —
//! the function row whose parent is the Program — and builds the pair's
//! [`AlignSide`]s from the real graph rows.

use std::collections::HashMap;

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_span::Span;

use crate::graph::UnifiedGraph;
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;

use super::{AlignSide, BodyAlignment, compute_body_local_transfers, parse_json_unbounded};
use crate::matching::features::row_estree_json;

/// TS `transferMap`: minified name → the prior name its binding inherits.
fn transfer_map(a: &BodyAlignment) -> HashMap<String, String> {
    a.transfers
        .iter()
        .map(|t| (t.old_name.clone(), t.new_name.clone()))
        .collect()
}

/// TS `hintMap`: minified name → prior-name hint.
fn hint_map(a: &BodyAlignment) -> HashMap<String, String> {
    a.hints
        .iter()
        .map(|h| (h.new_name.clone(), h.prior_name.clone()))
        .collect()
}

/// TS `snapMap`: the hints whose definition still corroborates.
fn snap_map(a: &BodyAlignment) -> HashMap<String, String> {
    a.hints
        .iter()
        .filter(|h| h.snap_eligible)
        .map(|h| (h.new_name.clone(), h.prior_name.clone()))
        .collect()
}

/// TS `fnOf`: the single top-level function of a fixture as an alignment
/// side (the function row whose parent is the Program).
fn with_fn_pair<T>(
    prior_code: &str,
    next_code: &str,
    run: impl FnOnce(&AlignSide<'_>, &AlignSide<'_>) -> T,
) -> T {
    let prior_alloc = Allocator::default();
    let next_alloc = Allocator::default();
    let prior_ingest = Ingest::parse(&prior_alloc, prior_code, "test.js");
    assert!(
        prior_ingest.errors.is_empty(),
        "prior must parse: {:?}",
        prior_ingest.errors
    );
    let next_ingest = Ingest::parse(&next_alloc, next_code, "test.js");
    assert!(
        next_ingest.errors.is_empty(),
        "next must parse: {:?}",
        next_ingest.errors
    );

    let prior_tables = SymbolTables::build(&prior_ingest.semantic);
    let next_tables = SymbolTables::build(&next_ingest.semantic);
    let prior_graph = crate::graph::build_unified_graph(
        &prior_ingest.semantic,
        prior_ingest.program,
        "test.js",
        &[],
        None,
        None,
    );
    let next_graph = crate::graph::build_unified_graph(
        &next_ingest.semantic,
        next_ingest.program,
        "test.js",
        &[],
        None,
        None,
    );
    let prior_program_json =
        parse_json_unbounded(&prior_ingest.program.to_estree_json(false, true));
    let next_program_json = parse_json_unbounded(&next_ingest.program.to_estree_json(false, true));

    let (prior_row_json, prior_span) = top_level_fn_row(&prior_ingest, &prior_graph);
    let (next_row_json, next_span) = top_level_fn_row(&next_ingest, &next_graph);

    let prior = AlignSide::build(
        &prior_ingest.semantic,
        &prior_tables,
        prior_program_json,
        prior_row_json,
        prior_span,
    );
    let next = AlignSide::build(
        &next_ingest.semantic,
        &next_tables,
        next_program_json,
        next_row_json,
        next_span,
    );
    run(&prior, &next)
}

/// The graph function row whose arena node's parent is the Program (the TS
/// `functions.find(f => f.path.parentPath?.isProgram())`), as its row JSON
/// and row span.
fn top_level_fn_row(ingest: &Ingest<'_>, graph: &UnifiedGraph) -> (serde_json::Value, Span) {
    let rows = crate::matching::row_node_ids(&graph.functions, ingest.semantic.nodes());
    let nodes = ingest.semantic.nodes();
    for f in &graph.functions {
        let Some(&(node_id, kind)) = rows.get(&(f.span.start, f.span.end)) else {
            continue;
        };
        let parent = nodes.parent_id(node_id);
        if matches!(nodes.get_node(parent).kind(), AstKind::Program(_))
            && let Some(json) = row_estree_json(kind)
        {
            return (parse_json_unbounded(&json), f.span);
        }
    }
    panic!("no top-level function in fixture");
}

// ---------------------------------------------------------------------------
// deep-branch anchoring
// ---------------------------------------------------------------------------

// The 2.1.166→167 transport function: locals churned because they sit in
// the 3rd+ branch of an else-if chain inside a try — beyond the old
// recursion budget. When an edit lands deep in ONE branch, the untouched
// sibling branches' locals must still transfer.
const PRIOR_DEEP: &str = r#"
    function connect(cfg, mk) {
      try {
        if (cfg.kind === "sse") {
          let sseOptions = { url: cfg.url, retry: true };
          mk.start(sseOptions);
        } else if (cfg.kind === "ws") {
          let wsSocket = mk.openSocket(cfg.url);
          mk.attach(wsSocket);
        } else if (cfg.kind === "http") {
          let httpHeaders = mk.buildHeaders(cfg);
          mk.request(cfg.url, httpHeaders);
        } else if (cfg.kind === "stdio") {
          let stdioEnv = mk.mergeEnv(cfg);
          mk.spawn(cfg.cmd, stdioEnv);
        } else {
          throw Error("nope");
        }
      } finally {
        mk.done();
      }
    }"#;

/// TS "transfers locals from deep else-if branches when the edit is in the
/// deepest branch": same shape, minified names, and a REAL edit (extra
/// statement) in the LAST branch — every earlier branch aligns and must
/// carry its local.
#[test]
fn transfers_locals_from_deep_else_if_branches() {
    with_fn_pair(
        PRIOR_DEEP,
        r#"
      function connect(a, b) {
        try {
          if (a.kind === "sse") {
            let q = { url: a.url, retry: true };
            b.start(q);
          } else if (a.kind === "ws") {
            let w = b.openSocket(a.url);
            b.attach(w);
          } else if (a.kind === "http") {
            let h = b.buildHeaders(a);
            b.request(a.url, h);
          } else if (a.kind === "stdio") {
            let s = b.mergeEnv(a);
            b.audit(s);
            b.spawn(a.cmd, s);
          } else {
            throw Error("nope");
          }
        } finally {
          b.done();
        }
      }"#,
        |prior, next| {
            let transfers = transfer_map(&compute_body_local_transfers(prior, next));
            assert_eq!(
                transfers.get("q").map(String::as_str),
                Some("sseOptions"),
                "sse branch local"
            );
            assert_eq!(
                transfers.get("w").map(String::as_str),
                Some("wsSocket"),
                "ws branch local"
            );
            assert_eq!(
                transfers.get("h").map(String::as_str),
                Some("httpHeaders"),
                "http branch local (beyond the old depth budget)"
            );
        },
    );
}

/// TS "transfers locals from sibling cases when the edit is inside one
/// switch case". (The `d` case is edited too — `k.trace(d)` inserted — and
/// the TS deliberately does NOT assert it; its read statement still aligns,
/// so `d` transfers as well, but the assertion pins only what the TS does.)
#[test]
fn transfers_locals_from_sibling_switch_cases() {
    with_fn_pair(
        r#"
      function route(msg, h) {
        switch (msg.tag) {
          case "open": {
            let openPayload = h.decode(msg.body);
            h.onOpen(openPayload);
            break;
          }
          case "data": {
            let dataChunk = h.read(msg.body);
            h.onData(dataChunk);
            break;
          }
          case "close": {
            let closeCode = h.code(msg);
            h.onClose(closeCode);
            break;
          }
        }
      }"#,
        r#"
      function route(m, k) {
        switch (m.tag) {
          case "open": {
            let o = k.decode(m.body);
            k.onOpen(o);
            break;
          }
          case "data": {
            let d = k.read(m.body);
            k.trace(d);
            k.onData(d);
            break;
          }
          case "close": {
            let c = k.code(m);
            k.onClose(c);
            break;
          }
        }
      }"#,
        |prior, next| {
            let transfers = transfer_map(&compute_body_local_transfers(prior, next));
            assert_eq!(
                transfers.get("o").map(String::as_str),
                Some("openPayload"),
                "open case local"
            );
            assert_eq!(
                transfers.get("c").map(String::as_str),
                Some("closeCode"),
                "close case local"
            );
        },
    );
}

/// TS "descends multiple changed containers when their types pair
/// unambiguously": BOTH containers edited (one line each) — the if and the
/// try both fail to align as wholes, but they pair 1:1 by node type, and
/// their untouched inner statements still align.
#[test]
fn descends_multiple_changed_containers() {
    with_fn_pair(
        r#"
      function work(cfg, io) {
        if (cfg.fast) {
          let fastQueue = io.queue(cfg);
          io.push(fastQueue);
          io.flush(cfg.now);
        }
        try {
          let retryBudget = io.budget(cfg);
          io.consume(retryBudget);
          io.log(cfg.tag);
        } finally {
          io.done();
        }
      }"#,
        r#"
      function work(a, b) {
        if (a.fast) {
          let f = b.queue(a);
          b.push(f);
          b.flushAll(a.now);
        }
        try {
          let r = b.budget(a);
          b.consume(r);
          b.logSlow(a.tag);
        } finally {
          b.done();
        }
      }"#,
        |prior, next| {
            let transfers = transfer_map(&compute_body_local_transfers(prior, next));
            assert_eq!(
                transfers.get("f").map(String::as_str),
                Some("fastQueue"),
                "if-container local"
            );
            assert_eq!(
                transfers.get("r").map(String::as_str),
                Some("retryBudget"),
                "try-container local"
            );
        },
    );
}

/// TS "does not pair changed same-type siblings (ambiguous
/// correspondence)": TWO changed if-statements at the same level — pairing
/// them by position would be a guess. Locals inside them must NOT transfer.
#[test]
fn does_not_pair_changed_same_type_siblings() {
    with_fn_pair(
        r#"
      function pick(cfg, io) {
        if (cfg.a) {
          let alphaBox = io.box(cfg.a);
          io.send(alphaBox, cfg.k1);
        }
        if (cfg.b) {
          let betaBox = io.box(cfg.b);
          io.send(betaBox, cfg.k2);
        }
      }"#,
        r#"
      function pick(c, o) {
        if (c.a) {
          let x = o.box(c.a);
          o.sendFast(x, c.k1);
        }
        if (c.b) {
          let y = o.box(c.b);
          o.sendFast(y, c.k2);
        }
      }"#,
        |prior, next| {
            let transfers = transfer_map(&compute_body_local_transfers(prior, next));
            assert!(
                !transfers.contains_key("x"),
                "ambiguous sibling must not transfer, got {transfers:?}"
            );
            assert!(
                !transfers.contains_key("y"),
                "ambiguous sibling must not transfer, got {transfers:?}"
            );
        },
    );
}

// ---------------------------------------------------------------------------
// per-identifier hints (A1)
// ---------------------------------------------------------------------------

// A local whose DECLARATION statement changed shape cannot be safely
// auto-transferred (its defining content is not provably unchanged), but
// its prior name is still known from aligned USE-sites. That name is a
// valid LLM hint even though it fails the auto-transfer precision gate.
const PRIOR_HINT: &str = r#"
    function process(input) {
      let result = compute(input);
      log(result);
      return result;
    }"#;
const NEXT_HINT: &str = r#"
    function process(a) {
      let b = compute(normalize(a));
      log(b);
      return b;
    }"#;

/// TS "hints an own-scope local known only from aligned use-sites".
#[test]
fn hints_an_own_scope_local_known_only_from_use_sites() {
    with_fn_pair(PRIOR_HINT, NEXT_HINT, |prior, next| {
        let hints = hint_map(&compute_body_local_transfers(prior, next));
        assert_eq!(
            hints.get("b").map(String::as_str),
            Some("result"),
            "use-site-only local b should be hinted result, got {hints:?}"
        );
    });
}

/// TS "does NOT auto-transfer a local whose declaration statement changed":
/// the hint exists but the transfer must not — the declaration
/// `let b = compute(normalize(a))` did not align.
#[test]
fn does_not_auto_transfer_a_changed_declaration() {
    with_fn_pair(PRIOR_HINT, NEXT_HINT, |prior, next| {
        let transfers = transfer_map(&compute_body_local_transfers(prior, next));
        assert!(
            !transfers.contains_key("b"),
            "use-site-only local must not be auto-transferred (precision gate), got {transfers:?}"
        );
    });
}

/// TS "does not hint bindings owned by nested functions": `helper`'s own
/// param `n`/`z` must not be hinted for the OUTER function.
#[test]
fn does_not_hint_nested_function_bindings() {
    with_fn_pair(
        r#"
      function outer(input) {
        let total = seed(input);
        function helper(count) { return count + total; }
        return helper(total);
      }"#,
        r#"
      function outer(a) {
        let total = seed(reshape(a));
        function helper(z) { return z + total; }
        return helper(total);
      }"#,
        |prior, next| {
            let hints = hint_map(&compute_body_local_transfers(prior, next));
            assert!(
                !hints.contains_key("z"),
                "nested-function-owned binding must not be hinted for the outer function, got {hints:?}"
            );
        },
    );
}

// ---------------------------------------------------------------------------
// snap eligibility (A2)
// ---------------------------------------------------------------------------

/// TS "marks a use-site hint snap-eligible when the definition is
/// unchanged": caughtError's DECLARATION does not align (its
/// `let _ = decode(_)` group has count 2 in prior, 1 in next), so it
/// reaches the LLM as a hint — but its init `decode(input)` is
/// rename-identical to next's `decode(a)`, so the binding's role provably
/// held. That corroboration makes it a snap.
#[test]
fn marks_a_hint_snap_eligible_when_the_definition_is_unchanged() {
    with_fn_pair(
        r#"
      function handle(input) {
        let caughtError = decode(input);
        let scratch = decode(input);
        report(caughtError);
      }"#,
        r#"
      function handle(a) {
        let x = decode(a);
        report(x);
      }"#,
        |prior, next| {
            let snaps = snap_map(&compute_body_local_transfers(prior, next));
            assert_eq!(
                snaps.get("x").map(String::as_str),
                Some("caughtError"),
                "unchanged definition should be snap-eligible, got {snaps:?}"
            );
        },
    );
}

/// TS "does NOT mark snap-eligible when the definition materially changed":
/// b's declaration gained a `normalize(...)` wrapper — its content no
/// longer corroborates `result`, so the name is a hint the LLM may
/// override but NOT a forced snap (that would risk a repurposed-binding
/// mispin).
#[test]
fn does_not_mark_snap_eligible_when_the_definition_changed() {
    with_fn_pair(PRIOR_HINT, NEXT_HINT, |prior, next| {
        let alignment = compute_body_local_transfers(prior, next);
        let hint = alignment
            .hints
            .iter()
            .find(|h| h.new_name == "b")
            .expect("b should still be a plain hint");
        assert!(
            !hint.snap_eligible,
            "materially changed definition must not be snap-eligible"
        );
    });
}

// ---------------------------------------------------------------------------
// bare-let bindings defined by a separate assignment (exp080)
// ---------------------------------------------------------------------------

/// TS "auto-transfers when the DEFINING assignment is in an aligned
/// statement": a binding declared `let X;` and assigned separately — the
/// assignment is the definition, and when that statement aligns the
/// definition is provably unchanged. Real case:
/// `isDeferredMcpRequestPresent` -> `containerBox`.
#[test]
fn auto_transfers_bare_let_when_the_defining_assignment_aligned() {
    with_fn_pair(
        r#"function host(input) {
      let isDeferredMcpRequestPresent;
      isDeferredMcpRequestPresent = input.some(checkDeferred);
      return isDeferredMcpRequestPresent;
    }"#,
        r#"function host(a) {
      let b;
      b = a.some(checkDeferred);
      return b;
    }"#,
        |prior, next| {
            let alignment = compute_body_local_transfers(prior, next);
            let applied = alignment
                .transfers
                .iter()
                .find(|t| t.old_name == "b")
                .expect(
                    "the binding's defining assignment aligned, so its prior name must be \
                     APPLIED, not merely hinted",
                );
            assert_eq!(applied.new_name, "isDeferredMcpRequestPresent");
        },
    );
}

/// TS "still transfers the untouched locals when ONE declarator is
/// inserted" — THE REAL SHAPE: the failing file declares 24 locals in ONE
/// statement and the next release inserts two more; that statement's
/// declarator count changes, it cannot align, and NONE of the bindings it
/// declares are anchored by declaration. They all fall to local-use —
/// except through their defining ASSIGNMENTS, which still align. (Real
/// case: `columnCache` -> `hasAnyToolsFlag`.)
#[test]
fn still_transfers_untouched_locals_when_one_declarator_is_inserted() {
    with_fn_pair(
        r#"function host(input) {
      let firstFlag, secondFlag, thirdFlag;
      firstFlag = input.a();
      secondFlag = input.b();
      thirdFlag = input.c();
      return [firstFlag, secondFlag, thirdFlag];
    }"#,
        r#"function host(q) {
      let m, inserted, n, o;
      m = q.a();
      inserted = q.d();
      n = q.b();
      o = q.c();
      return [m, n, o];
    }"#,
        |prior, next| {
            let alignment = compute_body_local_transfers(prior, next);
            let names: Vec<String> = alignment
                .transfers
                .iter()
                .map(|t| format!("{}->{}", t.old_name, t.new_name))
                .collect();
            assert!(
                names.iter().any(|n| n == "m->firstFlag"),
                "inserting one declarator must not cost the others their names. \
                 transfers={names:?} hints={:?}",
                alignment
                    .hints
                    .iter()
                    .map(|h| &h.new_name)
                    .collect::<Vec<_>>()
            );
        },
    );
}

// ---------------------------------------------------------------------------
// PARITY against the frozen TS probe (`test/parity/wp22-probe.mjs`, frozen
// at `test/parity/wp22-synthetic.json`): every fixture's transfers, hints
// (WITH the snap-eligibility flags) and the two counters, in ORDER — the
// evidence vectors are insertion-ordered, so list equality is the contract,
// not the map views the individual tests above use.
// ---------------------------------------------------------------------------

/// The frozen probe output, parsed once.
fn frozen_align() -> serde_json::Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/parity/wp22-synthetic.json"
    ))
    .expect("the WP2.2 probe output must be frozen at test/parity/wp22-synthetic.json");
    serde_json::from_str::<serde_json::Value>(&raw).expect("probe JSON parses")["align"].clone()
}

/// The alignment as the probe froze it: ordered (oldName, newName) pairs,
/// ordered (newName, priorName, snapEligible) hints, and the two counts.
type AlignmentShape = (
    Vec<(String, String)>,
    Vec<(String, String, bool)>,
    usize,
    usize,
);

fn alignment_shape(a: &BodyAlignment) -> AlignmentShape {
    (
        a.transfers
            .iter()
            .map(|t| (t.old_name.clone(), t.new_name.clone()))
            .collect(),
        a.hints
            .iter()
            .map(|h| (h.new_name.clone(), h.prior_name.clone(), h.snap_eligible))
            .collect(),
        a.aligned_statements,
        a.total_new_statements,
    )
}

fn frozen_shape(case: &serde_json::Value) -> AlignmentShape {
    (
        case["transfers"]
            .as_array()
            .expect("transfers array")
            .iter()
            .map(|t| {
                (
                    t["oldName"].as_str().expect("oldName").to_string(),
                    t["newName"].as_str().expect("newName").to_string(),
                )
            })
            .collect(),
        case["hints"]
            .as_array()
            .expect("hints array")
            .iter()
            .map(|h| {
                (
                    h["newName"].as_str().expect("newName").to_string(),
                    h["priorName"].as_str().expect("priorName").to_string(),
                    h["snapEligible"].as_bool().expect("snapEligible"),
                )
            })
            .collect(),
        case["alignedStatements"].as_u64().expect("aligned") as usize,
        case["totalNewStatements"].as_u64().expect("total") as usize,
    )
}

#[test]
fn parity_alignments_match_the_frozen_probe_exactly() {
    let frozen = frozen_align();

    // (label, prior code, next code) — the SAME fixtures the probe froze.
    let cases: Vec<(&str, &str, &str)> = vec![
        (
            "deep_branches",
            PRIOR_DEEP,
            r#"
      function connect(a, b) {
        try {
          if (a.kind === "sse") {
            let q = { url: a.url, retry: true };
            b.start(q);
          } else if (a.kind === "ws") {
            let w = b.openSocket(a.url);
            b.attach(w);
          } else if (a.kind === "http") {
            let h = b.buildHeaders(a);
            b.request(a.url, h);
          } else if (a.kind === "stdio") {
            let s = b.mergeEnv(a);
            b.audit(s);
            b.spawn(a.cmd, s);
          } else {
            throw Error("nope");
          }
        } finally {
          b.done();
        }
      }"#,
        ),
        (
            "switch_cases",
            r#"
      function route(msg, h) {
        switch (msg.tag) {
          case "open": {
            let openPayload = h.decode(msg.body);
            h.onOpen(openPayload);
            break;
          }
          case "data": {
            let dataChunk = h.read(msg.body);
            h.onData(dataChunk);
            break;
          }
          case "close": {
            let closeCode = h.code(msg);
            h.onClose(closeCode);
            break;
          }
        }
      }"#,
            r#"
      function route(m, k) {
        switch (m.tag) {
          case "open": {
            let o = k.decode(m.body);
            k.onOpen(o);
            break;
          }
          case "data": {
            let d = k.read(m.body);
            k.trace(d);
            k.onData(d);
            break;
          }
          case "close": {
            let c = k.code(m);
            k.onClose(c);
            break;
          }
        }
      }"#,
        ),
        (
            "two_changed_containers",
            r#"
      function work(cfg, io) {
        if (cfg.fast) {
          let fastQueue = io.queue(cfg);
          io.push(fastQueue);
          io.flush(cfg.now);
        }
        try {
          let retryBudget = io.budget(cfg);
          io.consume(retryBudget);
          io.log(cfg.tag);
        } finally {
          io.done();
        }
      }"#,
            r#"
      function work(a, b) {
        if (a.fast) {
          let f = b.queue(a);
          b.push(f);
          b.flushAll(a.now);
        }
        try {
          let r = b.budget(a);
          b.consume(r);
          b.logSlow(a.tag);
        } finally {
          b.done();
        }
      }"#,
        ),
        (
            "same_type_siblings",
            r#"
      function pick(cfg, io) {
        if (cfg.a) {
          let alphaBox = io.box(cfg.a);
          io.send(alphaBox, cfg.k1);
        }
        if (cfg.b) {
          let betaBox = io.box(cfg.b);
          io.send(betaBox, cfg.k2);
        }
      }"#,
            r#"
      function pick(c, o) {
        if (c.a) {
          let x = o.box(c.a);
          o.sendFast(x, c.k1);
        }
        if (c.b) {
          let y = o.box(c.b);
          o.sendFast(y, c.k2);
        }
      }"#,
        ),
        ("use_site_hint", PRIOR_HINT, NEXT_HINT),
        (
            "nested_binding",
            r#"
      function outer(input) {
        let total = seed(input);
        function helper(count) { return count + total; }
        return helper(total);
      }"#,
            r#"
      function outer(a) {
        let total = seed(reshape(a));
        function helper(z) { return z + total; }
        return helper(total);
      }"#,
        ),
        (
            "snap_eligible",
            r#"
      function handle(input) {
        let caughtError = decode(input);
        let scratch = decode(input);
        report(caughtError);
      }"#,
            r#"
      function handle(a) {
        let x = decode(a);
        report(x);
      }"#,
        ),
        (
            "bare_let_single",
            r#"function host(input) {
      let isDeferredMcpRequestPresent;
      isDeferredMcpRequestPresent = input.some(checkDeferred);
      return isDeferredMcpRequestPresent;
    }"#,
            r#"function host(a) {
      let b;
      b = a.some(checkDeferred);
      return b;
    }"#,
        ),
        (
            "bare_let_multi_declarator",
            r#"function host(input) {
      let firstFlag, secondFlag, thirdFlag;
      firstFlag = input.a();
      secondFlag = input.b();
      thirdFlag = input.c();
      return [firstFlag, secondFlag, thirdFlag];
    }"#,
            r#"function host(q) {
      let m, inserted, n, o;
      m = q.a();
      inserted = q.d();
      n = q.b();
      o = q.c();
      return [m, n, o];
    }"#,
        ),
    ];

    for (label, prior_code, next_code) in cases {
        let case = &frozen[label];
        with_fn_pair(prior_code, next_code, |prior, next| {
            let got = alignment_shape(&compute_body_local_transfers(prior, next));
            let want = frozen_shape(case);
            assert_eq!(got.0, want.0, "{label}: transfers (ordered)");
            assert_eq!(got.1, want.1, "{label}: hints (ordered, with snap flags)");
            assert_eq!(got.2, want.2, "{label}: alignedStatements");
            assert_eq!(got.3, want.3, "{label}: totalNewStatements");
        });
    }
}
