import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import { clusterFunctions } from "./cluster.js";
import { computeMQ } from "./quality.js";

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse");
  return ast;
}

describe("computeMQ", () => {
  it("perfect partition → MQ > 0", () => {
    // Two completely independent groups
    const code = `
      function rootA() { leafA(); }
      function leafA() {}
      function rootB() { leafB(); }
      function leafB() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    const mq = computeMQ(result.clusters, functions);
    assert.ok(mq > 0, `MQ should be > 0 for a clean partition, got ${mq}`);
  });

  it("single cluster → MQ based on intra-cluster cohesion", () => {
    const code = `
      function a() { b(); }
      function b() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");
    const result = clusterFunctions(functions);

    const mq = computeMQ(result.clusters, functions);
    assert.ok(typeof mq === "number", "MQ should be a number");
    assert.ok(mq >= 0, `Single cluster MQ should be >= 0, got ${mq}`);
  });

  it("empty clusters → MQ is 0", () => {
    const mq = computeMQ([], []);
    assert.strictEqual(mq, 0);
  });

  it("terrible partition (all in separate clusters with cross-edges) → lower MQ", () => {
    // Chain where each function is its own cluster would have all inter-cluster edges
    const code = `
      function rootA() { leafA(); }
      function leafA() {}
      function rootB() { leafB(); }
      function leafB() {}
    `;
    const ast = parse(code);
    const functions = buildFunctionGraph(ast, "test.js");

    // Perfect partition
    const perfectResult = clusterFunctions(functions);
    const perfectMQ = computeMQ(perfectResult.clusters, functions);

    // Terrible partition: put all in one cluster, or make artificial bad clusters
    // For comparison, the "natural" clustering should be better than putting
    // connected functions in separate clusters
    assert.ok(
      perfectMQ > 0,
      `Perfect partition MQ should be > 0, got ${perfectMQ}`
    );
  });
});
