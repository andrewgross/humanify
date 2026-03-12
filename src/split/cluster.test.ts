import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
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
    const sizes = result.clusters.map((c) => c.members.size).sort();
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
    assert.strictEqual(
      result.shared.size,
      1,
      "shared function should be in shared set"
    );
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

    assert.strictEqual(
      result.clusters.length,
      1,
      "Circular roots should be merged"
    );
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
        clusters: result.clusters.map((c) => ({
          id: c.id,
          roots: c.rootFunctions.sort(),
          members: Array.from(c.members).sort(),
          hashes: c.memberHashes
        })),
        shared: Array.from(result.shared).sort()
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
    assert.ok(
      /^[0-9a-f]{16}$/.test(cluster.id),
      `Cluster ID should be 16 hex chars, got: ${cluster.id}`
    );

    // memberHashes should be sorted
    const sorted = [...cluster.memberHashes].sort();
    assert.deepStrictEqual(
      cluster.memberHashes,
      sorted,
      "memberHashes should be sorted"
    );
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
    assert.strictEqual(
      result.shared.size,
      2,
      "Both mid and deep should be shared"
    );
  });
});

describe("clusterFunctions with merging", () => {
  it("merges singleton cluster into most-connected neighbor", () => {
    // rootA calls helper, rootB calls helper
    // Without merging: rootA(1), rootB(1), helper→shared
    // With minClusterSize=2: rootA should merge into rootB or vice versa (whichever has more edges)
    const code = `
      function rootA() { helper(); leafA(); }
      function leafA() {}
      function rootB() { helper(); }
      function helper() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions, { minClusterSize: 2 });

    // rootB (1 member) should merge into rootA (2 members) since rootB→helper→rootA
    // After merge + reabsorb, helper should be absorbed since all callers now in one cluster
    assert.ok(
      result.clusters.length < 3,
      `Should have fewer clusters after merging, got ${result.clusters.length}`
    );
  });

  it("reabsorbs shared function when callers merge into one cluster", () => {
    // Three roots each calling shared. After merging the small ones together,
    // shared should be reabsorbed.
    const code = `
      function rootA() { shared(); leafA(); }
      function rootB() { shared(); }
      function rootC() { shared(); }
      function shared() {}
      function leafA() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    // Without merging
    const baseline = clusterFunctions(functions);
    assert.strictEqual(
      baseline.shared.size,
      1,
      "shared should be in shared set"
    );
    assert.strictEqual(baseline.clusters.length, 3);

    // With merging (min size 2)
    const merged = clusterFunctions(functions, { minClusterSize: 2 });
    // rootB and rootC (1 member each) should merge into rootA's cluster
    // Then shared should be reabsorbed
    assert.ok(
      merged.shared.size < baseline.shared.size,
      `Shared should decrease after merging: ${merged.shared.size} vs ${baseline.shared.size}`
    );
  });

  it("does not merge clusters already above minSize", () => {
    const code = `
      function rootA() { leafA1(); leafA2(); leafA3(); }
      function leafA1() {}
      function leafA2() {}
      function leafA3() {}
      function rootB() { leafB1(); leafB2(); leafB3(); }
      function leafB1() {}
      function leafB2() {}
      function leafB3() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions, { minClusterSize: 3 });

    // Both clusters have 4 members (root + 3 leaves) — no merging needed
    assert.strictEqual(result.clusters.length, 2);
  });

  it("merging preserves determinism", () => {
    const code = `
      function rootA() { shared(); leafA(); }
      function rootB() { shared(); leafB(); }
      function rootC() { shared(); }
      function shared() {}
      function leafA() {}
      function leafB() {}
    `;

    const results: string[] = [];
    for (let i = 0; i < 50; i++) {
      const ast = parse(code);
      const functions = buildFunctionGraph(ast, "test.js");
      const result = clusterFunctions(functions, { minClusterSize: 2 });
      const serialized = JSON.stringify({
        clusters: result.clusters.map((c) => ({
          id: c.id,
          members: Array.from(c.members).sort()
        })),
        shared: Array.from(result.shared).sort()
      });
      results.push(serialized);
    }

    for (let i = 1; i < results.length; i++) {
      assert.strictEqual(results[i], results[0], `Run ${i} differs from run 0`);
    }
  });

  it("handles isolated singletons with no edges (no merge target)", () => {
    const code = `
      function rootA() { leafA(); }
      function leafA() {}
      function isolated() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions, { minClusterSize: 2 });

    // isolated has no edges to any other cluster, so it stays as-is
    assert.ok(result.clusters.length >= 1);
  });

  it("many small roots calling same shared → all merge together", () => {
    const code = `
      function r1() { s(); }
      function r2() { s(); }
      function r3() { s(); }
      function r4() { s(); }
      function s() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions, { minClusterSize: 3 });

    // All small roots should merge together, then s gets reabsorbed
    // End result: 1 cluster with all 5 functions
    assert.strictEqual(
      result.clusters.length,
      1,
      "All should merge into one cluster"
    );
    assert.strictEqual(
      result.shared.size,
      0,
      "shared function should be reabsorbed"
    );
    assert.strictEqual(result.clusters[0].members.size, 5);
  });
});

describe("clusterFunctions with proximity fallback", () => {
  it("merges isolated singleton into nearest cluster by line proximity", () => {
    // rootA and leafA are connected (cluster 1)
    // rootB and leafB are connected (cluster 2)
    // isolated has no connections — should merge into nearest cluster
    const code = `
      function rootA() { leafA(); }
      function leafA() {}
      function isolated() {}
      function rootB() { leafB(); }
      function leafB() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    // Without proximity
    const base = clusterFunctions(functions);
    const isolatedCluster = base.clusters.find((c) => c.members.size === 1);
    assert.ok(isolatedCluster, "isolated should be its own cluster");

    // With proximity
    const result = clusterFunctions(functions, { proximityFallback: true });
    // isolated should be absorbed into one of the two clusters
    assert.strictEqual(
      result.clusters.length,
      2,
      "Should only have 2 clusters after proximity merge"
    );
    const sizes = result.clusters.map((c) => c.members.size).sort();
    assert.deepStrictEqual(
      sizes,
      [2, 3],
      "One cluster gets the isolated function"
    );
  });

  it("does not merge non-singletons by proximity", () => {
    // Two independent clusters, both with 2 members — neither should merge
    const code = `
      function rootA() { leafA(); }
      function leafA() {}
      function rootB() { leafB(); }
      function leafB() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions, { proximityFallback: true });

    assert.strictEqual(result.clusters.length, 2);
    const sizes = result.clusters.map((c) => c.members.size).sort();
    assert.deepStrictEqual(sizes, [2, 2]);
  });

  it("proximity + merge work together", () => {
    const code = `
      function rootA() { leafA(); }
      function leafA() {}
      function iso1() {}
      function iso2() {}
      function rootB() { leafB(); }
      function leafB() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions, {
      minClusterSize: 2,
      proximityFallback: true
    });

    // Both iso1 and iso2 should merge into nearest cluster
    assert.strictEqual(
      result.clusters.length,
      2,
      "Should only have 2 clusters"
    );
    // Total should be 6 functions across 2 clusters
    const total = result.clusters.reduce((s, c) => s + c.members.size, 0);
    assert.strictEqual(total, 6);
  });

  it("preserves determinism with proximity", () => {
    const code = `
      function rootA() { leafA(); }
      function leafA() {}
      function iso() {}
      function rootB() { leafB(); }
      function leafB() {}
    `;

    const results: string[] = [];
    for (let i = 0; i < 50; i++) {
      const ast = parse(code);
      const functions = buildFunctionGraph(ast, "test.js");
      const result = clusterFunctions(functions, { proximityFallback: true });
      const serialized = JSON.stringify({
        clusters: result.clusters.map((c) => ({
          id: c.id,
          members: Array.from(c.members).sort()
        }))
      });
      results.push(serialized);
    }

    for (let i = 1; i < results.length; i++) {
      assert.strictEqual(results[i], results[0], `Run ${i} differs from run 0`);
    }
  });
});
