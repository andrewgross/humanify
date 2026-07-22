/**
 * Statement-twin wholesale name inheritance (roadmap Lever 1).
 *
 * When a top-level statement's rename-invariant statementHash (the split's
 * literal-preserving, identifier-blind hash) uniquely matches a prior
 * statement (count 1 on both sides), every binding declared inside it can
 * inherit its prior name positionally through the placeholder-slot bridge —
 * deterministic, no LLM.
 *
 * The fixtures here are built to defeat the FUNCTION matcher but not the
 * statement tier: same-shaped lazy-init arrows whose fingerprints are
 * literal-blind collide (and an added third sibling makes the bucket
 * unequal-count, so the ordinal tier abstains and every arrow stays
 * PENDING), while the enclosing statements stay unique by their distinct
 * numeric literals — the one discriminator statementHash preserves but no
 * function-cascade feature (blind structural hash, string-only shingles)
 * can see.
 */
import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { generate } from "../babel-utils.js";
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import type { FunctionNode, UnifiedGraph } from "../analysis/types.js";
import { NULL_PROFILER } from "../profiling/profiler.js";
import { applyPriorVersionIfPresent } from "../rename/prior-transfer.js";
import { matchPriorVersion } from "./prior-version.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("parse failed");
  return ast;
}

function graphOf(code: string): {
  graph: UnifiedGraph;
  functions: Map<string, FunctionNode>;
  bindings: import("../analysis/types.js").ModuleBindingNode[];
  ast: t.File;
} {
  const ast = parse(code);
  const graph = buildUnifiedGraph(ast, "test.js");
  const functions = new Map<string, FunctionNode>();
  const bindings: import("../analysis/types.js").ModuleBindingNode[] = [];
  for (const [, node] of graph.nodes) {
    if (node.type === "function") functions.set(node.node.sessionId, node.node);
    else bindings.push(node.node);
  }
  return { graph, functions, bindings, ast };
}

/** Flatten statement-twin pairs into oldName → newName for assertions. */
function twinRenames(
  result: ReturnType<typeof matchPriorVersion>
): Map<string, string> {
  const map = new Map<string, string>();
  for (const pair of result.statementTwins.pairs) {
    map.set(pair.oldName, pair.newName);
  }
  return map;
}

/**
 * The three sibling arrows share one literal-blind fingerprint with
 * UNEQUAL counts (2 prior vs 3 fresh), reference nothing bound (no
 * identity evidence), and their heads are function-expression inits (the
 * binding cascade excludes those) — so every function tier abstains and
 * the arrows stay pending. The statements themselves stay unique via
 * their numeric literals — which the statement hash preserves but every
 * function-cascade feature (blind structural hash, string-only shingles)
 * ignores.
 */
const PRIOR_LAZY = `
var loadAlphaService = (alphaRetries) => { var alphaEndpoint = 111; return alphaEndpoint + alphaRetries; };
var loadBetaService = (betaRetries) => { var betaEndpoint = 222; return betaEndpoint + betaRetries; };
`;

const FRESH_LAZY = `
var x1 = (r1) => { var e1 = 111; return e1 + r1; };
var x2 = (r2) => { var e2 = 222; return e2 + r2; };
var x3 = (r3) => { var e3 = 333; return e3 + r3; };
`;

describe("statement-twin transfer computation", () => {
  it("bridges pending arrows' bindings through unique statement twins", () => {
    const fresh = graphOf(FRESH_LAZY);
    const result = matchPriorVersion(
      PRIOR_LAZY,
      fresh.functions,
      fresh.bindings,
      NULL_PROFILER,
      fresh.graph
    );

    const renames = twinRenames(result);
    assert.strictEqual(renames.get("r1"), "alphaRetries");
    assert.strictEqual(renames.get("e1"), "alphaEndpoint");
    assert.strictEqual(renames.get("r2"), "betaRetries");
    assert.strictEqual(renames.get("e2"), "betaEndpoint");
    // heads are fn-expression inits the binding cascade excludes — the
    // twin tier is what recovers them
    assert.strictEqual(renames.get("x1"), "loadAlphaService");
    assert.strictEqual(renames.get("x2"), "loadBetaService");
    // gamma statement has no prior twin — genuinely new code, untouched
    assert.strictEqual(renames.has("r3"), false);
    assert.strictEqual(renames.has("e3"), false);
    assert.strictEqual(renames.has("x3"), false);
    // every emitted pair carries its resolved fresh-side binding
    for (const pair of result.statementTwins.pairs) {
      assert.ok(pair.binding, `pair ${pair.oldName} must carry its binding`);
    }
  });

  it("abstains when the statement hash is not unique on both sides", () => {
    // Identical literals on two prior statements → 2:1 counts, no twin.
    const prior = `
var loadOne = (oneRetries) => { var oneEndpoint = 555; return oneEndpoint + oneRetries; };
var loadTwo = (twoRetries) => { var twoEndpoint = 555; return twoEndpoint + twoRetries; };
`;
    const freshCode = `
var x1 = (r1) => { var e1 = 555; return e1 + r1; };
`;
    const fresh = graphOf(freshCode);
    const result = matchPriorVersion(
      prior,
      fresh.functions,
      fresh.bindings,
      NULL_PROFILER,
      fresh.graph
    );
    assert.strictEqual(result.statementTwins.pairs.length, 0);
  });

  it("vetoes a same-shape twin whose exact-matched callee identity differs", () => {
    // execAlpha/execBeta have distinct bodies → both exact-match across
    // versions. The wrapped statements hash equal (callee names are masked,
    // same "task" literal) and are unique per side — but the prior one calls
    // execAlpha while the fresh one calls execBeta: a false twin the callee
    // gate must reject. The extra fresh-only sibling (distinct literal)
    // keeps the arrow bucket unequal-count so the arrows stay pending.
    const prior = `
function execAlpha(taskName) { return taskName + "a"; }
function execBeta(taskName) { return taskName + "b" + "b"; }
function wrap(runFn) { return { run: runFn }; }
var runTask = wrap((taskInput) => { var taskResult = execAlpha("task"); return taskResult + taskInput; });
`;
    const freshCode = `
function g1(t) { return t + "a"; }
function g2(t) { return t + "b" + "b"; }
function w(f) { return { run: f }; }
var rt = w((ti) => { var tr = g2("task"); return tr + ti; });
var extra = w((xi) => { var xr = g2("other"); return xr + xi; });
`;
    const fresh = graphOf(freshCode);
    const result = matchPriorVersion(
      prior,
      fresh.functions,
      fresh.bindings,
      NULL_PROFILER,
      fresh.graph
    );
    const renames = twinRenames(result);
    assert.strictEqual(
      renames.has("ti"),
      false,
      "different-callee twin must be vetoed"
    );
    assert.strictEqual(renames.has("tr"), false);
  });

  it("is a no-op on a quiet hop (names already match the prior)", () => {
    const fresh = graphOf(PRIOR_LAZY);
    const result = matchPriorVersion(
      PRIOR_LAZY,
      fresh.functions,
      fresh.bindings,
      NULL_PROFILER,
      fresh.graph
    );
    assert.strictEqual(result.statementTwins.pairs.length, 0);
  });

  it("abstains when property-name content differs (structural gate)", () => {
    // statementHash masks property names, so {onFoo:…} and {onBar:…} twin
    // at the coarse level — the finer placeholder-walk hash keeps property
    // names verbatim and must reject the bridge.
    const prior = `
function util(x) { return x + 1; }
var registerAlpha = reg({ onFoo: (alphaCb) => { var alphaVal = util(alphaCb); return alphaVal; } }, "alpha");
`;
    const freshCode = `
function u2(x) { return x + 9; }
var b1 = reg({ onBar: (cb1) => { var v1 = u2(cb1); return v1; } }, "alpha");
var b2 = reg({ onBar: (cb2) => { var v2 = u2(cb2); return v2; } }, "other");
`;
    const fresh = graphOf(freshCode);
    const result = matchPriorVersion(
      prior,
      fresh.functions,
      fresh.bindings,
      NULL_PROFILER,
      fresh.graph
    );
    const renames = twinRenames(result);
    assert.strictEqual(renames.has("cb1"), false);
    assert.strictEqual(renames.has("v1"), false);
  });

  it("computes nothing when no fresh graph is provided (legacy callers)", () => {
    const fresh = graphOf(FRESH_LAZY);
    const result = matchPriorVersion(
      PRIOR_LAZY,
      fresh.functions,
      fresh.bindings,
      NULL_PROFILER
    );
    assert.strictEqual(result.statementTwins.pairs.length, 0);
  });
});

describe("statement-twin application (applyPriorVersionIfPresent)", () => {
  it("applies twin renames to the AST before the LLM pass", () => {
    const fresh = graphOf(FRESH_LAZY);
    const allFunctions = [...fresh.functions.values()];
    applyPriorVersionIfPresent(
      PRIOR_LAZY,
      allFunctions,
      fresh.graph,
      NULL_PROFILER
    );
    const out = generate(fresh.ast).code;
    assert.match(out, /alphaRetries/);
    assert.match(out, /alphaEndpoint/);
    assert.match(out, /betaRetries/);
    assert.match(out, /betaEndpoint/);
    // heads come via the binding cascade — also applied by this entry point
    assert.match(out, /loadAlphaService/);
    assert.match(out, /loadBetaService/);
  });

  it("registers transferred names so the LLM pass will skip them", () => {
    const fresh = graphOf(FRESH_LAZY);
    const allFunctions = [...fresh.functions.values()];
    applyPriorVersionIfPresent(
      PRIOR_LAZY,
      allFunctions,
      fresh.graph,
      NULL_PROFILER
    );
    const registered = new Set<string>();
    for (const fn of allFunctions) {
      for (const name of fn.priorVersionTransferred ?? []) {
        registered.add(name);
      }
    }
    assert.ok(registered.has("alphaRetries"), "param registered with owner");
    assert.ok(registered.has("alphaEndpoint"), "local registered with owner");
  });
});

describe("cross-pair repair under statement reorder", () => {
  // The 85→86-style bundle shuffle: two same-shaped siblings swap source
  // order, so the equal-count ordinal tier exact-matches them CROSSED
  // (alpha↔beta). Their statements stay unique 1:1 by numeric literals —
  // the twin tier must outrank the cross-paired exact matches and restore
  // each statement's own prior names.
  const PRIOR_ORDERED = `
var loadAlphaService = (alphaRetries) => { var alphaEndpoint = 111; return alphaEndpoint + alphaRetries; };
var loadBetaService = (betaRetries) => { var betaEndpoint = 222; return betaEndpoint + betaRetries; };
`;
  const FRESH_SWAPPED = `
var x2 = (r2) => { var e2 = 222; return e2 + r2; };
var x1 = (r1) => { var e1 = 111; return e1 + r1; };
`;

  it("bridges twins whose contained functions were exact-matched across statements", () => {
    const fresh = graphOf(FRESH_SWAPPED);
    const result = matchPriorVersion(
      PRIOR_ORDERED,
      fresh.functions,
      fresh.bindings,
      NULL_PROFILER,
      fresh.graph
    );
    const renames = twinRenames(result);
    assert.strictEqual(renames.get("x1"), "loadAlphaService");
    assert.strictEqual(renames.get("r1"), "alphaRetries");
    assert.strictEqual(renames.get("e1"), "alphaEndpoint");
    assert.strictEqual(renames.get("x2"), "loadBetaService");
    assert.strictEqual(renames.get("r2"), "betaRetries");
    assert.strictEqual(renames.get("e2"), "betaEndpoint");
  });

  it("applies the repair to the AST (twin outranks the crossed exact match)", () => {
    const fresh = graphOf(FRESH_SWAPPED);
    applyPriorVersionIfPresent(
      PRIOR_ORDERED,
      [...fresh.functions.values()],
      fresh.graph,
      NULL_PROFILER
    );
    const out = generate(fresh.ast).code;
    // the 111-statement must carry the alpha names, the 222-statement beta —
    // a crossed exact transfer would swap them
    const alphaIdx = out.indexOf("alphaEndpoint = 111");
    const betaIdx = out.indexOf("betaEndpoint = 222");
    assert.ok(alphaIdx >= 0, `alpha names on the 111 statement:\n${out}`);
    assert.ok(betaIdx >= 0, `beta names on the 222 statement:\n${out}`);
  });
});

describe("bucket identity pairing (non-unique hashes)", () => {
  // The initializeApp-family pattern: same-shaped lazy statements whose
  // only distinguishing feature is WHICH matched helper they reference
  // (bound → masked by the statement hash → one collided bucket, and the
  // outer write-target is an unhashable `var x;` the alternation can't
  // map, so the arrows stay pending). Reference-identity keys pair the
  // bucket members; the outer write-target names arrive as votes.
  const prior = `
function libAlpha() { return 1 + 1; }
function libBeta() { return 2 * 3; }
var alphaCache;
var betaCache;
var initAlphaModule = (alphaReady) => { alphaCache = libAlpha(); return alphaReady; };
var initBetaModule = (betaReady) => { betaCache = libBeta(); return betaReady; };
var readAlphaTwice = () => alphaCache + alphaCache + 7777;
var readBetaTwice = () => betaCache + betaCache + 8888;
`;
  const freshCode = `
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
`;

  it("pairs bucket members by matched-reference identity and bridges them", () => {
    const fresh = graphOf(freshCode);
    const result = matchPriorVersion(
      prior,
      fresh.functions,
      fresh.bindings,
      NULL_PROFILER,
      fresh.graph
    );
    const renames = twinRenames(result);
    assert.strictEqual(renames.get("p1"), "alphaReady");
    assert.strictEqual(renames.get("p2"), "betaReady");
    // heads of the paired statements bridge too
    assert.strictEqual(renames.get("k1"), "initAlphaModule");
    assert.strictEqual(renames.get("k2"), "initBetaModule");
    // the gamma sibling references an unmatched helper — no key, no pair
    assert.strictEqual(renames.has("p3"), false);
    assert.strictEqual(renames.has("k3"), false);
  });

  it("emits outer-reference votes that name the var-only cache roots", () => {
    const fresh = graphOf(freshCode);
    const allFunctions = [...fresh.functions.values()];
    applyPriorVersionIfPresent(prior, allFunctions, fresh.graph, NULL_PROFILER);
    const out = generate(fresh.ast).code;
    // c1 is written by initAlphaModule and read by readAlphaTwice — two
    // bridged statements' outer votes agree → propagation renames it.
    assert.match(out, /var alphaCache/);
    assert.match(out, /var betaCache/);
    // the unpaired gamma cache stays untouched
    assert.match(out, /var c3/);
  });
});
