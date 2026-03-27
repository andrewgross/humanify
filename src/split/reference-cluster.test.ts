import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import {
  computeIdfWeights,
  computeSparsity,
  buildSimilarityGraph,
  extractBundlerSignals,
  referenceCluster,
  estimateFileCount
} from "./reference-cluster.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse");
  return ast;
}

describe("computeIdfWeights", () => {
  it("assigns higher weight to rare names", () => {
    const refSets = new Map<string, Set<string>>([
      ["fn1", new Set(["a", "b", "common"])],
      ["fn2", new Set(["a", "c", "common"])],
      ["fn3", new Set(["d", "e", "common"])]
    ]);

    const weights = computeIdfWeights(refSets);

    // "common" is in all 3 → lowest IDF
    // "a" is in 2 → medium IDF
    // "b", "c", "d", "e" are in 1 → highest IDF
    const wCommon = weights.get("common") ?? 0;
    const wA = weights.get("a") ?? 0;
    const wB = weights.get("b") ?? 0;
    assert.ok(wCommon < wA);
    assert.ok(wA < wB);
  });

  it("returns empty map for empty input", () => {
    const weights = computeIdfWeights(new Map());
    assert.strictEqual(weights.size, 0);
  });
});

describe("computeSparsity", () => {
  it("returns 1.0 when no function has top-level callees", () => {
    // Two independent functions with no calls between them
    const ast = parse(`
      function foo() { return 1; }
      function bar() { return 2; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const sparsity = computeSparsity(topLevel);
    assert.strictEqual(sparsity, 1.0);
  });

  it("returns 0.0 when all functions have top-level callees", () => {
    const ast = parse(`
      function foo() { bar(); }
      function bar() { foo(); }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const sparsity = computeSparsity(topLevel);
    assert.strictEqual(sparsity, 0.0);
  });

  it("returns 0.5 when half the functions have no callees", () => {
    const ast = parse(`
      function foo() { bar(); }
      function bar() { return 1; }
      function baz() { return 2; }
      function qux() { baz(); }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const sparsity = computeSparsity(topLevel);
    // bar and baz have no top-level callees = 2/4 = 0.5
    assert.strictEqual(sparsity, 0.5);
  });
});

describe("buildSimilarityGraph", () => {
  it("connects functions that share discriminative names", () => {
    const refSets = new Map<string, Set<string>>([
      ["fn1", new Set(["sharedHelper", "utilA"])],
      ["fn2", new Set(["sharedHelper", "utilB"])],
      ["fn3", new Set(["unrelated"])]
    ]);

    const idf = computeIdfWeights(refSets);
    const graph = buildSimilarityGraph(refSets, idf);

    // fn1 and fn2 share "sharedHelper" → connected
    assert.ok(graph.has("fn1"));
    const fn1Edges = graph.get("fn1") ?? [];
    const fn2Edge = fn1Edges.find((e) => e.target === "fn2");
    assert.ok(fn2Edge, "fn1 should be connected to fn2");
    assert.ok(fn2Edge.weight > 0);

    // fn3 shares nothing with fn1 or fn2
    const fn3Edge = fn1Edges.find((e) => e.target === "fn3");
    assert.ok(!fn3Edge, "fn1 should not be connected to fn3");
  });

  it("returns empty graph for single function", () => {
    const refSets = new Map([["fn1", new Set(["a"])]]);
    const idf = computeIdfWeights(refSets);
    const graph = buildSimilarityGraph(refSets, idf);

    // Single function → no edges
    const edges = graph.get("fn1") ?? [];
    assert.strictEqual(edges.length, 0);
  });
});

describe("referenceCluster", () => {
  it("groups functions sharing references into the same cluster", () => {
    // Simulate an esbuild-hoisted bundle: all top-level, no calls between them,
    // but functions from same "module" reference the same variables.
    const ast = parse(`
      var moduleA_state = {};
      function initA() { return moduleA_state; }
      function processA() { moduleA_state.x = 1; return moduleA_state; }
      function cleanupA() { moduleA_state.x = 0; }

      var moduleB_config = {};
      function initB() { return moduleB_config; }
      function processB() { moduleB_config.y = 2; return moduleB_config; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const parsedFiles = [{ ast, filePath: "test.js", source: "" }];

    const result = referenceCluster(functions, parsedFiles);

    // Functions referencing moduleA_state should cluster together
    const aFunctions = functions.filter(
      (fn) =>
        fn.path.node.type === "FunctionDeclaration" &&
        (fn.path.node as t.FunctionDeclaration).id?.name?.endsWith("A")
    );
    const bFunctions = functions.filter(
      (fn) =>
        fn.path.node.type === "FunctionDeclaration" &&
        (fn.path.node as t.FunctionDeclaration).id?.name?.endsWith("B")
    );

    if (aFunctions.length >= 2 && bFunctions.length >= 1) {
      const aFile = result.get(aFunctions[0].sessionId);
      for (const fn of aFunctions) {
        assert.strictEqual(
          result.get(fn.sessionId),
          aFile,
          `All A functions should be in same file, but ${(fn.path.node as t.FunctionDeclaration).id?.name} differs`
        );
      }

      // B functions should be in a different cluster from A
      const bFile = result.get(bFunctions[0].sessionId);
      assert.notStrictEqual(
        aFile,
        bFile,
        "Module A and B functions should be in different files"
      );
    }
  });

  it("assigns all functions to output files", () => {
    const ast = parse(`
      function alpha() { return 1; }
      function beta() { return 2; }
      function gamma() { return 3; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);
    const parsedFiles = [{ ast, filePath: "test.js", source: "" }];

    const result = referenceCluster(functions, parsedFiles);

    // Every top-level function should have an assignment
    for (const fn of topLevel) {
      assert.ok(
        result.has(fn.sessionId),
        `Function ${fn.sessionId} should be assigned`
      );
    }
  });

  it("returns deterministic results", () => {
    const code = `
      var shared = {};
      function x1() { shared.a = 1; }
      function x2() { shared.b = 2; }
      function y1() { return "unrelated"; }
    `;
    const ast1 = parse(code);
    const ast2 = parse(code);
    const fns1 = buildFunctionGraph(ast1, "test.js");
    const fns2 = buildFunctionGraph(ast2, "test.js");
    const pf1 = [{ ast: ast1, filePath: "test.js", source: "" }];
    const pf2 = [{ ast: ast2, filePath: "test.js", source: "" }];

    const result1 = referenceCluster(fns1, pf1);
    const result2 = referenceCluster(fns2, pf2);

    // Same output file names in same order
    const files1 = Array.from(result1.values()).sort();
    const files2 = Array.from(result2.values()).sort();
    assert.deepStrictEqual(files1, files2);
  });
});

describe("extractBundlerSignals", () => {
  it("detects __export blocks and creates pairwise boosts", () => {
    const ast = parse(`
      function alpha() { return 1; }
      function beta() { return 2; }
      function gamma() { return 3; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const source = `
      var mod_exports = {};
      __export(mod_exports, { alpha: () => alpha, beta: () => beta });
    `;
    const signals = extractBundlerSignals(source, topLevel);

    // alpha and beta are in the same __export block → should have a boost
    assert.ok(signals.pairBoosts.size > 0, "Should have pairwise boosts");
  });

  it("returns empty boosts when no bundler patterns found", () => {
    const ast = parse(`
      function foo() { return 1; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const signals = extractBundlerSignals("plain code", topLevel);
    assert.strictEqual(signals.pairBoosts.size, 0);
  });
});

describe("estimateFileCount", () => {
  it("returns at least 2 for any non-trivial input", () => {
    const count = estimateFileCount(50, 2000);
    assert.ok(count >= 2, `Expected >= 2, got ${count}`);
  });

  it("returns 1 for very few functions", () => {
    const count = estimateFileCount(1, 50);
    assert.strictEqual(count, 1);
  });

  it("scales with function count", () => {
    const small = estimateFileCount(20, 1000);
    const large = estimateFileCount(200, 10000);
    assert.ok(
      large > small,
      `More functions should yield more files: ${large} vs ${small}`
    );
  });
});
