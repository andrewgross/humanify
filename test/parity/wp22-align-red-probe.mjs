// TS ground truth for the collect_aligned_pairs rest-index bug (WP2.2 gate).
// Fixture shape: an IDENTICAL-HASH unit FIRST (paired by hash, removed from
// the rest), then a structurally-changed container whose INNER statements
// also cannot align. TS descends the changed container (object refs, no
// shift) and finds nothing → aligned stays 1. The Rust bug re-indexes the
// rest positions against the ORIGINAL vectors → descends the already-paired
// first unit instead → phantom +1.
// Swap the two statements: the shift vanishes (the changed container is
// last in the rest, but the FIRST original is the changed one... ) — either
// way TS must give the same aligned count for both orders.
import { buildFunctionGraph } from "../../src/analysis/function-graph.js";
import { computeBodyLocalTransfers } from "../../src/prior-version/statement-align.js";
import { parseSync } from "@babel/core";

function fnOf(code) {
  const ast = parseSync(code, { sourceType: "module" });
  const functions = buildFunctionGraph(ast, "test.js");
  const outer = functions.find((f) => f.path.parentPath?.isProgram());
  if (!outer) throw new Error("no top-level function");
  return outer;
}

const ORDER_A_PRIOR = `
    function f(input) {
      if (flag) { log("same"); }
      if (check(input, extra)) { return prep(input, more); }
    }`;
const ORDER_A_NEXT = `
    function f(a) {
      if (flag) { log("same"); }
      if (check(a)) { return prep(a, fewer); }
    }`;
const ORDER_B_PRIOR = `
    function f(input) {
      if (check(input, extra)) { return prep(input, more); }
      if (flag) { log("same"); }
    }`;
const ORDER_B_NEXT = `
    function f(a) {
      if (check(a)) { return prep(a, fewer); }
      if (flag) { log("same"); }
    }`;

for (const [label, p, n] of [
  ["orderA (log first)", ORDER_A_PRIOR, ORDER_A_NEXT],
  ["orderB (log last)", ORDER_B_PRIOR, ORDER_B_NEXT],
]) {
  const a = computeBodyLocalTransfers(fnOf(p), fnOf(n));
  console.log(
    `${label}: aligned=${a.alignedStatements} total=${a.totalNewStatements} transfers=${JSON.stringify(a.transfers)} hints=${JSON.stringify(a.hints.map((h) => [h.newName, h.priorName, h.snapEligible]))}`
  );
}
