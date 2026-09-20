// probe: WP2.1 propagation ground truth — run the TS matchFunctions
// (hash-only cascade) + propagate over a small synthetic version pair and
// print the precondition (the matches/ambiguous the propagation starts
// from) plus the post-propagation state the Rust port must reproduce.
//
// Frozen at test/parity/wp21-propagation-frozen.json; the Rust test
// (propagation_test.rs) rebuilds both graphs from the embedded texts,
// feeds the frozen precondition into crates/humanify-core's propagate and
// compares the outcome AND the mutated maps.
//
// Session ids are `input.js:LINE:COL` on BOTH sides (the Rust graph build
// uses the same convention), so the frozen pairs are compared as strings.
import { parseSync } from "@babel/core";

const OLD = `
function wrapper1() { return uniqueA(); }
function wrapper2() { return uniqueB(); }
function uniqueA() { return "hello"; }
function uniqueB(x) { return x + 1; }
function complexCaller(x) {
  for (let i = 0; i < x; i++) {
    if (i > 5) leaf1();
  }
}
function simpleCaller() { leaf2(); }
function leaf1() { return 1; }
function leaf2() { return 1; }
function parent(x) {
  for (let i = 0; i < x; i++) { if (i > 5) console.log(i); }
  function child1() { return 1; }
  function child2() { return 1; }
  function child3() { return 1; }
  return [child1, child2, child3];
}
`;

const NEW = `
function w1() { return uA(); }
function w2() { return uB(); }
function uA() { return "hello"; }
function uB(x) { return x + 1; }
function cc(x) {
  for (let i = 0; i < x; i++) {
    if (i > 5) l1();
  }
}
function sc() { l2(); }
function l1() { return 1; }
function l2() { return 1; }
function p(x) {
  for (let i = 0; i < x; i++) { if (i > 5) console.log(i); }
  function c1() { return 1; }
  function c2() { return 1; }
  function c3() { return 1; }
  return [c1, c2, c3];
}
`;

const { buildFunctionGraph } = await import(
  "../../src/analysis/function-graph.js"
);
const { buildFingerprintIndex, matchFunctions } = await import(
  "../../src/analysis/fingerprint-index.js"
);
const { propagate } = await import("../../src/analysis/propagation.js");

function buildIndex(code) {
  const ast = parseSync(code, { sourceType: "module" });
  const functions = buildFunctionGraph(ast, "input.js");
  const map = new Map(functions.map((f) => [f.sessionId, f]));
  return buildFingerprintIndex(map);
}

const oldIndex = buildIndex(OLD);
const newIndex = buildIndex(NEW);

// Hash-only cascade: the same precondition class the Rust unit tests build
// by hand (unique hashes match, same-hash groups park in ambiguous).
const result = matchFunctions(oldIndex, newIndex, { maxCascadeDepth: 0 });

const precondition = {
  matches: [...result.matches.entries()],
  ambiguous: [...result.ambiguous.entries()]
};

const { resolved, iterations, byRung } = propagate(
  result.matches,
  result.ambiguous,
  oldIndex,
  newIndex
);

const post = {
  resolved,
  iterations,
  byRung,
  matches: [...result.matches.entries()],
  ambiguous: [...result.ambiguous.entries()]
};

console.log(
  JSON.stringify({ old: OLD, new: NEW, precondition, post }, null, 2)
);
