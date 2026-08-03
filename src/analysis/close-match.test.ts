import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFingerprintIndex, matchFunctions } from "./fingerprint-index.js";
import { buildFunctionGraph } from "./function-graph.js";
import {
  CLOSE_MATCH_TOP_K,
  findCloseMatches,
  scorePairs
} from "./close-match.js";
import type { FingerprintIndex } from "./types.js";

describe("scorePairs candidate bound", () => {
  it("keeps at most top-K candidates per old function", () => {
    // 50x50 identical vectors used to materialize 2,500 pairs; on the
    // real bundle that is ~8Kx8K unmatched functions - a memory cliff.
    const vector = {
      arity: 1,
      complexity: 2,
      returnCount: 1,
      loopCount: 0,
      branchCount: 1,
      tryCount: 0,
      calleeCount: 1,
      externalCallCount: 1,
      stringLiteralCount: 1,
      propertyAccessCount: 2,
      numericLiteralCount: 0,
      hasRestParam: 0
    };
    const olds = new Map(
      Array.from({ length: 50 }, (_, i) => [`old${i}`, { ...vector }])
    );
    const news = new Map(
      Array.from({ length: 50 }, (_, i) => [`new${i}`, { ...vector }])
    );

    const candidates = scorePairs(olds, news, 0.8);

    assert.ok(
      candidates.length <= 50 * CLOSE_MATCH_TOP_K,
      `expected <= ${50 * CLOSE_MATCH_TOP_K} candidates, got ${candidates.length}`
    );
    const oldsCovered = new Set(candidates.map((c) => c.oldId));
    assert.strictEqual(oldsCovered.size, 50, "every old keeps its best K");
  });
});

function parse(code: string): t.File {
  const ast = parseSync(code, { sourceType: "module" });
  if (!ast || ast.type !== "File") throw new Error("Failed to parse code");
  return ast;
}

function buildIndex(code: string): FingerprintIndex {
  const ast = parse(code);
  const functions = buildFunctionGraph(ast, "test.js");
  const map = new Map(functions.map((f) => [f.sessionId, f]));
  return buildFingerprintIndex(map);
}

describe("findCloseMatches", () => {
  it("matches functions that differ by one statement", () => {
    // Function with a minor modification (added console.log)
    const codeV1 = `
      function process(x) {
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
    `;
    const codeV2 = `
      function process(x) {
        console.log("debug");
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
    `;

    const oldIndex = buildIndex(codeV1);
    const newIndex = buildIndex(codeV2);

    // Standard matching should not match (different structure)
    const result = matchFunctions(oldIndex, newIndex);
    assert.strictEqual(result.matches.size, 0, "Should not exact-match");

    // Close matching should find the similarity
    const closeResult = findCloseMatches(
      result.unmatched,
      [...newIndex.fingerprints.keys()].filter(
        (id) => !new Set(result.matches.values()).has(id)
      ),
      oldIndex,
      newIndex
    );

    assert.strictEqual(
      closeResult.closeMatches.size,
      1,
      "Should find 1 close match"
    );

    // Check that similarity score is high
    for (const score of closeResult.scores.values()) {
      assert.ok(score > 0.5, `Similarity should be high, got ${score}`);
    }
  });

  it("does not match unrelated functions", () => {
    const codeV1 = `
      function fetchData(url) {
        return fetch(url).then(function(r) { return r.json(); });
      }
    `;
    const codeV2 = `
      function calculate(x, y) {
        for (var i = 0; i < x; i++) {
          if (i > y) return i;
        }
        return 0;
      }
    `;

    const oldIndex = buildIndex(codeV1);
    const newIndex = buildIndex(codeV2);

    const result = matchFunctions(oldIndex, newIndex);

    const closeResult = findCloseMatches(
      result.unmatched,
      [...newIndex.fingerprints.keys()],
      oldIndex,
      newIndex,
      { threshold: 0.7 }
    );

    assert.strictEqual(
      closeResult.closeMatches.size,
      0,
      "Unrelated functions should not close-match"
    );
  });

  it("returns empty when no unmatched functions", () => {
    const closeResult = findCloseMatches(
      [],
      [],
      buildIndex("function a() {}"),
      buildIndex("function b() {}")
    );
    assert.strictEqual(closeResult.closeMatches.size, 0);
    assert.strictEqual(closeResult.scores.size, 0);
  });

  it("respects threshold parameter", () => {
    // Functions with meaningfully different structure (different feature vectors)
    const codeV1 = `
      function calc(x) {
        return x + 1;
      }
    `;
    // V2: same-ish concept but with loops, branches, try/catch — different features
    const codeV2 = `
      function calc(x) {
        for (var i = 0; i < x; i++) {
          if (i > 5) return i;
        }
        return x + 1;
      }
    `;

    const oldIndex = buildIndex(codeV1);
    const newIndex = buildIndex(codeV2);
    const result = matchFunctions(oldIndex, newIndex);
    const unmatchedNew = [...newIndex.fingerprints.keys()].filter(
      (id) => !new Set(result.matches.values()).has(id)
    );

    // Very high threshold — should not match (structures are meaningfully different)
    const strict = findCloseMatches(
      result.unmatched,
      unmatchedNew,
      oldIndex,
      newIndex,
      { threshold: 0.99 }
    );
    assert.strictEqual(
      strict.closeMatches.size,
      0,
      "Strict threshold should reject"
    );

    // Lower threshold — should match (same arity, some overlap)
    const relaxed = findCloseMatches(
      result.unmatched,
      unmatchedNew,
      oldIndex,
      newIndex,
      { threshold: 0.3 }
    );
    assert.strictEqual(
      relaxed.closeMatches.size,
      1,
      "Relaxed threshold should accept"
    );
  });

  it("picks the best match when multiple candidates exist", () => {
    // One old function, two new candidates with different similarity
    const codeV1 = `
      function process(x) {
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
    `;
    // V2: similar (added one line) and very different function
    const codeV2 = `
      function processV2(x) {
        console.log("start");
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
      function totallyDifferent(a, b, c) {
        try { return a + b + c; } catch(e) { return 0; }
      }
    `;

    const oldIndex = buildIndex(codeV1);
    const newIndex = buildIndex(codeV2);

    const result = matchFunctions(oldIndex, newIndex);
    const unmatchedNew = [...newIndex.fingerprints.keys()].filter(
      (id) => !new Set(result.matches.values()).has(id)
    );

    const closeResult = findCloseMatches(
      result.unmatched,
      unmatchedNew,
      oldIndex,
      newIndex,
      { threshold: 0.3 }
    );

    // Should match to the similar function, not the totally different one
    assert.strictEqual(closeResult.closeMatches.size, 1);
  });
});

/**
 * `findCloseMatches` scores a fingerprint only if it carries `features`, and
 * every fingerprint that does not is skipped. Today only function ids reach
 * it, and `buildFullFingerprint` always populates `features` — so the skip
 * never fires and the tier looks healthy.
 *
 * `buildBindingFullFingerprint` does NOT populate `features`. The moment a
 * caller passes binding ids — which the binding-corroboration work explicitly
 * contemplates — every id is dropped and the function returns an empty map,
 * which is indistinguishable from "these functions are genuinely not close".
 *
 * That is exactly the failure `singletonContradicts` had: a guard that was
 * structurally dead on the binding path reported 11,094 zero-corroboration
 * accepts as `singletonRejected: 0`, which reads like perfect precision. The
 * counter below is what makes the difference legible instead of invisible.
 */
describe("findCloseMatches reports what it could not score", () => {
  /** A fingerprint with the relational fields but no `features` — what
   *  `buildBindingFullFingerprint` produces for every module binding. */
  function featurelessIndex(ids: string[]): FingerprintIndex {
    const fingerprints = new Map(
      ids.map((id) => [
        id,
        {
          structuralHash: `hash-${id}`,
          calleeShapes: [],
          callerShapes: [],
          calleeHashes: [],
          twoHopShapes: []
        }
      ])
    );
    return { fingerprints, byHash: new Map() } as unknown as FingerprintIndex;
  }

  it("counts ids it skipped for lack of features, per side", () => {
    const result = findCloseMatches(
      ["oldA", "oldB"],
      ["newA"],
      featurelessIndex(["oldA", "oldB"]),
      featurelessIndex(["newA"])
    );

    assert.strictEqual(result.closeMatches.size, 0, "nothing can be scored");
    assert.strictEqual(
      result.skippedOld,
      2,
      "both old ids were dropped for lack of features and it must be visible"
    );
    assert.strictEqual(result.skippedNew, 1);
  });

  it("reports zero skips when every fingerprint carries features", () => {
    // The live path. If this ever reports a skip, some producer stopped
    // populating `features` and the close-match tier is quietly shrinking.
    const oldIndex = buildIndex("function a(x) { return x + 1; }");
    const newIndex = buildIndex("function b(x) { return x + 2; }");
    const result = findCloseMatches(
      [...oldIndex.fingerprints.keys()],
      [...newIndex.fingerprints.keys()],
      oldIndex,
      newIndex
    );
    assert.strictEqual(result.skippedOld, 0);
    assert.strictEqual(result.skippedNew, 0);
  });

  it("distinguishes 'nothing eligible' from 'nothing close'", () => {
    // The two cases the old signature collapsed into one empty Map.
    const nothingEligible = findCloseMatches(
      ["a"],
      ["b"],
      featurelessIndex(["a"]),
      featurelessIndex(["b"])
    );
    const oldIndex = buildIndex("function a(x) { return x + 1; }");
    const newIndex = buildIndex(
      "function b(a, b, c) { for (;;) { try { return a; } catch (e) {} } }"
    );
    const nothingClose = findCloseMatches(
      [...oldIndex.fingerprints.keys()],
      [...newIndex.fingerprints.keys()],
      oldIndex,
      newIndex,
      { threshold: 0.99 }
    );

    assert.strictEqual(nothingEligible.closeMatches.size, 0);
    assert.strictEqual(nothingClose.closeMatches.size, 0);
    assert.ok(
      nothingEligible.skippedOld > 0 && nothingClose.skippedOld === 0,
      "the skip counters are what tells these two apart"
    );
  });
});
