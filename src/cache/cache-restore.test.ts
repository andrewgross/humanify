import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/core";
import type * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import {
  buildPlaceholderMapping,
  computeFingerprint
} from "../analysis/structural-hash.js";
import type { FunctionNode } from "../analysis/types.js";
import { buildCache, type HumanifyCache } from "./cache-file.js";
import { restoreFromCache } from "./cache-restore.js";

function makeFunctionNode(
  code: string,
  overrides?: Partial<FunctionNode>
): FunctionNode {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse");

  let fnPath: NodePath<t.Function> | null = null;
  traverse(ast, {
    Function(p: NodePath<t.Function>) {
      if (!fnPath) fnPath = p;
    }
  });

  if (!fnPath) throw new Error("No function found");

  const resolvedPath = fnPath as NodePath<t.Function>;
  const fingerprint = computeFingerprint(resolvedPath.node);
  return {
    sessionId: overrides?.sessionId ?? "test:1:0",
    fingerprint,
    placeholderMapping: buildPlaceholderMapping(resolvedPath.node),
    path: resolvedPath,
    internalCallees: new Set(),
    externalCallees: new Set(),
    callers: new Set(),
    status: "pending",
    callSites: [],
    ...overrides
  } as FunctionNode;
}

describe("restoreFromCache", () => {
  it("matches functions with same exactHash and translates placeholder names", () => {
    // Old version: function a(b) { return b; }
    const oldFn = makeFunctionNode(`function a(b) { return b; }`, {
      sessionId: "old:1:0",
      renameMapping: { names: { a: "getUser", b: "userId" } }
    });
    const oldFunctions = new Map([["old:1:0", oldFn]]);
    const cache = buildCache(oldFunctions, "v1.min.js");

    // New version: same structure but different minified names
    const newFn = makeFunctionNode(`function x(y) { return y; }`, {
      sessionId: "new:1:0"
    });
    const newFunctions = new Map([["new:1:0", newFn]]);

    const { applied } = restoreFromCache(cache, newFunctions);

    assert.strictEqual(applied, 1);
    assert.ok(newFn.renameMapping);
    // Placeholders: $0→x (fn name), $1→y (param)
    // Cache has: $0→getUser, $1→userId
    // Result: x→getUser, y→userId
    assert.strictEqual(newFn.renameMapping.names.x, "getUser");
    assert.strictEqual(newFn.renameMapping.names.y, "userId");
  });

  it("skips functions not in cache (new functions)", () => {
    // Cache with one function
    const oldFn = makeFunctionNode(`function a(b) { return b; }`, {
      sessionId: "old:1:0",
      renameMapping: { names: { a: "getUser", b: "userId" } }
    });
    const cache = buildCache(new Map([["old:1:0", oldFn]]), "v1.min.js");

    // New version has a structurally different function
    const newFn = makeFunctionNode(
      `function z(w) { if (w) { return w + 1; } return 0; }`,
      { sessionId: "new:1:0" }
    );
    const newFunctions = new Map([["new:1:0", newFn]]);

    const { applied } = restoreFromCache(cache, newFunctions);

    assert.strictEqual(applied, 0);
    assert.strictEqual(newFn.renameMapping, undefined);
  });

  it("handles empty cache gracefully", () => {
    const emptyCache: HumanifyCache = {
      version: 1,
      sourceFile: "empty.js",
      createdAt: new Date().toISOString(),
      functions: []
    };

    const newFn = makeFunctionNode(`function a(b) { return b; }`, {
      sessionId: "new:1:0"
    });
    const newFunctions = new Map([["new:1:0", newFn]]);

    const { applied } = restoreFromCache(emptyCache, newFunctions);
    assert.strictEqual(applied, 0);
  });

  it("matches multiple functions correctly", () => {
    const oldFn1 = makeFunctionNode(`function a(b) { return b; }`, {
      sessionId: "old:1:0",
      renameMapping: { names: { a: "getUser", b: "userId" } }
    });
    const oldFn2 = makeFunctionNode(`function c(d, e) { return d + e; }`, {
      sessionId: "old:2:0",
      renameMapping: { names: { c: "add", d: "left", e: "right" } }
    });
    const cache = buildCache(
      new Map([
        ["old:1:0", oldFn1],
        ["old:2:0", oldFn2]
      ]),
      "v1.min.js"
    );

    const newFn1 = makeFunctionNode(`function x(y) { return y; }`, {
      sessionId: "new:1:0"
    });
    const newFn2 = makeFunctionNode(`function p(q, r) { return q + r; }`, {
      sessionId: "new:2:0"
    });
    const newFunctions = new Map([
      ["new:1:0", newFn1],
      ["new:2:0", newFn2]
    ]);

    const { applied } = restoreFromCache(cache, newFunctions);

    assert.strictEqual(applied, 2);
    assert.strictEqual(newFn1.renameMapping?.names.x, "getUser");
    assert.strictEqual(newFn2.renameMapping?.names.p, "add");
    assert.strictEqual(newFn2.renameMapping?.names.q, "left");
    assert.strictEqual(newFn2.renameMapping?.names.r, "right");
  });

  it("returns match result with resolution stats", () => {
    const oldFn = makeFunctionNode(`function a(b) { return b; }`, {
      sessionId: "old:1:0",
      renameMapping: { names: { a: "getUser", b: "userId" } }
    });
    const cache = buildCache(new Map([["old:1:0", oldFn]]), "v1.min.js");

    const newFn = makeFunctionNode(`function x(y) { return y; }`, {
      sessionId: "new:1:0"
    });
    const newFunctions = new Map([["new:1:0", newFn]]);

    const { matchResult } = restoreFromCache(cache, newFunctions);

    assert.ok(matchResult);
    assert.strictEqual(matchResult.matches.size, 1);
    assert.strictEqual(matchResult.resolutionStats.exactHashUnique, 1);
  });
});
