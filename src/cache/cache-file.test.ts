import assert from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/core";
import type * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import { computeFingerprint } from "../analysis/structural-hash.js";
import type { FunctionNode } from "../analysis/types.js";
import { buildCache, readCache, writeCache } from "./cache-file.js";

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
    path: resolvedPath,
    internalCallees: new Set(),
    externalCallees: new Set(),
    callers: new Set(),
    status: "done",
    callSites: [],
    ...overrides
  } as FunctionNode;
}

describe("buildCache", () => {
  it("stores placeholder-keyed names (not minified names)", () => {
    // function a(b) { return b; } — a=$0, b=$1
    const fn = makeFunctionNode(`function a(b) { return b; }`, {
      sessionId: "file:1:0",
      renameMapping: { names: { a: "getUser", b: "userId" }, model: "gpt-4o" }
    });

    const functions = new Map([["file:1:0", fn]]);
    const cache = buildCache(functions, "input.min.js");

    assert.strictEqual(cache.version, 1);
    assert.strictEqual(cache.sourceFile, "input.min.js");
    assert.strictEqual(cache.functions.length, 1);

    const cached = cache.functions[0];
    // Names should be keyed by placeholder, not minified name
    assert.strictEqual(cached.renameMapping.names.$0, "getUser");
    assert.strictEqual(cached.renameMapping.names.$1, "userId");
    assert.strictEqual(cached.renameMapping.model, "gpt-4o");
    // Should NOT have minified keys
    assert.strictEqual(cached.renameMapping.names.a, undefined);
    assert.strictEqual(cached.renameMapping.names.b, undefined);
  });

  it("skips functions without renameMapping", () => {
    const fn1 = makeFunctionNode(`function a(b) { return b; }`, {
      sessionId: "file:1:0",
      renameMapping: { names: { a: "getUser", b: "userId" } }
    });
    const fn2 = makeFunctionNode(`function x(y) { return y + 1; }`, {
      sessionId: "file:2:0"
      // no renameMapping
    });

    const functions = new Map([
      ["file:1:0", fn1],
      ["file:2:0", fn2]
    ]);
    const cache = buildCache(functions, "input.min.js");

    assert.strictEqual(cache.functions.length, 1);
    assert.strictEqual(cache.functions[0].sessionId, "file:1:0");
  });

  it("skips functions with empty renameMapping", () => {
    const fn = makeFunctionNode(`function a(b) { return b; }`, {
      sessionId: "file:1:0",
      renameMapping: { names: {} }
    });

    const functions = new Map([["file:1:0", fn]]);
    const cache = buildCache(functions, "input.min.js");

    assert.strictEqual(cache.functions.length, 0);
  });

  it("stores topology sessionIds", () => {
    const parent = makeFunctionNode(`function outer() { return 1; }`, {
      sessionId: "file:1:0",
      renameMapping: { names: { outer: "main" } }
    });
    const callee = makeFunctionNode(`function inner() { return 2; }`, {
      sessionId: "file:3:0",
      renameMapping: { names: { inner: "helper" } }
    });
    const caller = makeFunctionNode(`function top() { return 3; }`, {
      sessionId: "file:5:0",
      renameMapping: { names: { top: "entry" } }
    });

    const fn = makeFunctionNode(`function mid(x) { return x; }`, {
      sessionId: "file:2:0",
      renameMapping: { names: { mid: "process", x: "data" } },
      scopeParent: parent,
      internalCallees: new Set([callee]),
      callers: new Set([caller])
    });

    const functions = new Map([
      ["file:1:0", parent],
      ["file:2:0", fn],
      ["file:3:0", callee],
      ["file:5:0", caller]
    ]);
    const cache = buildCache(functions, "input.min.js");
    const cached = cache.functions.find((f) => f.sessionId === "file:2:0");
    assert.ok(cached, "Expected to find cached function file:2:0");

    assert.strictEqual(cached.scopeParentId, "file:1:0");
    assert.deepStrictEqual(cached.calleeIds, ["file:3:0"]);
    assert.deepStrictEqual(cached.callerIds, ["file:5:0"]);
  });

  it("stores fingerprint data", () => {
    const fn = makeFunctionNode(`function a(b) { return b; }`, {
      sessionId: "file:1:0",
      renameMapping: { names: { a: "getUser", b: "userId" } }
    });

    const functions = new Map([["file:1:0", fn]]);
    const cache = buildCache(functions, "input.min.js");

    assert.strictEqual(
      cache.functions[0].fingerprint.exactHash,
      fn.fingerprint.exactHash
    );
  });
});

describe("writeCache / readCache round-trip", () => {
  it("round-trips cache data through JSON file", () => {
    const fn = makeFunctionNode(`function a(b) { return b; }`, {
      sessionId: "file:1:0",
      renameMapping: { names: { a: "getUser", b: "userId" } }
    });

    const functions = new Map([["file:1:0", fn]]);
    const cache = buildCache(functions, "input.min.js");

    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "humanify-test-"));
    const cachePath = path.join(tmpDir, "test.cache.json");

    try {
      writeCache(cache, cachePath);
      const loaded = readCache(cachePath);

      assert.ok(loaded);
      assert.strictEqual(loaded.version, cache.version);
      assert.strictEqual(loaded.sourceFile, cache.sourceFile);
      assert.strictEqual(loaded.functions.length, cache.functions.length);
      assert.deepStrictEqual(
        loaded.functions[0].renameMapping.names,
        cache.functions[0].renameMapping.names
      );
      assert.strictEqual(
        loaded.functions[0].fingerprint.exactHash,
        cache.functions[0].fingerprint.exactHash
      );
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  it("readCache returns null for non-existent file", () => {
    const result = readCache("/tmp/nonexistent-cache-file.json");
    assert.strictEqual(result, null);
  });
});
