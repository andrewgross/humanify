import { describe, it } from "node:test";
import assert from "node:assert";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import { clusterFunctions } from "./cluster.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse");
  return ast;
}

describe("clusterFunctions", () => {
  it("single function → single cluster", () => {
    const code = `function a() {}`;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    assert.strictEqual(result.clusters.length, 1);
    assert.strictEqual(result.clusters[0].members.size, 1);
    assert.strictEqual(result.shared.size, 0);
  });

  it("no functions → empty clusters", () => {
    const code = `const x = 1;`;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    assert.strictEqual(result.clusters.length, 0);
    assert.strictEqual(result.shared.size, 0);
  });

  it("root calls leaf → both in same cluster", () => {
    const code = `
      function root() { leaf(); }
      function leaf() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    assert.strictEqual(result.clusters.length, 1);
    assert.strictEqual(result.clusters[0].members.size, 2);
  });

  it("two independent roots → two clusters", () => {
    const code = `
      function rootA() { leafA(); }
      function leafA() {}
      function rootB() { leafB(); }
      function leafB() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    assert.strictEqual(result.clusters.length, 2);
    // Each cluster should have 2 members (root + leaf)
    const sizes = result.clusters.map(c => c.members.size).sort();
    assert.deepStrictEqual(sizes, [2, 2]);
  });

  it("shared callee (called by two roots) → shared", () => {
    const code = `
      function rootA() { shared(); }
      function rootB() { shared(); }
      function shared() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    assert.strictEqual(result.clusters.length, 2);
    assert.strictEqual(result.shared.size, 1, "shared function should be in shared set");
    // Each cluster should just have the root
    for (const cluster of result.clusters) {
      assert.strictEqual(cluster.members.size, 1);
    }
  });

  it("circular roots (A↔B) → merged", () => {
    const code = `
      function a() { b(); }
      function b() { a(); }
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    assert.strictEqual(result.clusters.length, 1, "Circular roots should be merged");
    assert.strictEqual(result.clusters[0].members.size, 2);
    assert.strictEqual(result.clusters[0].rootFunctions.length, 2);
  });

  it("chain: A→B→C → all in one cluster", () => {
    const code = `
      function a() { b(); }
      function b() { c(); }
      function c() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    assert.strictEqual(result.clusters.length, 1);
    assert.strictEqual(result.clusters[0].members.size, 3);
  });

  it("diamond: A→B, A→C, B→D, C→D → D exclusively owned by A", () => {
    const code = `
      function a() { b(); c(); }
      function b() { d(); }
      function c() { d(); }
      function d() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    // A is the only root. B, C, D all reachable only from A.
    assert.strictEqual(result.clusters.length, 1);
    assert.strictEqual(result.clusters[0].members.size, 4);
    assert.strictEqual(result.shared.size, 0);
  });

  it("determinism: 100 runs produce identical output", () => {
    const code = `
      function rootA() { shared(); leafA(); }
      function rootB() { shared(); leafB(); }
      function shared() {}
      function leafA() {}
      function leafB() {}
    `;

    const results: string[] = [];
    for (let i = 0; i < 100; i++) {
      const ast = parse(code);
      const functions = buildFunctionGraph(ast, "test.js");
      const result = clusterFunctions(functions);

      // Serialize to a deterministic string for comparison
      const serialized = JSON.stringify({
        clusters: result.clusters.map(c => ({
          id: c.id,
          roots: c.rootFunctions.sort(),
          members: Array.from(c.members).sort(),
          hashes: c.memberHashes,
        })),
        shared: Array.from(result.shared).sort(),
      });
      results.push(serialized);
    }

    // All 100 runs should produce identical output
    for (let i = 1; i < results.length; i++) {
      assert.strictEqual(results[i], results[0], `Run ${i} differs from run 0`);
    }
  });

  it("cluster fingerprint = hash of sorted member hashes", () => {
    const code = `
      function a() { b(); }
      function b() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    assert.strictEqual(result.clusters.length, 1);
    const cluster = result.clusters[0];

    // ID should be 16 hex characters
    assert.ok(/^[0-9a-f]{16}$/.test(cluster.id), `Cluster ID should be 16 hex chars, got: ${cluster.id}`);

    // memberHashes should be sorted
    const sorted = [...cluster.memberHashes].sort();
    assert.deepStrictEqual(cluster.memberHashes, sorted, "memberHashes should be sorted");
  });

  it("only top-level functions are roots (nested functions stay with parent)", () => {
    const code = `
      function outer() {
        function inner() {}
        inner();
      }
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    // outer is the root, inner is nested (has scopeParent)
    assert.strictEqual(result.clusters.length, 1);
    assert.strictEqual(result.clusters[0].rootFunctions.length, 1);
    // Only the top-level function should be in the cluster
    assert.strictEqual(result.clusters[0].members.size, 1);
  });

  it("two roots sharing a callee that shares a deeper callee → deeper callee also shared", () => {
    const code = `
      function rootA() { mid(); }
      function rootB() { mid(); }
      function mid() { deep(); }
      function deep() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    assert.strictEqual(result.clusters.length, 2);
    // mid is called by both roots → shared
    // deep is called only by mid, but mid is shared → deep is also reachable from both roots → shared
    assert.strictEqual(result.shared.size, 2, "Both mid and deep should be shared");
  });
});
