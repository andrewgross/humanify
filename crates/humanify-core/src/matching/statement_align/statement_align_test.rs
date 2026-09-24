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

use super::{
    AlignSide, BodyAlignment, Tokenizer, compute_body_local_transfers, parse_json_unbounded,
};
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
    run: impl FnOnce(&AlignSide<'_, '_>, &AlignSide<'_, '_>) -> T,
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

    let prior_json_index = crate::matching::statement_align::build_json_index(&prior_program_json);
    let fresh_json_index = crate::matching::statement_align::build_json_index(&next_program_json);
    let prior = AlignSide::build(
        &prior_ingest.semantic,
        &prior_tables,
        &prior_json_index,
        prior_row_json,
        prior_span,
    );
    let next = AlignSide::build(
        &next_ingest.semantic,
        &next_tables,
        &fresh_json_index,
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

// ---------------------------------------------------------------------------
// the rest-index shift (found by the WP2.2 close-dump gate, 2026-09-21)
// ---------------------------------------------------------------------------

/// TS ground truth (test/parity/wp22-align-red-probe.mjs): an
/// identical-hash unit FIRST — paired by hash, so the unaligned remainder's
/// positions shift — and a changed container SECOND whose inner statements
/// cannot align. TS aligned=1 in BOTH statement orders (it descends the
/// rest by OBJECT). The shifted-index bug descends the ALREADY-PAIRED first
/// unit instead (rest index 0 → original 0), minting a phantom aligned pair
/// (aligned=2 in order A) and the count changes with the statement order —
/// the shift vanishes when the unpaired unit's rest position happens to
/// equal its original position.
#[test]
fn descends_the_rest_by_object_not_by_shifted_index() {
    const CASES: [(&str, &str, &str); 2] = [
        (
            "log first (rest positions shift)",
            r#"
      function f(input) {
        if (flag) { log("same"); }
        if (check(input, extra)) { return prep(input, more); }
      }"#,
            r#"
      function f(a) {
        if (flag) { log("same"); }
        if (check(a)) { return prep(a, fewer); }
      }"#,
        ),
        (
            "log last (rest positions coincide)",
            r#"
      function f(input) {
        if (check(input, extra)) { return prep(input, more); }
        if (flag) { log("same"); }
      }"#,
            r#"
      function f(a) {
        if (check(a)) { return prep(a, fewer); }
        if (flag) { log("same"); }
      }"#,
        ),
    ];
    for (label, prior_code, next_code) in CASES {
        with_fn_pair(prior_code, next_code, |prior, next| {
            let a = compute_body_local_transfers(prior, next);
            assert_eq!(
                a.aligned_statements, 1,
                "{label}: TS aligned=1 in both orders"
            );
            assert_eq!(a.total_new_statements, 2, "{label}: total=2");
        });
    }
}

/// The walk order for the types both sides key-set-share must be babel's
/// PARSED field order (`Object.keys(babelNode)` in structural-hash.ts's
/// serializeNode), not @babel/types VISITOR_KEYS — the two differ where a
/// non-child scalar sits between the children (`MemberExpression`:
/// object, computed, property — VISITOR_KEYS says [object, property], so
/// the scalar fell to the alphabetical tail AFTER the property token).
/// The k-gram shingle windows sit on token positions, so which side of
/// the differing token `computed:` lands on decides the overlap — this
/// flipped a snap-eligibility verdict on the real 2.1.118→119 pair.
#[test]
fn member_expression_tokens_follow_babel_field_order() {
    let code = r#"class C { #M = 0; m() { this.#M.b; x = y; !a; } }"#;
    let alloc = Allocator::default();
    let ingest = Ingest::parse(&alloc, code, "prior.js");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let tables = SymbolTables::build(&ingest.semantic);

    // The program's ESTree JSON (oxc) — the same substrate the Tokenizer
    // walks; the node shapes are what the TS's babel-parsed Object.keys
    // walk must mirror.
    let program = parse_json_unbounded(&ingest.program.to_estree_json(false, true));
    let find_node = |ty: &str| {
        fn walk<'v>(v: &'v serde_json::Value, ty: &str) -> Option<&'v serde_json::Value> {
            if let Some(map) = v.as_object() {
                if map.get("type").and_then(serde_json::Value::as_str) == Some(ty) {
                    return Some(v);
                }
                for child in map.values() {
                    if let Some(found) = walk(child, ty) {
                        return Some(found);
                    }
                }
            } else if let Some(arr) = v.as_array() {
                for child in arr {
                    if let Some(found) = walk(child, ty) {
                        return Some(found);
                    }
                }
            }
            None
        }
        walk(&program, ty).cloned().expect("node missing")
    };
    let member = find_node("MemberExpression");
    let assignment = find_node("AssignmentExpression");
    let unary = find_node("UnaryExpression");

    // MemberExpression: babel field order object, computed, property —
    // the `computed:` scalar sits BETWEEN the object and the property
    // token, not after it.
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&member, None, "");
    let computed = tok
        .parts
        .iter()
        .position(|p| p == "computed:")
        .expect("computed part missing");
    let prop = tok
        .parts
        .iter()
        .position(|p| p.starts_with("P=#"))
        .expect("private-name token missing");
    assert!(
        computed < prop,
        "babel field order is object, computed, property; got {:?}",
        tok.parts
    );

    // AssignmentExpression: operator, left, right — the operator precedes
    // the children, not the alphabetical tail after them.
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&assignment, None, "");
    let operator = tok
        .parts
        .iter()
        .position(|p| p == "operator:")
        .expect("operator part missing");
    assert_eq!(
        operator, 1,
        "AssignmentExpression field order is operator, left, right; got {:?}",
        tok.parts
    );

    // UnaryExpression: operator, prefix, argument.
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&unary, None, "");
    let operator = tok
        .parts
        .iter()
        .position(|p| p == "operator:")
        .expect("operator part missing");
    let prefix = tok
        .parts
        .iter()
        .position(|p| p == "prefix:")
        .expect("prefix part missing");
    assert_eq!(
        operator, 1,
        "UnaryExpression operator precedes the children; got {:?}",
        tok.parts
    );
    assert!(
        prefix
            < tok
                .parts
                .iter()
                .position(|p| p == "argument:")
                .expect("argument part"),
        "UnaryExpression field order is operator, prefix, argument; got {:?}",
        tok.parts
    );
}

/// oxc's ESTree JSON carries nodes babel's parse does not produce:
/// - `ParenthesizedExpression` (the source's explicit parens) — babel
///   drops parens entirely (no retainParens in the pipeline's parse), so
///   the TS token stream never carries the header or an extra nesting
///   level. The Rust walk must unwrap transparently — the extra
///   `ParenthesizedExpression{`/`expression:` tokens shifted k-gram
///   windows and flipped a snap-eligibility verdict on the real
///   2.1.118→119 pair (0.526 → 0.476 across the 0.5 floor).
/// - `null`/`true`/`false` literals — oxc names them all "Literal" with
///   raw/value keys; babel names them NullLiteral/BooleanLiteral (no raw
///   key, NullLiteral with NO keys). The generic walk over oxc's Literal
///   emitted 4 tokens where TS emits 0–3, moving a jaccard the other way
///   across the same floor on the same pair set.
#[test]
fn parenthesized_and_bool_null_literals_match_babel_shapes() {
    let code = r#"
      async function f(x, y) {
        const a = x ?? (await y); const b = true; const c = null;
      }
    "#;
    let alloc = Allocator::default();
    let ingest = Ingest::parse(&alloc, code, "prior.js");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let tables = SymbolTables::build(&ingest.semantic);
    let program = parse_json_unbounded(&ingest.program.to_estree_json(false, true));

    fn walk<'v>(v: &'v serde_json::Value, ty: &str) -> Option<&'v serde_json::Value> {
        if let Some(map) = v.as_object() {
            if map.get("type").and_then(serde_json::Value::as_str) == Some(ty) {
                return Some(v);
            }
            for child in map.values() {
                if let Some(found) = walk(child, ty) {
                    return Some(found);
                }
            }
        } else if let Some(arr) = v.as_array() {
            for child in arr {
                if let Some(found) = walk(child, ty) {
                    return Some(found);
                }
            }
        }
        None
    }

    // ParenthesizedExpression unwraps to its expression: no paren header,
    // no extra nesting level.
    let logical = walk(&program, "LogicalExpression")
        .cloned()
        .expect("no logical expression");
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&logical, None, "");
    assert!(
        !tok.parts
            .iter()
            .any(|p| p.contains("ParenthesizedExpression")),
        "parens must unwrap transparently; got {:?}",
        tok.parts
    );

    // null → babel's NullLiteral (a bare header, no keys, no raw/value).
    // The FIRST Literal in the fixture is the boolean — search by value.
    fn walk_literal<'v>(
        v: &'v serde_json::Value,
        pred: &impl Fn(&serde_json::Value) -> bool,
    ) -> Option<&'v serde_json::Value> {
        if let Some(map) = v.as_object() {
            if map.get("type").and_then(serde_json::Value::as_str) == Some("Literal") && pred(v) {
                return Some(v);
            }
            for child in map.values() {
                if let Some(found) = walk_literal(child, pred) {
                    return Some(found);
                }
            }
        } else if let Some(arr) = v.as_array() {
            for child in arr {
                if let Some(found) = walk_literal(child, pred) {
                    return Some(found);
                }
            }
        }
        None
    }
    let null_lit = walk_literal(&program, &|v: &serde_json::Value| {
        v.get("value") == Some(&serde_json::Value::Null)
    })
    .cloned()
    .expect("no null literal");
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&null_lit, None, "");
    assert_eq!(
        tok.parts,
        ["NullLiteral{", "}"],
        "null must serialize as babel's NullLiteral; got {:?}",
        tok.parts
    );

    // true → babel's BooleanLiteral{value:...} (no raw key).
    let bool_lit = walk_literal(&program, &|v: &serde_json::Value| {
        matches!(v.get("value"), Some(serde_json::Value::Bool(_)))
    })
    .cloned()
    .expect("no boolean literal");
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&bool_lit, None, "");
    assert_eq!(
        tok.parts,
        ["BooleanLiteral{", "value:", "true", ";", "}"],
        "boolean must serialize as babel's BooleanLiteral; got {:?}",
        tok.parts
    );
}

/// TS `resolveBindingContentPath` (function-graph.ts :409) REFUSES a
/// binding whose declaration is not a ClassDeclaration or a
/// VariableDeclarator — the `if (!bindingPath.isVariableDeclarator())
/// return null` guard fires BEFORE the constantViolations fallback, which
/// only serves a declared-never-initialized var. A destructured PARAM is
/// refused even when the body reassigns it and the assignment RHS would
/// hash identically: the snap gate has no content path. The port fell
/// through to the write-reference path for ANY non-declarator declaration
/// and snapped such hints — 8 of round 3's 9 parity flips (2.1.85, 197,
/// 215; e.g. the `s|allowedTools` hint on 2.1.215's createSkillPrompt).
#[test]
fn param_binding_hint_never_snaps() {
    with_fn_pair(
        r#"
      function makeSkill({ root: rootDir, tools: allowedTools }) {
        allowedTools = allowedTools.map(t => t.replace("${D}", () => rootDir));
        return { tools: allowedTools, root: rootDir };
      }"#,
        r#"
      function makeSkill({ root: _, tools: s }) {
        s = s.map(t => t.replace("${D}", () => _));
        return { tools: s, root: _ };
      }"#,
        |prior, next| {
            let alignment = compute_body_local_transfers(prior, next);
            let hint = alignment
                .hints
                .iter()
                .find(|h| h.new_name == "s")
                .expect("param hint must exist");
            assert_eq!(hint.prior_name, "allowedTools");
            assert!(
                !hint.snap_eligible,
                "a param binding has no content path — TS refuses the snap gate"
            );
        },
    );
}

/// oxc's ESTree "Property" is NEITHER of babel's two object-member nodes:
/// a plain property is babel's ObjectProperty (whose parsed keys carry NO
/// `kind` — babel's parser leaves it unset) and a method/getter/setter is
/// babel's ObjectMethod, where the function fields (id/generator/async/
/// params/body) sit at the TOP level, not under the FunctionExpression oxc
/// nests in `value`. Carrying oxc's shape emitted the wrong header
/// (`Property{`), an extra `kind: "init"` pair on every plain property and
/// a `value: FunctionExpression{...}` nesting babel never has — shifting
/// k-gram windows and flipping a snap verdict on 2.1.197→198 (TS 29/57 =
/// 0.509 vs Rust 29/60 = 0.483, across the 0.5 floor). Pinned against
/// serializePathTokens (probe-prop, 2026-09-21).
#[test]
fn object_property_and_method_nodes_match_babel_shapes() {
    let code = r#"
      function f(u) {
        const cfg = { plain: u, m() { return u; }, get g() { return 2; } };
      }
    "#;
    let alloc = Allocator::default();
    let ingest = Ingest::parse(&alloc, code, "prior.js");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let tables = SymbolTables::build(&ingest.semantic);
    let program = parse_json_unbounded(&ingest.program.to_estree_json(false, true));

    fn find_value(v: &serde_json::Value, ty: &str) -> Option<serde_json::Value> {
        if let Some(map) = v.as_object() {
            if map.get("type").and_then(serde_json::Value::as_str) == Some(ty) {
                return Some(v.clone());
            }
            for child in map.values() {
                if let Some(found) = find_value(child, ty) {
                    return Some(found);
                }
            }
        } else if let Some(arr) = v.as_array() {
            for child in arr {
                if let Some(found) = find_value(child, ty) {
                    return Some(found);
                }
            }
        }
        None
    }
    let object = find_value(&program, "ObjectExpression").expect("object literal");
    let props = object["properties"].as_array().expect("properties").clone();
    assert_eq!(props.len(), 3, "plain, method, getter");

    // Plain property: babel's ObjectProperty — NO kind token.
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&props[0], None, "properties");
    assert_eq!(
        tok.parts,
        [
            "ObjectProperty{",
            "method:",
            "false",
            ";",
            "key:",
            "I=plain",
            ";",
            "computed:",
            "false",
            ";",
            "value:",
            "$0",
            ";",
            "}"
        ],
        "plain property must serialize as babel's ObjectProperty; got {:?}",
        tok.parts
    );

    // Method: babel's ObjectMethod — the function fields flat, kind
    // translated from oxc's "init".
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&props[1], None, "properties");
    assert_eq!(
        tok.parts,
        [
            "ObjectMethod{",
            "method:",
            "true",
            ";",
            "key:",
            "I=m",
            ";",
            "computed:",
            "false",
            ";",
            "kind:",
            "\"method\"",
            ";",
            "id:",
            "null",
            ";",
            "generator:",
            "false",
            ";",
            "async:",
            "false",
            ";",
            "params:",
            "[",
            "]",
            ";",
            "body:",
            "BlockStatement{",
            "body:",
            "[",
            "ReturnStatement{",
            "argument:",
            "$0",
            ";",
            "}",
            ",",
            "]",
            ";",
            "directives:",
            "[",
            "]",
            ";",
            "}",
            ";",
            "}"
        ],
        "object method must serialize as babel's flat ObjectMethod; got {:?}",
        tok.parts
    );

    // Getter: babel's ObjectMethod with kind "get" and method FALSE (oxc
    // says method: false here too, but the flag must come from the KIND —
    // oxc setters carry method: true where babel says false).
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&props[2], None, "properties");
    assert_eq!(
        &tok.parts[..11],
        [
            "ObjectMethod{",
            "method:",
            "false",
            ";",
            "key:",
            "I=g",
            ";",
            "computed:",
            "false",
            ";",
            "kind:"
        ],
        "getter header must be babel's ObjectMethod; got {:?}",
        tok.parts
    );
    assert_eq!(&tok.parts[11..13], ["\"get\"", ";"], "getter kind");
}

/// oxc's ArrowFunctionExpression carries the ESTree `expression` flag
/// (false for block bodies, true for bare ones); THIS babel version leaves
/// the key unset on both — `Object.keys` never yields it, so the TS stream
/// never carries an `expression:` token for an arrow/function (pinned by
/// probe-arrow, 2026-09-21: a bare-body arrow serializes
/// `... ; params: [ ] ; body: N=1 ; }` with NO expression pair). Carrying
/// oxc's two tokens added ~5 shingles per arrow to the union but not the
/// shared set, holding five 2.1.85→86 module-wrapper hints
/// (initializeApp140→K$9 etc.) just above the 0.5 snap floor where the TS
/// sits below it.
#[test]
fn arrow_expression_flag_is_not_emitted() {
    let code = r#"
      function f() {
        let a = () => 1;
        let b = () => { return 2; };
      }
    "#;
    let alloc = Allocator::default();
    let ingest = Ingest::parse(&alloc, code, "prior.js");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let tables = SymbolTables::build(&ingest.semantic);
    let program = parse_json_unbounded(&ingest.program.to_estree_json(false, true));

    fn collect(v: &serde_json::Value, ty: &str, out: &mut Vec<serde_json::Value>) {
        if let Some(map) = v.as_object() {
            if map.get("type").and_then(serde_json::Value::as_str) == Some(ty) {
                out.push(v.clone());
            }
            for child in map.values() {
                collect(child, ty, out);
            }
        } else if let Some(arr) = v.as_array() {
            for child in arr {
                collect(child, ty, out);
            }
        }
    }
    let mut arrows = Vec::new();
    collect(&program, "ArrowFunctionExpression", &mut arrows);
    assert_eq!(arrows.len(), 2, "bare-body and block-body arrows");

    // Bare body: the body value inlines; no expression flag.
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&arrows[0], None, "");
    assert_eq!(
        tok.parts,
        [
            "ArrowFunctionExpression{",
            "id:",
            "null",
            ";",
            "generator:",
            "false",
            ";",
            "async:",
            "false",
            ";",
            "params:",
            "[",
            "]",
            ";",
            "body:",
            "N=1",
            ";",
            "}"
        ],
        "bare-body arrow; got {:?}",
        tok.parts
    );

    // Block body: block carries body then directives: [].
    let mut tok = Tokenizer::new(&tables, true);
    tok.serialize_value(&arrows[1], None, "");
    assert_eq!(
        tok.parts,
        [
            "ArrowFunctionExpression{",
            "id:",
            "null",
            ";",
            "generator:",
            "false",
            ";",
            "async:",
            "false",
            ";",
            "params:",
            "[",
            "]",
            ";",
            "body:",
            "BlockStatement{",
            "body:",
            "[",
            "ReturnStatement{",
            "argument:",
            "N=2",
            ";",
            "}",
            ",",
            "]",
            ";",
            "directives:",
            "[",
            "]",
            ";",
            "}",
            ";",
            "}"
        ],
        "block-body arrow; got {:?}",
        tok.parts
    );
}

/// babel renders an optional chain as Optional* nodes on EVERY link —
/// OptionalMemberExpression with its own `optional` true/false on each
/// member, OptionalCallExpression with `optional` between callee and
/// arguments — and has NO ChainExpression wrapper (probe-chain-keys,
/// 2026-09-21: babel's parsed key orders are [object, computed, property,
/// optional] and [callee, optional, arguments]). oxc wraps the whole chain
/// in ChainExpression and normalizes the links to plain
/// MemberExpression/CallExpression carrying an `optional` bool, which
/// emitted the wrong headers, dropped the per-link `optional` scaffolding
/// and shifted k-gram windows across the 0.5 snap floor — the last
/// 2.1.215→216 flip (e|filteredItems, `Snt?.filter(sjc) ?? []`, Rust
/// jaccard exactly 0.5 → snap TRUE where TS sits below).
/// Chain links keep chain context only along the object/callee spine, and
/// only where it reaches an optional link — `a.b?.c()` renders its object
/// `a.b` as a PLAIN MemberExpression (babel x9), and a paren-terminated
/// chain `(a?.b)()` is a plain CallExpression over an OptionalMemberExpression
/// callee (babel x5).
#[test]
fn optional_chain_nodes_match_babel_shapes() {
    let code = r#"
      function f(a, f2) {
        let x1 = a?.b;
        let x2 = a?.b();
        let x3 = f2?.();
        let x4 = a?.b.c();
        let x5 = (a?.b)();
        let x6 = a?.b?.c();
        let x7 = a.b.c();
        let x8 = a?.b[0];
        let x9 = a.b?.c();
      }
    "#;
    let alloc = Allocator::default();
    let ingest = Ingest::parse(&alloc, code, "prior.js");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let tables = SymbolTables::build(&ingest.semantic);
    let program = parse_json_unbounded(&ingest.program.to_estree_json(false, true));

    fn collect_inits(v: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        if let Some(map) = v.as_object() {
            if map.get("type").and_then(serde_json::Value::as_str) == Some("VariableDeclarator") {
                out.push(map.get("init").cloned().unwrap_or(serde_json::Value::Null));
            }
            for child in map.values() {
                collect_inits(child, out);
            }
        } else if let Some(arr) = v.as_array() {
            for child in arr {
                collect_inits(child, out);
            }
        }
    }
    let mut inits = Vec::new();
    collect_inits(&program, &mut inits);
    assert_eq!(inits.len(), 9, "one init per chain fixture");

    // Slots: `a` ($0, param, first seen in x1), `f2` ($1, param, x3).
    // Member property names are verbatim (I=); the computed [0] keeps its
    // literal (N=0).
    let expected: [&[&str]; 9] = [
        // x1 `a?.b`
        &[
            "OptionalMemberExpression{",
            "object:",
            "$0",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=b",
            ";",
            "optional:",
            "true",
            ";",
            "}",
        ],
        // x2 `a?.b()` — the call is Optional with optional: FALSE
        &[
            "OptionalCallExpression{",
            "callee:",
            "OptionalMemberExpression{",
            "object:",
            "$0",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=b",
            ";",
            "optional:",
            "true",
            ";",
            "}",
            ";",
            "optional:",
            "false",
            ";",
            "arguments:",
            "[",
            "]",
            ";",
            "}",
        ],
        // x3 `f2?.()` — the call itself carries optional: true (each
        // fixture serializes with a FRESH tokenizer, so f2 is $0 here)
        &[
            "OptionalCallExpression{",
            "callee:",
            "$0",
            ";",
            "optional:",
            "true",
            ";",
            "arguments:",
            "[",
            "]",
            ";",
            "}",
        ],
        // x4 `a?.b.c()` — every link is Optional; the outer member is
        // optional: false and the inner carries the chain's true
        &[
            "OptionalCallExpression{",
            "callee:",
            "OptionalMemberExpression{",
            "object:",
            "OptionalMemberExpression{",
            "object:",
            "$0",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=b",
            ";",
            "optional:",
            "true",
            ";",
            "}",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=c",
            ";",
            "optional:",
            "false",
            ";",
            "}",
            ";",
            "optional:",
            "false",
            ";",
            "arguments:",
            "[",
            "]",
            ";",
            "}",
        ],
        // x5 `(a?.b)()` — parens TERMINATE the chain: plain
        // CallExpression, no optional token
        &[
            "CallExpression{",
            "callee:",
            "OptionalMemberExpression{",
            "object:",
            "$0",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=b",
            ";",
            "optional:",
            "true",
            ";",
            "}",
            ";",
            "arguments:",
            "[",
            "]",
            ";",
            "}",
        ],
        // x6 `a?.b?.c()` — two optional links
        &[
            "OptionalCallExpression{",
            "callee:",
            "OptionalMemberExpression{",
            "object:",
            "OptionalMemberExpression{",
            "object:",
            "$0",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=b",
            ";",
            "optional:",
            "true",
            ";",
            "}",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=c",
            ";",
            "optional:",
            "true",
            ";",
            "}",
            ";",
            "optional:",
            "false",
            ";",
            "arguments:",
            "[",
            "]",
            ";",
            "}",
        ],
        // x7 `a.b.c()` — plain members, NO optional tokens at all
        &[
            "CallExpression{",
            "callee:",
            "MemberExpression{",
            "object:",
            "MemberExpression{",
            "object:",
            "$0",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=b",
            ";",
            "}",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=c",
            ";",
            "}",
            ";",
            "arguments:",
            "[",
            "]",
            ";",
            "}",
        ],
        // x8 `a?.b[0]` — computed member on a chain, optional: false
        &[
            "OptionalMemberExpression{",
            "object:",
            "OptionalMemberExpression{",
            "object:",
            "$0",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=b",
            ";",
            "optional:",
            "true",
            ";",
            "}",
            ";",
            "computed:",
            "true",
            ";",
            "property:",
            "N=0",
            ";",
            "optional:",
            "false",
            ";",
            "}",
        ],
        // x9 `a.b?.c()` — the object `a.b` is NOT on the chain: plain
        // MemberExpression inside an OptionalMemberExpression
        &[
            "OptionalCallExpression{",
            "callee:",
            "OptionalMemberExpression{",
            "object:",
            "MemberExpression{",
            "object:",
            "$0",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=b",
            ";",
            "}",
            ";",
            "computed:",
            "false",
            ";",
            "property:",
            "I=c",
            ";",
            "optional:",
            "true",
            ";",
            "}",
            ";",
            "optional:",
            "false",
            ";",
            "arguments:",
            "[",
            "]",
            ";",
            "}",
        ],
    ];
    for (i, init) in inits.iter().enumerate() {
        let mut tok = Tokenizer::new(&tables, true);
        tok.serialize_value(init, None, "");
        assert_eq!(
            tok.parts,
            expected[i],
            "fixture x{}; got {:?}",
            i + 1,
            tok.parts
        );
    }
}
