import assert from "node:assert";
import { describe, it } from "node:test";
import { computeShingleSet } from "./function-fingerprint.js";
import {
  registerNodeCacheReset,
  resetAnalysisNodeCaches
} from "./node-caches.js";
import type { FunctionNode } from "./types.js";

/** Minimal FunctionNode for computeShingleSet: empty callee set short-circuits
 *  the edge-ngram walk, so only fingerprint.features is read. */
function fakeFn(stringLiterals: string[]): FunctionNode {
  return {
    internalCallees: new Set(),
    fingerprint: {
      structuralHash: "hash",
      features: {
        externalCalls: [],
        propertyAccesses: [],
        stringLiterals
      }
    }
  } as unknown as FunctionNode;
}

describe("resetAnalysisNodeCaches", () => {
  it("runs every registered reset", () => {
    let calls = 0;
    registerNodeCacheReset(() => {
      calls++;
    });
    resetAnalysisNodeCaches();
    assert.strictEqual(calls, 1);
    resetAnalysisNodeCaches();
    assert.strictEqual(calls, 2);
  });

  it("drops the shingle memoization so recompute sees fresh inputs", () => {
    const literals = ["alpha"];
    const fn = fakeFn(literals);

    const first = computeShingleSet(fn);
    assert.ok(first.has("str:alpha"));

    // Same FunctionNode identity → the memoized set masks the new literal.
    literals.push("beta");
    const cached = computeShingleSet(fn);
    assert.strictEqual(cached, first);
    assert.ok(!cached.has("str:beta"));

    resetAnalysisNodeCaches();
    const fresh = computeShingleSet(fn);
    assert.notStrictEqual(fresh, first);
    assert.ok(fresh.has("str:beta"));
  });
});
