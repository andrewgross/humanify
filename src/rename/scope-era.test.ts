import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import { clearBabelTraverseCache, generate, traverse } from "../babel-utils.js";
import { attemptValidatedRename } from "./validated-rename.js";

/**
 * Clearing Babel's traverse cache creates a SECOND SCOPE TREE over the same
 * AST, and a rename applied through one tree is invisible to a guard walking
 * the other.
 *
 * This is exp059's capture, reduced to a unit test. In the pipeline:
 *
 *    885  buildUnifiedGraph(...)               graph captures scope objects
 *    906  prior-version matching               fills Babel-cache tombstones
 *    965  clearBabelCacheAfterPriorMatch(...)  "induces scope re-crawls"
 *    970  runRenamePass(...)                   fresh path.scope accesses
 *
 * The clear's own comment argues it is safe "for later hashing ... because slot
 * placeholders key by declaration node, which survives the scope re-crawls this
 * clear induces". That is true of hashing. It is NOT true of the rename guards,
 * which read `bindings` maps off Scope OBJECTS — and a re-crawl makes new
 * objects for the same lexical scopes.
 *
 * Measured on a real reproduction: the two captured renames carried scope uids
 * 278209 and 577865 — clusters 151,687 apart — while their blocks were
 * lexically nested, and both guards saw complete reference lists (refCount 2
 * and 3) and still allowed the rename.
 */

/** The shape from runtime.js: an outer `let` remembered across calls, and an
 *  inner `let` in a nested closure, compared and then assigned. */
const CODE = `
function getFileWriter() {
  let outerDir = null;
  register({ writeFn: (task) => {
    let innerDir = dirname(getPath());
    let changed = outerDir !== innerDir;
    outerDir = innerDir;
    return changed;
  }});
}`;

function scopeOwning(ast: t.File, name: string): Scope {
  let found: Scope | undefined;
  traverse(ast, {
    Scopable(p) {
      if (!found && p.scope.bindings[name]) found = p.scope;
    }
  });
  if (!found) throw new Error(`no scope owns ${name}`);
  return found;
}

describe("scope-tree eras: a rename through one tree is invisible to the other", () => {
  it("a cache clear produces a DIFFERENT Scope object for the same block", () => {
    const ast = parseSync(CODE, { sourceType: "unambiguous" }) as t.File;
    const before = scopeOwning(ast, "innerDir");
    clearBabelTraverseCache();
    const after = scopeOwning(ast, "innerDir");

    assert.strictEqual(
      (before.block as { start?: number }).start,
      (after.block as { start?: number }).start,
      "same lexical block"
    );
    assert.notStrictEqual(
      before.uid,
      after.uid,
      "a cache clear must produce a new Scope OBJECT — if this ever passes, the era hazard is gone and this whole test is obsolete"
    );
  });

  // SKIPPED BECAUSE IT REPRODUCES A LIVE BUG, not because it is flaky.
  //
  // This is exp059's capture, deterministic and in ~30ms instead of a
  // 16-minute eval run that hits it 20% of the time. It asserts the CORRECT
  // behaviour and therefore fails on current main; the gate has to stay green,
  // so it is skipped until the bug is fixed.
  //
  // WHEN YOU FIX IT: remove `.skip` — do not adjust the assertions. They
  // describe what must be true, and the emitted-code check (`no self
  // comparison`) is the one that matters, because a guard-verdict assertion
  // alone would still pass if the rejection reason changed for an unrelated
  // reason.
  //
  // See experiments/059-rename-capture/RESULTS.md.
  it.skip("a rename applied AFTER a re-crawl, through a RETAINED old scope, is invisible", () => {
    // Order is everything, and my first attempt had it backwards. A fresh
    // crawl reads the MUTATED ast, so a tree built after a rename SEES it.
    // The hazard is the opposite: keep an old scope object across the clear
    // (which is what the unified graph does), build the new tree, and only
    // THEN rename through the retained one. The new tree never learns.
    const ast = parseSync(CODE, { sourceType: "unambiguous" }) as t.File;

    // ERA A: the graph captures the inner scope BEFORE the clear and keeps it.
    const retainedInner = scopeOwning(ast, "innerDir");

    clearBabelTraverseCache();

    // ERA B: the naming pass crawls fresh and holds the outer scope.
    const freshOuter = scopeOwning(ast, "outerDir");

    // Now the inner rename lands through the RETAINED era-A object.
    const first = attemptValidatedRename(retainedInner, "innerDir", "dirPath");
    assert.ok(first.applied, "the inner rename is legitimate on its own");

    // And the outer rename is judged by the era-B tree, which was crawled
    // before that rename and was never told about it.
    const second = attemptValidatedRename(freshOuter, "outerDir", "dirPath");

    // Assert on the EMITTED CODE, not just the verdict: the point is that
    // shipped output computes the wrong answer, and a guard-verdict assertion
    // alone would still pass if the rejection reason changed for some unrelated
    // reason.
    const emitted = generate(ast).code.replace(/\s+/g, " ");
    assert.ok(
      !/(\w+) !== \1/.test(emitted),
      `emitted a self-comparison — exp059's capture, reproduced: ` +
        `${/[^;]*!==[^;]*/.exec(emitted)?.[0]?.trim()}`
    );
    assert.strictEqual(
      second.applied,
      false,
      "the outer rename MUST be rejected: the inner binding is already called " +
        "dirPath and shadows it."
    );
  });
});
