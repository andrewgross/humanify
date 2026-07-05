import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import { collectModuleUsedNames } from "./processor.js";

/**
 * Split out of processor.test.ts deliberately — that file pins legacy
 * paths slated for deletion (review P3); this one tests the live
 * processUnified seeding.
 */
describe("collectModuleUsedNames", () => {
  function wrapperScope(code: string) {
    const ast = parseSync(code, { sourceType: "script" });
    if (!ast) throw new Error("Failed to parse fixture");
    let scope: NodePath<t.Function>["scope"] | undefined;
    traverse(ast, {
      Function(path: NodePath<t.Function>) {
        scope ??= path.scope;
      }
    });
    if (!scope) throw new Error("No wrapper function in fixture");
    return scope;
  }

  it("includes the wrapper scope's own bindings", () => {
    const scope = wrapperScope("(function () { var wrapped = 1; })();");
    const used = collectModuleUsedNames(scope);
    assert.ok(used.has("wrapped"));
  });

  it("includes the file's free names even for a wrapper (non-Program) scope (review C1)", () => {
    // scope.globals is populated only on the Program scope; Bun bundles'
    // target scope is the wrapper IIFE, where .globals is always {} —
    // seeding from it silently loses every free name.
    const scope = wrapperScope(
      "(function () { var wrapped = myAppGlobal.value; })();"
    );
    const used = collectModuleUsedNames(scope);
    assert.ok(
      used.has("myAppGlobal"),
      "file free names must seed usedNames even when the target scope is a wrapper"
    );
  });
});
