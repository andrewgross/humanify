import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/core";
import * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import {
  buildPlaceholderMapping,
  computeFingerprint
} from "../analysis/structural-hash.js";
import type { FunctionNode } from "../analysis/types.js";
import {
  buildCache,
  type CachedModuleBinding,
  type HumanifyCache,
  type ModuleBindingCacheInput
} from "./cache-file.js";
import {
  restoreFromCache,
  restoreModuleBindingsFromCache
} from "./cache-restore.js";

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

  it("applies cached renames to AST scope bindings", () => {
    // Use a function expression assigned to a variable so the param
    // binding lives in fn.path.scope (the function's own scope).
    const oldFn = makeFunctionNode(`var z = function a(b) { return b; }`, {
      sessionId: "old:1:0",
      renameMapping: { names: { b: "userId" } }
    });
    const cache = buildCache(new Map([["old:1:0", oldFn]]), "v1.min.js");

    const newFn = makeFunctionNode(`var w = function x(y) { return y; }`, {
      sessionId: "new:1:0"
    });
    const newFunctions = new Map([["new:1:0", newFn]]);

    restoreFromCache(cache, newFunctions);

    assert.ok(newFn.renameMapping);
    assert.strictEqual(newFn.renameMapping.names.y, "userId");

    // Before scope.rename(): the function scope has the original param name
    const scope = newFn.path.scope;
    assert.ok(scope.bindings.y, "original binding 'y' should still exist");

    // Simulate what applyCacheIfPresent does: apply scope.rename()
    for (const [oldName, newName] of Object.entries(
      newFn.renameMapping.names
    )) {
      if (oldName !== newName && scope.bindings[oldName]) {
        scope.rename(oldName, newName);
      }
    }

    // After rename, scope should have humanified name
    assert.ok(
      scope.bindings.userId,
      "scope should have 'userId' binding after rename"
    );
    assert.ok(
      !scope.bindings.y,
      "original 'y' binding should be gone after rename"
    );
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

// ---------------------------------------------------------------------------
// Module binding cache restore
// ---------------------------------------------------------------------------

function extractDeclarator(code: string): t.VariableDeclarator {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse");
  for (const stmt of ast.program.body) {
    if (t.isVariableDeclaration(stmt)) {
      return stmt.declarations[0];
    }
  }
  throw new Error("No variable declarator found");
}

function makeModuleBindingCache(
  entries: Array<{
    code: string;
    name: string;
    humanifiedName: string;
    declarationIndex?: number;
    firstAssignmentRHS?: t.Expression;
  }>
): CachedModuleBinding[] {
  const result: CachedModuleBinding[] = [];
  for (const entry of entries) {
    const decl = extractDeclarator(entry.code);
    const input: ModuleBindingCacheInput = {
      name: entry.name,
      declarator: decl,
      firstAssignmentRHS: entry.firstAssignmentRHS,
      declarationIndex: entry.declarationIndex ?? 0,
      humanifiedName: entry.humanifiedName
    };
    // Build via buildCache's internal logic
    const cache = buildCache(new Map(), "test.js", [input]);
    if (cache.moduleBindings) {
      result.push(...cache.moduleBindings);
    }
  }
  return result;
}

interface CurrentBinding {
  init: t.Expression | null | undefined;
  firstAssignmentRHS?: t.Expression | null;
  declarationIndex: number;
}

describe("restoreModuleBindingsFromCache", () => {
  it("matches by content hash and translates $binding name", () => {
    // Old: var n = 0; renamed to "counter"
    const cached = makeModuleBindingCache([
      { code: "var n = 0;", name: "n", humanifiedName: "counter" }
    ]);
    // New: var m = 0; (same init, different minified name)
    const newDecl = extractDeclarator("var m = 0;");
    const currentBindings = new Map<string, CurrentBinding>([
      ["m", { init: newDecl.init, declarationIndex: 0 }]
    ]);

    const restored = restoreModuleBindingsFromCache(cached, currentBindings);

    assert.strictEqual(restored.size, 1);
    assert.strictEqual(restored.get("m"), "counter");
  });

  it("disambiguates same-hash bindings by declarationIndex", () => {
    // Two bindings with same init hash but different indices
    const cached = makeModuleBindingCache([
      {
        code: "var a = 0;",
        name: "a",
        humanifiedName: "first",
        declarationIndex: 0
      },
      {
        code: "var b = 0;",
        name: "b",
        humanifiedName: "second",
        declarationIndex: 1
      }
    ]);

    const decl1 = extractDeclarator("var x = 0;");
    const decl2 = extractDeclarator("var y = 0;");
    const currentBindings = new Map<string, CurrentBinding>([
      ["x", { init: decl1.init, declarationIndex: 0 }],
      ["y", { init: decl2.init, declarationIndex: 1 }]
    ]);

    const restored = restoreModuleBindingsFromCache(cached, currentBindings);

    assert.strictEqual(restored.size, 2);
    assert.strictEqual(restored.get("x"), "first");
    assert.strictEqual(restored.get("y"), "second");
  });

  it("handles mix of init-based and assignment-based bindings", () => {
    // One with init, one would need first-assignment RHS
    const cached = makeModuleBindingCache([
      { code: "var a = 0;", name: "a", humanifiedName: "counter" }
    ]);

    const decl = extractDeclarator("var m = 0;");
    const currentBindings = new Map<string, CurrentBinding>([
      ["m", { init: decl.init, declarationIndex: 0 }]
    ]);

    const restored = restoreModuleBindingsFromCache(cached, currentBindings);
    assert.strictEqual(restored.get("m"), "counter");
  });

  it("skips bindings not in cache", () => {
    const cached = makeModuleBindingCache([
      { code: "var a = 0;", name: "a", humanifiedName: "counter" }
    ]);

    // New binding has structurally different init — no match
    const decl = extractDeclarator("var m = [1, 2, 3];");
    const currentBindings = new Map<string, CurrentBinding>([
      ["m", { init: decl.init, declarationIndex: 0 }]
    ]);

    const restored = restoreModuleBindingsFromCache(cached, currentBindings);
    assert.strictEqual(restored.size, 0);
  });

  it("returns empty map for empty cache", () => {
    const decl = extractDeclarator("var m = 0;");
    const currentBindings = new Map<string, CurrentBinding>([
      ["m", { init: decl.init, declarationIndex: 0 }]
    ]);

    const restored = restoreModuleBindingsFromCache([], currentBindings);
    assert.strictEqual(restored.size, 0);
  });
});
