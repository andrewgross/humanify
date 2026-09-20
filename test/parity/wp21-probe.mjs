// probe: WP2.1 ground truth — run the TS buildFingerprintIndex /
// buildBindingFingerprintIndex over a small synthetic bundle and print the
// full fingerprints + hash buckets the Rust port must reproduce.
//
// The structuralHash BYTES are oxc-vs-babel serializer artifacts (the WP1.4
// gate established they differ by design); what must match is the
// equivalence classes (bucket partitions) and every RELATIONAL field:
// memberKey, features, callee/caller shapes, two-hop shapes, and which
// sessionIds share a bucket.
import { readFileSync } from "node:fs";
import { parseSync } from "@babel/core";

// The wrapper detection needs >= 50 bindings in the wrapper scope
// (WRAPPER_IIFE_BINDING_THRESHOLD) — the filler vars both trigger it and
// populate the module-binding index.
const FILLER = Array.from(
  { length: 55 },
  (_, i) => `var z${i} = ${i};`
).join("\n");

const SYNTHETIC = `(function(){
${FILLER}
var d1 = "s";
var d2 = "s";
var dep = d1;
var fnRef = leaf;
var uq;
function twin1() { return 9; }
function twin2() { return 9; }
function caller() {
  leaf();
  loopy(3);
}
function leaf() { return console.log("hi", 42); }
function loopy(n) {
  for (let i = 0; i < n; i++) {
    if (i > 2) return i;
  }
  try { leaf(); } catch (e) { other(); }
  return 0;
}
function other() { return "abc".length; }
var api = {
  getCount: function () { return leaf(); },
  loopy2: loopy,
  arrowVal: (x) => x + 1,
};
class Widget {
  render() { return leaf(); }
  compute(a, b) { return a + b; }
}
var throughVar = () => 2;
var wrapper = { viaVar: throughVar };
var assigned;
assigned = () => other();
api.commit = assigned;
if (typeof fetch !== "undefined") { fetch("http://x"); }
var localFetch = 1;
localFetch(leaf);
caller();
})();`;

const code = process.argv[2]
  ? readFileSync(process.argv[2], "utf8")
  : SYNTHETIC;
const filePath = process.argv[3] ?? "input.js";

const { buildFunctionGraph, buildUnifiedGraph } = await import(
  "../../src/analysis/function-graph.js"
);
const {
  serializeCalleeShape,
  computeShingleSet,
  jaccardSimilarity,
  SHINGLE_SIMILARITY_FLOOR,
} = await import("../../src/analysis/function-fingerprint.js");
const { buildFingerprintIndex, buildBindingFingerprintIndex } = await import(
  "../../src/analysis/fingerprint-index.js"
);

const ast = parseSync(code, { sourceType: "module" });

// --- function index --------------------------------------------------------
const functions = buildFunctionGraph(ast, filePath);
const fnMap = new Map(functions.map((f) => [f.sessionId, f]));
const index = buildFingerprintIndex(fnMap);

const fnRows = functions.map((fn) => {
  const fp = index.fingerprints.get(fn.sessionId);
  return {
    sessionId: fn.sessionId,
    structuralHash: fp.structuralHash,
    memberKey: fp.memberKey ?? null,
    features: fp.features,
    calleeShapes: (fp.calleeShapes ?? []).map(serializeCalleeShape),
    callerShapes: (fp.callerShapes ?? []).map(serializeCalleeShape),
    calleeHashes: fp.calleeHashes ?? [],
    twoHopShapes: fp.twoHopShapes ?? [],
    shingleCount: computeShingleSet(fn).size,
    internalCallees: [...fn.internalCallees].map((c) => c.sessionId),
    externalCallees: [...fn.externalCallees].sort(),
  };
});
const fnBuckets = [...index.byStructuralHash.entries()].map(([h, ids]) => ({
  hashKey: h,
  members: ids,
}));

// --- binding index ---------------------------------------------------------
const unified = buildUnifiedGraph(ast, filePath);
const bindings = [...unified.nodes.values()]
  .filter((n) => n.type === "module-binding")
  .map((n) => n.node);
const bindingIndex = buildBindingFingerprintIndex(bindings);

const bindingRows = bindings.map((b) => {
  const fp = bindingIndex.fingerprints.get(b.sessionId);
  return {
    sessionId: b.sessionId,
    name: b.name,
    hasFingerprint: Boolean(b.fingerprint),
    indexed: Boolean(fp),
    structuralHash: fp ? fp.structuralHash : null,
    calleeShapes: fp ? (fp.calleeShapes ?? []).map(serializeCalleeShape) : [],
    callerShapes: fp ? (fp.callerShapes ?? []).map(serializeCalleeShape) : [],
    calleeHashes: fp ? fp.calleeHashes ?? [] : [],
    twoHopShapes: fp ? fp.twoHopShapes ?? [] : [],
    internalCallees: [...b.internalCallees].map((c) => c.sessionId),
    callers: [...b.callers].map((c) => c.sessionId),
  };
});
const bindingBuckets = [...bindingIndex.byStructuralHash.entries()].map(
  ([h, ids]) => ({ hashKey: h, members: ids })
);

// --- the sort-order question ----------------------------------------------
// serializeCalleeShape is sorted with localeCompare in the TS; the output
// alphabet is ASCII (digits, comma, lowercase, true/false). Whether a Rust
// byte sort agrees is checked over EVERY shape combination.
const cfgTypes = ["linear", "branching", "looping", "complex"];
const shapes = [];
for (let arity = 0; arity <= 16; arity++)
  for (let complexity = 0; complexity <= 40; complexity++)
    for (const cfgType of cfgTypes)
      for (const hasExternalCalls of [false, true])
        shapes.push(
          serializeCalleeShape({
            arity,
            complexity,
            cfgType,
            hasExternalCalls,
          })
        );
const byLocale = [...shapes].sort((a, b) => a.localeCompare(b));
const byBytes = [...shapes].sort();
const sortsAgree =
  byLocale.length === byBytes.length &&
  byLocale.every((s, i) => s === byBytes[i]);

// Jaccard + the floor constant, for the record.
const sim = jaccardSimilarity(
  computeShingleSet(fnMap.get(fnRows[0].sessionId)),
  computeShingleSet(fnMap.get(fnRows[0].sessionId))
);

console.log(
  JSON.stringify(
    {
      shingleSimilarityFloor: SHINGLE_SIMILARITY_FLOOR,
      selfSimilarity: sim,
      sortsAgree,
      firstLocaleVsBytesDivergence: sortsAgree
        ? null
        : byLocale.findIndex((s, i) => s !== byBytes[i]),
      functions: fnRows,
      functionBuckets: fnBuckets,
      bindings: bindingRows,
      bindingBuckets: bindingBuckets,
    },
    null,
    1
  )
);
