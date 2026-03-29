import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import {
  collectPositionalReferences,
  computeGraphDensity,
  computeIdfWeights,
  computeSparsity,
  buildSimilarityGraph,
  detectBundleGaps,
  extractBundlerSignals,
  gapBasedClustering,
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

    // "common" is in all 3 -> lowest IDF
    // "a" is in 2 -> medium IDF
    // "b", "c", "d", "e" are in 1 -> highest IDF
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

    // fn1 and fn2 share "sharedHelper" -> connected
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

    // Single function -> no edges
    const edges = graph.get("fn1") ?? [];
    assert.strictEqual(edges.length, 0);
  });
});

describe("collectPositionalReferences", () => {
  it("returns position-keyed references to top-level bindings", () => {
    const ast = parse(`
      var state = {};
      function init() { return state; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const fn = functions.find(
      (f) =>
        f.path.node.type === "FunctionDeclaration" &&
        (f.path.node as t.FunctionDeclaration).id?.name === "init"
    );
    assert.ok(fn, "should find init function");

    const refs = collectPositionalReferences(fn, ast);
    assert.ok(refs.size > 0, "should have at least one reference");
    // All references should be position-keyed
    for (const ref of refs) {
      assert.ok(
        ref.startsWith("pos:"),
        `Reference should be position-keyed: ${ref}`
      );
    }
  });

  it("does not include self-references", () => {
    const ast = parse(`
      function self() { return self; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const fn = functions.find(
      (f) =>
        f.path.node.type === "FunctionDeclaration" &&
        (f.path.node as t.FunctionDeclaration).id?.name === "self"
    );
    assert.ok(fn, "should find self function");

    const refs = collectPositionalReferences(fn, ast);
    // self-reference should be excluded
    assert.strictEqual(refs.size, 0, "should not include self-reference");
  });

  it("excludes local variables and parameters", () => {
    const ast = parse(`
      var topLevel = 1;
      function foo(param) {
        var local = 2;
        return topLevel + local + param;
      }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const fn = functions.find(
      (f) =>
        f.path.node.type === "FunctionDeclaration" &&
        (f.path.node as t.FunctionDeclaration).id?.name === "foo"
    );
    assert.ok(fn, "should find foo function");

    const refs = collectPositionalReferences(fn, ast);
    // Should only reference topLevel, not local or param
    assert.strictEqual(refs.size, 1, "should reference only topLevel");
  });

  it("produces identical results regardless of identifier names", () => {
    // Simulates minification: same structure, different names
    const astA = parse(`
      var state = {};
      function init() { return state; }
    `);
    const astB = parse(`
      var e = {};
      function t() { return e; }
    `);

    const fnsA = buildFunctionGraph(astA, "test.js");
    const fnsB = buildFunctionGraph(astB, "test.js");

    const fnA = fnsA.find((f) => f.path.node.type === "FunctionDeclaration");
    const fnB = fnsB.find((f) => f.path.node.type === "FunctionDeclaration");
    assert.ok(fnA && fnB, "should find functions");

    const refsA = collectPositionalReferences(fnA, astA);
    const refsB = collectPositionalReferences(fnB, astB);

    // Both should have exactly one positional reference, and at the same position
    assert.strictEqual(refsA.size, 1);
    assert.strictEqual(refsB.size, 1);
    assert.deepStrictEqual(
      Array.from(refsA),
      Array.from(refsB),
      "Positional references should be identical regardless of names"
    );
  });
});

describe("computeGraphDensity", () => {
  it("returns 0 for an empty graph", () => {
    const graph = new Map<string, { target: string; weight: number }[]>();
    assert.strictEqual(computeGraphDensity(graph), 0);
  });

  it("returns 0 for a graph with no edges", () => {
    const graph = new Map<string, { target: string; weight: number }[]>([
      ["a", []],
      ["b", []],
      ["c", []]
    ]);
    assert.strictEqual(computeGraphDensity(graph), 0);
  });

  it("returns 1.0 for a fully connected pair", () => {
    const graph = new Map<string, { target: string; weight: number }[]>([
      ["a", [{ target: "b", weight: 1 }]],
      ["b", [{ target: "a", weight: 1 }]]
    ]);
    // 2 connected nodes, 2 total edges, density = 2 / (2 * 1) = 1.0
    assert.strictEqual(computeGraphDensity(graph), 1);
  });

  it("excludes isolated nodes from density calculation", () => {
    // Two connected nodes + one isolated: density should be based on the 2 connected
    const graph = new Map<string, { target: string; weight: number }[]>([
      ["a", [{ target: "b", weight: 1 }]],
      ["b", [{ target: "a", weight: 1 }]],
      ["c", []]
    ]);
    // connectedNodes=2, totalEdges=2, density = 2/(2*1) = 1.0
    assert.strictEqual(computeGraphDensity(graph), 1);
  });
});

describe("detectBundleGaps", () => {
  it("returns empty array for fewer than 2 functions", () => {
    const ast = parse(`function only() {}`);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const gaps = detectBundleGaps(topLevel);
    assert.strictEqual(gaps.length, 0);
  });

  it("returns N-1 gap scores for N functions", () => {
    const ast = parse(`
      function a() { return 1; }
      function b() { return 2; }
      function c() { return 3; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const gaps = detectBundleGaps(topLevel);
    assert.strictEqual(gaps.length, topLevel.length - 1);
  });

  it("produces scores between 0 and 1", () => {
    const ast = parse(`
      function a() { return 1; }
      function b() { return 2; }
      function c() { return 3; }
      function d() { return 4; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const gaps = detectBundleGaps(topLevel);
    for (const score of gaps) {
      assert.ok(score >= 0, `score should be >= 0, got ${score}`);
      assert.ok(score <= 1, `score should be <= 1, got ${score}`);
    }
  });
});

describe("gapBasedClustering", () => {
  it("puts all functions in one cluster when targetCount is 1", () => {
    const ast = parse(`
      function a() { return 1; }
      function b() { return 2; }
      function c() { return 3; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const communities = gapBasedClustering(topLevel, 1);
    const communitySet = new Set(communities.values());
    assert.strictEqual(communitySet.size, 1, "should be 1 community");
  });

  it("splits into targetCount communities", () => {
    const ast = parse(`
      function a() { return 1; }
      function b() { return 2; }
      function c() { return 3; }
      function d() { return 4; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const communities = gapBasedClustering(topLevel, 2);
    const communitySet = new Set(communities.values());
    assert.strictEqual(communitySet.size, 2, "should be 2 communities");
  });

  it("assigns every function to a community", () => {
    const ast = parse(`
      function a() { return 1; }
      function b() { return 2; }
      function c() { return 3; }
    `);
    const functions = buildFunctionGraph(ast, "test.js");
    const topLevel = functions.filter((fn) => !fn.scopeParent);

    const communities = gapBasedClustering(topLevel, 2);
    for (const fn of topLevel) {
      assert.ok(
        communities.has(fn.sessionId),
        `${fn.sessionId} should be assigned`
      );
    }
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

    // alpha and beta are in the same __export block -> should have a boost
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

  it("uses sqrt(N) for minified bundles", () => {
    // Minified: 100 functions on 5 lines
    const count = estimateFileCount(100, 5);
    assert.strictEqual(count, 10, "sqrt(100) = 10 for minified bundles");
  });
});

describe("referenceCluster target count", () => {
  it("respects target count with gap-based splitting", () => {
    // Create a bundle with groups of functions separated by gaps (simulating
    // multiple source files concatenated). Gap-based splitting should respect
    // the target count.
    const groups = [];
    for (let g = 0; g < 10; g++) {
      // Each group has 3 functions close together
      for (let i = 0; i < 3; i++) {
        const id = g * 3 + i;
        groups.push(`var v${id} = ${id};`);
        groups.push(`function fn${id}() { return v${id}; }`);
      }
      // Add a large gap between groups (simulating file boundary)
      if (g < 9) {
        groups.push("\n".repeat(20));
      }
    }
    const code = groups.join("\n");
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const parsedFiles = [{ ast, filePath: "test.js", source: code }];

    const result = referenceCluster(functions, parsedFiles, 10);
    const fileCount = new Set(result.values()).size;

    // Should produce close to 10 files
    assert.ok(
      fileCount >= 5 && fileCount <= 15,
      `Expected ~10 files, got ${fileCount}`
    );
  });
});
