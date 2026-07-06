import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import { assertUnifiedGraphClosure } from "./graph-closure.js";

function buildGraph(code: string) {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("failed to parse");
  return buildUnifiedGraph(ast as t.File, "test.js", undefined, () => true);
}

describe("assertUnifiedGraphClosure", () => {
  const code = `
    var base = { port: 8080 };
    var alias = base;
    function useAlias() { return alias; }
  `;

  it("passes on a freshly built graph", () => {
    const graph = buildGraph(code);
    assert.doesNotThrow(() => assertUnifiedGraphClosure(graph, new Set()));
  });

  it("throws when a node with edges is deleted from the graph", () => {
    const graph = buildGraph(code);
    assert.ok(graph.nodes.delete("module:base"), "fixture must have the node");
    assert.throws(
      () => assertUnifiedGraphClosure(graph, new Set()),
      /closure violated/
    );
  });

  it("passes when the removed node is in doneIds", () => {
    const graph = buildGraph(code);
    graph.nodes.delete("module:base");
    assert.doesNotThrow(() =>
      assertUnifiedGraphClosure(graph, new Set(["module:base"]))
    );
  });
});
