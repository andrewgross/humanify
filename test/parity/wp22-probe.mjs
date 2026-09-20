// probe: WP2.2 ground truth — freeze the TS close-match + statement-alignment
// outputs the Rust port must reproduce EXACTLY:
//
//   cosine  — raw cosine scores over synthetic feature vectors (one pair per
//             scorePairs call, threshold 0, so no top-K capping hides a
//             score). JSON numbers are shortest-roundtrip, so parsing one
//             back yields the identical f64 bits — the frozen value IS the
//             bit pattern the Rust side must produce.
//   matrix  — scorePairs over a 4x4 synthetic matrix with tied and distinct
//             vectors: the full candidate list in ORDER (insertion order is
//             load-bearing; ties keep first-seen first) with the top-K cap.
//   assign  — findCloseMatches over REAL code fixtures: tie abstention
//             (contested), disjoint exact ties (still match), best pick,
//             threshold behavior — pairs + frozen score bits + skip counters.
//   align   — computeBodyLocalTransfers over the statement-align suite's
//             fixtures: transfers, hints (with snapEligible), the
//             aligned/total statement counts.
//
// Run: npx tsx test/parity/wp22-probe.mjs > test/parity/wp22-synthetic.json
import assert from "node:assert";
import { parseSync } from "@babel/core";

// Every frozen score is emitted BOTH as a decimal (display) and as the
// f64 BIT pattern (hex). The decimal is shortest-roundtrip — a correctly
// rounding parser reads back the identical bits — but serde_json's default
// float parser is up to 1 ulp off on full-precision literals, so the
// Rust-side parity tests compare the hex, not the decimal.
const f64Scratch = new Float64Array(1);
const u64Scratch = new BigUint64Array(f64Scratch.buffer);
function bits(score) {
  f64Scratch[0] = score;
  return u64Scratch[0].toString(16).padStart(16, "0");
}

const { scorePairs, findCloseMatches, CLOSE_MATCH_TOP_K } =
  await import("../../src/analysis/close-match.js");
const { buildFunctionGraph } = await import("../../src/analysis/function-graph.js");
const { buildFingerprintIndex, matchFunctions } = await import(
  "../../src/analysis/fingerprint-index.js"
);
const { computeBodyLocalTransfers } = await import(
  "../../src/prior-version/statement-align.js"
);

function parse(code) {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse code");
  return ast;
}

function buildIndex(code) {
  const functions = buildFunctionGraph(parse(code), "test.js");
  const map = new Map(functions.map((f) => [f.sessionId, f]));
  return buildFingerprintIndex(map);
}

// ---------------------------------------------------------------------------
// cosine — one (old, new) pair per call so every raw score is visible.
// ---------------------------------------------------------------------------

// The suite's 50x50 test vector, plus deliberately-shaped vectors:
// V2 and V4 have PERFECT-SQUARE norms (4 and 16), so sqrt is exact and
// cos(V,V) is exactly 1.0 — the exact tie the assignment cases need.
const VEC = {
  suiteVector: {
    arity: 1,
    complexity: 2,
    returnCount: 1,
    loopCount: 0,
    branchCount: 1,
    tryCount: 0,
    calleeCount: 1,
    externalCallCount: 1,
    stringLiteralCount: 1,
    propertyAccessCount: 2,
    numericLiteralCount: 0,
    hasRestParam: 0
  },
  // complex body: every feature non-zero (no exact sqrt expected)
  rich: {
    arity: 3,
    complexity: 7,
    returnCount: 2,
    loopCount: 2,
    branchCount: 4,
    tryCount: 1,
    calleeCount: 5,
    externalCallCount: 3,
    stringLiteralCount: 6,
    propertyAccessCount: 9,
    numericLiteralCount: 4,
    hasRestParam: 1
  },
  // norm^2 == 4 -> exact sqrt
  v2: { arity: 2, complexity: 0, returnCount: 0, loopCount: 0, branchCount: 0, tryCount: 0, calleeCount: 0, externalCallCount: 0, stringLiteralCount: 0, propertyAccessCount: 0, numericLiteralCount: 0, hasRestParam: 0 },
  // norm^2 == 16 -> exact sqrt
  v4: { arity: 0, complexity: 4, returnCount: 0, loopCount: 0, branchCount: 0, tryCount: 0, calleeCount: 0, externalCallCount: 0, stringLiteralCount: 0, propertyAccessCount: 0, numericLiteralCount: 0, hasRestParam: 0 },
  zeros: {
    arity: 0, complexity: 0, returnCount: 0, loopCount: 0, branchCount: 0, tryCount: 0, calleeCount: 0, externalCallCount: 0, stringLiteralCount: 0, propertyAccessCount: 0, numericLiteralCount: 0, hasRestParam: 0
  },
  disjoint: {
    arity: 0, complexity: 0, returnCount: 0, loopCount: 3, branchCount: 0, tryCount: 0, calleeCount: 0, externalCallCount: 0, stringLiteralCount: 0, propertyAccessCount: 0, numericLiteralCount: 0, hasRestParam: 0
  }
};

const cosine = {};
for (const [label, [aName, bName]] of Object.entries({
  identical: ["suiteVector", "suiteVector"],
  identicalRich: ["rich", "rich"],
  scaled2x: ["suiteVector", "rich"],
  zeroZero: ["zeros", "zeros"],
  zeroNonzero: ["zeros", "rich"],
  orthogonal: ["suiteVector", "disjoint"],
  exactTieA: ["v2", "v2"],
  exactTieB: ["v4", "v4"],
  crossExact: ["v2", "v4"],
  richVsSuite: ["rich", "suiteVector"]
})) {
  const a = VEC[aName];
  const b = VEC[bName];
  const candidates = scorePairs(
    new Map([["a", a]]),
    new Map([["b", b]]),
    0.0
  );
  assert.strictEqual(candidates.length, 1, `${label}: one candidate`);
  cosine[label] = {
    a: aName,
    b: bName,
    score: candidates[0].score,
    scoreBits: bits(candidates[0].score)
  };
}

// ---------------------------------------------------------------------------
// matrix — scorePairs over a 4x4 with ties and distinct scores. Order is
// part of the contract (insertion order + stable top-K).
// ---------------------------------------------------------------------------

const rawMatrix = scorePairs(
  new Map([
    ["o1", VEC.v2],
    ["o2", VEC.v4],
    ["o3", VEC.suiteVector],
    ["o4", VEC.rich]
  ]),
  new Map([
    ["n1", VEC.v2],
    ["n2", VEC.v4],
    ["n3", VEC.suiteVector],
    ["n4", VEC.disjoint]
  ]),
  0.5
);
const matrix = rawMatrix.map((c) => ({ ...c, scoreBits: bits(c.score) }));

// ---------------------------------------------------------------------------
// assign — findCloseMatches over real code fixtures.
// ---------------------------------------------------------------------------

const assign = {};

function closeRun(label, oldCode, newCode, unmatchedOld, unmatchedNew, threshold) {
  const oldIndex = buildIndex(oldCode);
  const newIndex = buildIndex(newCode);
  const result = matchFunctions(oldIndex, newIndex);
  const claimed = new Set([...result.matches.values()]);
  const olds =
    unmatchedOld === "ALL"
      ? [...oldIndex.fingerprints.keys()]
      : (unmatchedOld ?? result.unmatched);
  const news =
    unmatchedNew === "ALL"
      ? [...newIndex.fingerprints.keys()]
      : (unmatchedNew ??
        [...newIndex.fingerprints.keys()].filter((id) => !claimed.has(id)));
  const close = findCloseMatches(olds, news, oldIndex, newIndex, { threshold });
  assign[label] = {
    pairs: [...close.closeMatches.entries()].map(([oldId, newId]) => ({
      oldId,
      newId,
      score: close.scores.get(oldId),
      scoreBits: bits(close.scores.get(oldId))
    })),
    skippedOld: close.skippedOld,
    skippedNew: close.skippedNew
  };
}

// Contested exact tie (one old, two identical new) — nothing may match.
closeRun(
  "contested_tie_abstains",
  "function only(x) { if (x) { return svc(x); } return 0; }",
  'function n1(x) { if (x) { return svc(x); } return 0; }\nfunction n2(x) { if (x) { return svc(x); } return 0; }',
  undefined,
  undefined,
  0.8
);

// DISJOINT independent matches: two old / two new, every cross pair under
// the threshold, each side's best pair well above it — BOTH assigned (the
// tie-abstention logic only fires on EQUAL scores; here the two winners
// are distinct floats and the frozen bits pin that).
closeRun(
  "disjoint_pairs_both_match",
  'function aa(x, y) { }\nfunction bb(o) { o.a; o.b; }',
  'function m(x, y) { }\nfunction n(o) { o.x; o.y; }',
  "ALL",
  "ALL",
  0.8
);

// The suite's one-statement-diff pair: score pinned (must exceed 0.5).
closeRun(
  "one_statement_diff",
  `
      function process(x) {
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
    `,
  `
      function process(x) {
        console.log("debug");
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
    `,
  undefined,
  undefined,
  0.8
);

// Threshold behavior on the calc fixture: 0.99 rejects, 0.3 accepts.
const calcOld = `
      function calc(x) {
        return x + 1;
      }
    `;
const calcNew = `
      function calc(x) {
        for (var i = 0; i < x; i++) {
          if (i > 5) return i;
        }
        return x + 1;
      }
    `;
closeRun("threshold_strict_rejects", calcOld, calcNew, undefined, undefined, 0.99);
closeRun("threshold_relaxed_accepts", calcOld, calcNew, undefined, undefined, 0.3);

// Best pick: one old, two new candidates of different similarity.
closeRun(
  "best_pick",
  `
      function process(x) {
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
    `,
  `
      function processV2(x) {
        console.log("start");
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
      function totallyDifferent(a, b, c) {
        try { return a + b + c; } catch(e) { return 0; }
      }
    `,
  undefined,
  undefined,
  0.3
);

// Featureless (binding) ids are skipped, not scored — the counters are the
// observable. The BINDING index comes from the unified graph's module
// bindings (buildBindingFullFingerprint does not populate `features`).
{
  const { buildUnifiedGraph } = await import("../../src/analysis/function-graph.js");
  const { buildBindingFingerprintIndex } = await import(
    "../../src/analysis/fingerprint-index.js"
  );
  const bindingSides = (code) => {
    const unified = buildUnifiedGraph(parse(code), "test.js");
    const bindings = [...unified.nodes.values()]
      .filter((n) => n.type === "module-binding")
      .map((n) => n.node);
    assert.ok(bindings.length === 2, `fixture must produce 2 bindings, got ${bindings.length}`);
    return buildBindingFingerprintIndex(bindings);
  };
  const oldIndex = bindingSides('var helperA = require("./a");\nvar helperB = require("./b");');
  const newIndex = bindingSides('var helperA = require("./a");\nvar helperB = require("./b");');
  const close = findCloseMatches(
    [...oldIndex.fingerprints.keys()],
    [...newIndex.fingerprints.keys()],
    oldIndex,
    newIndex
  );
  assign.binding_ids_skipped = {
    pairs: [...close.closeMatches.entries()].map(([oldId, newId]) => ({
      oldId,
      newId,
      score: close.scores.get(oldId),
      scoreBits: bits(close.scores.get(oldId))
    })),
    skippedOld: close.skippedOld,
    skippedNew: close.skippedNew
  };
}

// ---------------------------------------------------------------------------
// align — computeBodyLocalTransfers over the statement-align suite fixtures.
// ---------------------------------------------------------------------------

function fnOf(code) {
  const functions = buildFunctionGraph(parse(code), "test.js");
  const outer = functions.find((f) => f.path.parentPath?.isProgram());
  if (!outer) throw new Error("no top-level function in fixture");
  return outer;
}

const align = {};

function alignCase(label, priorCode, nextCode) {
  const alignment = computeBodyLocalTransfers(fnOf(priorCode), fnOf(nextCode));
  align[label] = {
    transfers: alignment.transfers.map((p) => ({
      oldName: p.oldName,
      newName: p.newName
    })),
    hints: alignment.hints.map((h) => ({
      newName: h.newName,
      priorName: h.priorName,
      snapEligible: h.snapEligible
    })),
    alignedStatements: alignment.alignedStatements,
    totalNewStatements: alignment.totalNewStatements
  };
}

const priorDeep = `
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
    }`;
const nextDeep = `
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
      }`;
alignCase("deep_branches", priorDeep, nextDeep);

alignCase(
  "switch_cases",
  `
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
      }`,
  `
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
      }`
);

alignCase(
  "two_changed_containers",
  `
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
      }`,
  `
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
      }`
);

alignCase(
  "same_type_siblings",
  `
      function pick(cfg, io) {
        if (cfg.a) {
          let alphaBox = io.box(cfg.a);
          io.send(alphaBox, cfg.k1);
        }
        if (cfg.b) {
          let betaBox = io.box(cfg.b);
          io.send(betaBox, cfg.k2);
        }
      }`,
  `
      function pick(c, o) {
        if (c.a) {
          let x = o.box(c.a);
          o.sendFast(x, c.k1);
        }
        if (c.b) {
          let y = o.box(c.b);
          o.sendFast(y, c.k2);
        }
      }`
);

alignCase(
  "use_site_hint",
  `
    function process(input) {
      let result = compute(input);
      log(result);
      return result;
    }`,
  `
    function process(a) {
      let b = compute(normalize(a));
      log(b);
      return b;
    }`
);

alignCase(
  "nested_binding",
  `
      function outer(input) {
        let total = seed(input);
        function helper(count) { return count + total; }
        return helper(total);
      }`,
  `
      function outer(a) {
        let total = seed(reshape(a));
        function helper(z) { return z + total; }
        return helper(total);
      }`
);

alignCase(
  "snap_eligible",
  `
      function handle(input) {
        let caughtError = decode(input);
        let scratch = decode(input);
        report(caughtError);
      }`,
  `
      function handle(a) {
        let x = decode(a);
        report(x);
      }`
);

alignCase(
  "bare_let_single",
  `function host(input) {
      let isDeferredMcpRequestPresent;
      isDeferredMcpRequestPresent = input.some(checkDeferred);
      return isDeferredMcpRequestPresent;
    }`,
  `function host(a) {
      let b;
      b = a.some(checkDeferred);
      return b;
    }`
);

alignCase(
  "bare_let_multi_declarator",
  `function host(input) {
      let firstFlag, secondFlag, thirdFlag;
      firstFlag = input.a();
      secondFlag = input.b();
      thirdFlag = input.c();
      return [firstFlag, secondFlag, thirdFlag];
    }`,
  `function host(q) {
      let m, inserted, n, o;
      m = q.a();
      inserted = q.d();
      n = q.b();
      o = q.c();
      return [m, n, o];
    }`
);

const out = {
  schemaVersion: 1,
  closeMatchTopK: CLOSE_MATCH_TOP_K,
  cosine,
  matrix,
  assign,
  align
};

process.stdout.write(`${JSON.stringify(out, null, 2)}\n`);
