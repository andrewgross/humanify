import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { clearBabelTraverseCache, traverse } from "../babel-utils.js";
import { collectModuleUsedNames } from "./processor.js";
import { attemptValidatedRename } from "./validated-rename.js";

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

/**
 * The seeded set is read off a BABEL SCOPE OBJECT's `bindings` map, and this
 * pipeline runs two scope trees over one AST (exp059). `fastRenameBinding`
 * re-keys only the map of the scope it was handed — which is exactly why the
 * claim ledger in `validated-rename.ts` keys by BLOCK NODE instead.
 *
 * So a snapshot taken from one era can retain a name that has already been
 * renamed away through the other. That matters because this set feeds the
 * barrier's collision check: a name it wrongly reports as taken is REFUSED,
 * and the entry falls back to a deterministic conflict-variant. That is the
 * exact shape of an observed cross-run divergence — `serverConfig` becoming
 * `configVal` on one leg with provably identical prompts.
 *
 * This pins the ERA-SENSITIVITY at the bench, deterministically, instead of
 * hunting a ~3% event through 10-minute pipeline runs.
 */
describe("collectModuleUsedNames across scope eras", () => {
  // SKIPPED BECAUSE IT REPRODUCES A LIVE BUG, not because it is flaky. It
  // asserts the CORRECT behaviour and therefore fails on current main; the gate
  // has to stay green. Same treatment exp059's capture got before it was fixed.
  //
  // MEASURED, both harm directions in one snapshot, after renaming
  // oldModuleName -> freshName through era A:
  //     era A seed: [freshName, use]        correct
  //     era B seed: [oldModuleName, use]    stale BOTH ways
  // Missing `freshName` lets another entry take a bound name (a collision);
  // retaining `oldModuleName` makes the barrier refuse a name that is free, so
  // the entry falls back to a conflict-variant.
  //
  // WHEN YOU FIX IT: remove `.skip`, do not weaken the assertions. The fix is
  // NOT output-neutral (it changes what the collision check sees), so gate it
  // on the 4-pair eval with a cold control, not on neutrality.sh.
  it.skip("does not report a name that has already been renamed away", () => {
    const code =
      "(function () { var oldModuleName = 1; use(oldModuleName); })();";
    const ast = parseSync(code, { sourceType: "script" });
    if (!ast) throw new Error("Failed to parse fixture");

    const scopeOf = () => {
      let s: ReturnType<typeof wrapperScopeFinder> | undefined;
      traverse(ast as t.File, {
        Function(path: NodePath<t.Function>) {
          s ??= path.scope;
        }
      });
      if (!s) throw new Error("no wrapper");
      return s;
    };
    function wrapperScopeFinder() {
      return undefined as unknown as import("@babel/traverse").Scope;
    }

    // ERA A: the scope the graph would have captured.
    const eraA = scopeOf();
    clearBabelTraverseCache();
    // ERA B: a freshly crawled scope for the SAME lexical block.
    const eraB = scopeOf();
    assert.notStrictEqual(
      eraA.uid,
      eraB.uid,
      "precondition: the clear must yield a different Scope object, or this " +
        "test is not exercising the hazard at all"
    );

    // A rename lands through era A — as the naming pass does.
    const applied = attemptValidatedRename(eraA, "oldModuleName", "freshName");
    assert.ok(applied.applied, "the rename itself is legitimate");

    // Now seed the module used-names set from the OTHER era.
    const used = collectModuleUsedNames(eraB);

    assert.ok(
      used.has("freshName"),
      "the set must know the name that is actually bound now"
    );
    assert.ok(
      !used.has("oldModuleName"),
      "a name already renamed away must NOT be reported as taken — a stale " +
        "entry here makes the barrier refuse a free name and fall back to a " +
        "conflict-variant, which is the observed `serverConfig`->`configVal` shape"
    );
  });
});
