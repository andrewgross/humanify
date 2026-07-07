import assert from "node:assert";
import { createIsEligible } from "./rename-eligibility.js";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import type { FunctionNode } from "../analysis/types.js";
import { buildContext } from "./context-builder.js";

function makeFunctionNode(code: string): { fn: FunctionNode; ast: t.File } {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse test fixture");
  let fnPath: NodePath<t.Function> | undefined;
  traverse(ast, {
    Function(path: NodePath<t.Function>) {
      fnPath ??= path;
    }
  });
  if (!fnPath) throw new Error("No function in fixture");
  const fn = {
    sessionId: "test:1:0",
    path: fnPath,
    callSites: [],
    internalCallees: new Set(),
    externalCallees: new Set(),
    callers: new Set(),
    status: "pending"
  } as unknown as FunctionNode;
  return { fn, ast: ast as t.File };
}

describe("buildContext usedIdentifiers", () => {
  it("includes the file's observed free names for nested functions (review C1)", () => {
    // scope.globals is only populated on the Program scope; reading it
    // off the function's own scope silently yields {} and lets the LLM
    // pick a name the file uses as a global.
    const { fn, ast } = makeFunctionNode(
      "var top1 = 1; function f(p) { return myAppGlobal.title + p + top1; }"
    );
    const context = buildContext(fn, ast, createIsEligible());
    assert.ok(
      context.usedIdentifiers.has("myAppGlobal"),
      "file free names must be in usedIdentifiers"
    );
    assert.ok(
      context.usedIdentifiers.has("top1"),
      "ancestor bindings must be in usedIdentifiers"
    );
    assert.ok(
      context.usedIdentifiers.has("p"),
      "own bindings must be in usedIdentifiers"
    );
  });

  it("includes free names referenced only in sibling functions", () => {
    // The invariant is file-wide: a sibling's free reference is captured
    // just the same if this function's binding is renamed at module scope.
    const { fn, ast } = makeFunctionNode(
      "function f(p) { return p; } function g() { return siblingGlobal.x; }"
    );
    const context = buildContext(fn, ast, createIsEligible());
    assert.ok(
      context.usedIdentifiers.has("siblingGlobal"),
      "free names from anywhere in the file must be in usedIdentifiers"
    );
  });
});
