import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFingerprintIndex, matchFunctions } from "./fingerprint-index.js";
import { buildFunctionGraph } from "./function-graph.js";
import { propagate } from "./propagation.js";
import type { FingerprintIndex } from "./types.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse code");
  return ast;
}

function buildIndex(code: string): FingerprintIndex {
  const ast = parse(code);
  const functions = buildFunctionGraph(ast, "test.js");
  const map = new Map(functions.map((f) => [f.sessionId, f]));
  return buildFingerprintIndex(map);
}

describe("propagation", () => {
  describe("matched-callee constraint", () => {
    it("resolves ambiguous function when one candidate calls a matched callee", () => {
      // Two structurally identical wrappers calling different (unique) callees.
      // Standard matching: wrappers are ambiguous, callees are matched.
      // Propagation: resolve wrappers by checking which candidate calls the matched callee.
      const codeV1 = `
        function wrapper1() { return uniqueA(); }
        function wrapper2() { return uniqueB(); }
        function uniqueA() { return "hello"; }
        function uniqueB(x) { return x + 1; }
      `;
      const codeV2 = `
        function w1() { return uA(); }
        function w2() { return uB(); }
        function uA() { return "hello"; }
        function uB(x) { return x + 1; }
      `;

      const oldIndex = buildIndex(codeV1);
      const newIndex = buildIndex(codeV2);

      // Use maxCascadeDepth: 0 (hash-only) so the cascade can't resolve via calleeShapes
      const result = matchFunctions(oldIndex, newIndex, { maxCascadeDepth: 0 });

      // The unique callees match (unique hash). Wrappers should be ambiguous.
      assert.ok(
        result.ambiguous.size > 0,
        "Should have ambiguous functions before propagation"
      );

      const { resolved, iterations } = propagate(
        result.matches,
        result.ambiguous,
        oldIndex,
        newIndex
      );

      assert.ok(
        resolved > 0,
        "Propagation should resolve some ambiguous functions"
      );
      assert.strictEqual(
        result.ambiguous.size,
        0,
        "All ambiguous should be resolved"
      );
      assert.ok(iterations >= 1, "Should take at least 1 iteration");
    });
  });

  describe("matched-caller constraint", () => {
    it("resolves ambiguous function when its caller is matched", () => {
      // Two identical leaf functions called by different (unique) callers.
      // At resolution 0: callers are unique (matched), leaves are ambiguous.
      // Propagation: check which candidate leaf is called by the matched caller.
      const codeV1 = `
        function complexCaller(x) {
          for (let i = 0; i < x; i++) {
            if (i > 5) leaf1();
          }
        }
        function simpleCaller() { leaf2(); }
        function leaf1() { return 1; }
        function leaf2() { return 1; }
      `;
      const codeV2 = `
        function cc(x) {
          for (let i = 0; i < x; i++) {
            if (i > 5) l1();
          }
        }
        function sc() { l2(); }
        function l1() { return 1; }
        function l2() { return 1; }
      `;

      const oldIndex = buildIndex(codeV1);
      const newIndex = buildIndex(codeV2);

      // Use maxCascadeDepth: 0 (hash-only) to ensure callerShapes cascade doesn't resolve them first
      const result = matchFunctions(oldIndex, newIndex, { maxCascadeDepth: 0 });

      const ambiguousBefore = result.ambiguous.size;
      assert.ok(
        ambiguousBefore > 0,
        "Should have ambiguous at hash-only matching"
      );

      const { resolved } = propagate(
        result.matches,
        result.ambiguous,
        oldIndex,
        newIndex
      );

      assert.ok(
        resolved > 0,
        "Propagation should resolve via caller constraint"
      );
    });
  });

  describe("multi-iteration convergence", () => {
    it("resolves in multiple rounds when first round unlocks further matches", () => {
      // Chain: uniqueRoot → wrapper → leaf
      // uniqueRoot is matched (unique hash). wrapper and leaf are ambiguous.
      // Round 1: propagation resolves wrapper (its caller uniqueRoot is matched).
      // Round 2: propagation resolves leaf (its caller wrapper is now matched).
      const codeV1 = `
        function uniqueRoot(x) {
          for (let i = 0; i < x; i++) {
            if (i > 5) wrapper1();
          }
        }
        function uniqueRoot2() { wrapper2(); }
        function wrapper1() { return leaf1(); }
        function wrapper2() { return leaf2(); }
        function leaf1() { return 1; }
        function leaf2() { return 1; }
      `;
      const codeV2 = `
        function uR(x) {
          for (let i = 0; i < x; i++) {
            if (i > 5) w1();
          }
        }
        function uR2() { w2(); }
        function w1() { return l1(); }
        function w2() { return l2(); }
        function l1() { return 1; }
        function l2() { return 1; }
      `;

      const oldIndex = buildIndex(codeV1);
      const newIndex = buildIndex(codeV2);

      // Use maxCascadeDepth: 0 (hash-only) so cascade can't help
      const result = matchFunctions(oldIndex, newIndex, { maxCascadeDepth: 0 });

      // Roots should match (unique hashes), rest ambiguous
      assert.ok(
        result.ambiguous.size >= 2,
        "Should have ambiguous wrappers and leaves"
      );

      const { resolved, iterations } = propagate(
        result.matches,
        result.ambiguous,
        oldIndex,
        newIndex
      );

      assert.ok(resolved >= 2, "Should resolve wrappers and leaves");
      assert.ok(iterations >= 2, "Should take multiple iterations");
      assert.strictEqual(result.ambiguous.size, 0, "All should be resolved");
    });
  });

  describe("scope-parent constraint", () => {
    it("resolves ambiguous nested function when scope parent is matched", () => {
      // Two identical nested functions inside different (unique) parents.
      const codeV1 = `
        function outerA(x) {
          for (let i = 0; i < x; i++) { if (i > 5) console.log(i); }
          function inner() { return 1; }
          return inner;
        }
        function outerB() {
          function inner() { return 1; }
          return inner;
        }
      `;
      const codeV2 = `
        function oA(x) {
          for (let i = 0; i < x; i++) { if (i > 5) console.log(i); }
          function inn() { return 1; }
          return inn;
        }
        function oB() {
          function inn() { return 1; }
          return inn;
        }
      `;

      const oldIndex = buildIndex(codeV1);
      const newIndex = buildIndex(codeV2);

      const result = matchFunctions(oldIndex, newIndex);

      // The outer functions have different structures so should match uniquely.
      // The inner functions are identical so may be ambiguous.
      if (result.ambiguous.size > 0) {
        const { resolved } = propagate(
          result.matches,
          result.ambiguous,
          oldIndex,
          newIndex
        );
        assert.ok(
          resolved > 0,
          "Scope parent constraint should resolve nested functions"
        );
      }
    });
  });

  describe("convergence and safety", () => {
    it("converges to fixed point (no infinite loop)", () => {
      // Everything is ambiguous and nothing can be resolved
      const code = `
        function a() { return 1; }
        function b() { return 1; }
      `;

      const oldIndex = buildIndex(code);
      const newIndex = buildIndex(code);

      const result = matchFunctions(oldIndex, newIndex);

      const { resolved, iterations } = propagate(
        result.matches,
        result.ambiguous,
        oldIndex,
        newIndex
      );

      assert.strictEqual(
        resolved,
        0,
        "Nothing should resolve when fully ambiguous"
      );
      assert.strictEqual(
        iterations,
        1,
        "Should stop after one fruitless iteration"
      );
    });

    it("does nothing when there are no ambiguous functions", () => {
      const code = `
        function a() { return "hello"; }
        function b(x) { return x + 1; }
      `;

      const oldIndex = buildIndex(code);
      const newIndex = buildIndex(code);

      const result = matchFunctions(oldIndex, newIndex);

      assert.strictEqual(
        result.ambiguous.size,
        0,
        "Precondition: no ambiguous"
      );

      const { resolved, iterations } = propagate(
        result.matches,
        result.ambiguous,
        oldIndex,
        newIndex
      );

      assert.strictEqual(resolved, 0);
      assert.strictEqual(iterations, 0, "Should skip when nothing ambiguous");
    });

    it("respects maxIterations option", () => {
      const codeV1 = `
        function wrapper1() { return uniqueA(); }
        function wrapper2() { return uniqueB(); }
        function uniqueA() { return "hello"; }
        function uniqueB(x) { return x + 1; }
      `;
      const codeV2 = `
        function w1() { return uA(); }
        function w2() { return uB(); }
        function uA() { return "hello"; }
        function uB(x) { return x + 1; }
      `;

      const oldIndex = buildIndex(codeV1);
      const newIndex = buildIndex(codeV2);

      const result = matchFunctions(oldIndex, newIndex);

      const { iterations } = propagate(
        result.matches,
        result.ambiguous,
        oldIndex,
        newIndex,
        { maxIterations: 1 }
      );

      assert.ok(iterations <= 1, "Should respect maxIterations");
    });

    it("never produces incorrect matches (precision check)", () => {
      // Verify propagation doesn't match wrong functions.
      const codeV1 = `
        function wrapper1() { return uniqueA(); }
        function wrapper2() { return uniqueB(); }
        function uniqueA() { return "hello"; }
        function uniqueB(x) { return x + 1; }
      `;
      const codeV2 = `
        function w1() { return uA(); }
        function w2() { return uB(); }
        function uA() { return "hello"; }
        function uB(x) { return x + 1; }
      `;

      const oldIndex = buildIndex(codeV1);
      const newIndex = buildIndex(codeV2);

      const result = matchFunctions(oldIndex, newIndex);

      propagate(result.matches, result.ambiguous, oldIndex, newIndex);

      assertCallGraphConsistency(result.matches, oldIndex, newIndex);
    });
  });
});

/**
 * Verifies that matched pairs are consistent with call-graph topology:
 * if old function calls callee X (matched to X'), the matched new function must call X'.
 */
function assertCallGraphConsistency(
  matches: Map<string, string>,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): void {
  for (const [oldId, newId] of matches) {
    const oldFn = oldIndex.functions?.get(oldId);
    const newFn = newIndex.functions?.get(newId);
    if (!oldFn || !newFn) continue;

    const newCalleeIds = new Set(
      [...newFn.internalCallees].map((c) => c.sessionId)
    );
    for (const oldCallee of oldFn.internalCallees) {
      const matchedCalleeNewId = matches.get(oldCallee.sessionId);
      if (!matchedCalleeNewId) continue;

      assert.ok(
        newCalleeIds.has(matchedCalleeNewId),
        `Matched ${oldId}->${newId} but callee mapping inconsistent: ` +
          `old callee ${oldCallee.sessionId} matched to ${matchedCalleeNewId} ` +
          `which is not called by ${newId}`
      );
    }
  }
}
