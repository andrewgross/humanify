import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFingerprintIndex, matchFunctions } from "./fingerprint-index.js";
import { buildFunctionGraph } from "./function-graph.js";
import { findCloseMatches } from "./close-match.js";
import type { FingerprintIndex } from "./types.js";

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
