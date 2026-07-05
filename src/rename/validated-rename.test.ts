import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import { generate, traverse } from "../babel-utils.js";
import { attemptValidatedRename } from "./validated-rename.js";

function parseWithScopes(code: string): {
  ast: t.File;
  programScope: Scope;
  functionScopes: Scope[];
} {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast) throw new Error("Failed to parse test fixture");
  let programScope: Scope | undefined;
  const functionScopes: Scope[] = [];
  traverse(ast, {
    Program(path) {
      programScope = path.scope;
    },
    Function(path) {
      functionScopes.push(path.scope);
    }
  });
  if (!programScope) throw new Error("No program scope");
  return { ast: ast as t.File, programScope, functionScopes };
}

describe("attemptValidatedRename", () => {
  it("applies a valid rename and rewrites references", () => {
    const { ast, programScope } = parseWithScopes("var a = 1; console.log(a);");
    const result = attemptValidatedRename(programScope, "a", "fetchCount");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    assert.match(code, /var fetchCount = 1/);
    assert.match(code, /console\.log\(fetchCount\)/);
  });

  it("rejects a reserved word target", () => {
    const { ast, programScope } = parseWithScopes("var a = 1;");
    const result = attemptValidatedRename(programScope, "a", "delete");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "invalid-target");
    assert.match(generate(ast).code, /var a = 1/);
  });

  it("rejects a global builtin target", () => {
    const { programScope } = parseWithScopes("var a = 1;");
    const result = attemptValidatedRename(programScope, "a", "Map");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "invalid-target");
  });

  it("rejects an invalid identifier target", () => {
    const { programScope } = parseWithScopes("var a = 1;");
    const result = attemptValidatedRename(programScope, "a", "foo-bar");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "invalid-target");
  });

  it("rejects when the old name is not bound in the scope", () => {
    const { programScope } = parseWithScopes("var a = 1;");
    const result = attemptValidatedRename(programScope, "missing", "found");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "no-binding");
  });

  it("rejects when the target is already bound in the same scope", () => {
    const { ast, programScope } = parseWithScopes("var a = 1; var b = 2;");
    const result = attemptValidatedRename(programScope, "a", "b");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "target-in-scope");
    assert.match(generate(ast).code, /var a = 1/);
  });

  it("rejects two sequential renames to the same target", () => {
    // The NH failure shape: two bindings in one scope transferred to one name
    const { ast, programScope } = parseWithScopes("var a = 1; var b = 2;");
    const first = attemptValidatedRename(programScope, "a", "shared");
    const second = attemptValidatedRename(programScope, "b", "shared");
    assert.strictEqual(first.applied, true);
    assert.strictEqual(second.applied, false);
    assert.strictEqual(second.reason, "target-in-scope");
    const code = generate(ast).code;
    assert.match(code, /var shared = 1/);
    assert.match(code, /var b = 2/);
  });

  it("rejects when the target is visible from an ancestor scope", () => {
    const { functionScopes } = parseWithScopes(
      "var helper = 1; function f(a) { return a; }"
    );
    const result = attemptValidatedRename(functionScopes[0], "a", "helper");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "target-visible");
  });

  it("rejects when a child scope binds the target around a reference", () => {
    const { functionScopes } = parseWithScopes(
      "function f(a) { { let helper = 1; console.log(a, helper); } }"
    );
    const result = attemptValidatedRename(functionScopes[0], "a", "helper");
    assert.strictEqual(result.applied, false);
    assert.strictEqual(result.reason, "shadows-child");
  });

  it("allows a target bound only in an unrelated sibling scope", () => {
    const { functionScopes } = parseWithScopes(
      "function f(a) { return a; } function g(helper) { return helper; }"
    );
    const result = attemptValidatedRename(functionScopes[0], "a", "helper");
    assert.strictEqual(result.applied, true);
  });

  it("renames writes and destructuring violations, not just reads", () => {
    const { ast, programScope } = parseWithScopes(
      "var a = 1; a = 2; a += 3; [a] = [4]; console.log(a);"
    );
    const result = attemptValidatedRename(programScope, "a", "counter");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    assert.ok(!/\ba\b/.test(code), `no occurrence of 'a' may remain:\n${code}`);
    assert.match(code, /var counter = 1/);
    assert.match(code, /counter = 2/);
    assert.match(code, /counter \+= 3/);
    assert.match(code, /\[counter\] = \[4\]/);
  });

  it("renames a function declaration name and its call sites", () => {
    const { ast, programScope } = parseWithScopes(
      "function a() { return 1; } a(); var r = a;"
    );
    const result = attemptValidatedRename(programScope, "a", "getOne");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    assert.match(code, /function getOne\(\)/);
    assert.match(code, /getOne\(\)/);
    assert.match(code, /var r = getOne/);
  });

  it("preserves the external name when renaming an exported binding", () => {
    const { ast, programScope } = parseWithScopes(
      "export const a = 1; console.log(a);"
    );
    const result = attemptValidatedRename(programScope, "a", "counter");
    assert.strictEqual(result.applied, true);
    const code = generate(ast).code;
    // Whatever form Babel's renamer chooses, consumers must still see `a`.
    assert.ok(
      /export\s*\{[^}]*\bas a\b[^}]*\}/.test(code) ||
        /export const a\b/.test(code),
      `external export name 'a' must be preserved:\n${code}`
    );
    assert.strictEqual(
      validateOutputParsesForTest(code),
      null,
      `renamed export must parse:\n${code}`
    );
  });
});

/** Local parse check helper (mirrors the pipeline's output validation). */
function validateOutputParsesForTest(code: string): string | null {
  const ast = parseSync(code, { sourceType: "module" });
  return ast ? null : "parse failed";
}
