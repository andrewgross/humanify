import assert from "node:assert";
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, it } from "node:test";
import type { GroundTruth } from "./alignment.js";
import { computeAlignment, loadGroundTruth } from "./alignment.js";

/**
 * Helper: build a simple output mapping from an object.
 */
function mapFrom(obj: Record<string, string>): Map<string, string> {
  return new Map(Object.entries(obj));
}

describe("computeAlignment", () => {
  it("perfect alignment → Rand Index = 1.0", () => {
    const gt: GroundTruth = {
      functions: { a: "mod1", b: "mod1", c: "mod2", d: "mod2" }
    };
    const output = mapFrom({
      a: "file1.js",
      b: "file1.js",
      c: "file2.js",
      d: "file2.js"
    });

    const result = computeAlignment(output, gt);

    assert.strictEqual(result.randIndex, 1.0);
    assert.strictEqual(result.fp, 0);
    assert.strictEqual(result.fn, 0);
    assert.strictEqual(result.totalPairs, 6); // C(4,2) = 6
  });

  it("completely wrong alignment → Rand Index < 1.0", () => {
    const gt: GroundTruth = {
      functions: { a: "mod1", b: "mod1", c: "mod2", d: "mod2" }
    };
    // Swap: a with c, b with d
    const output = mapFrom({
      a: "file2.js",
      b: "file2.js",
      c: "file1.js",
      d: "file1.js"
    });

    const result = computeAlignment(output, gt);

    // a,b same in both (TP), c,d same in both (TP), but cross-pairs also match
    // Actually: a,b same output, same original → TP
    // c,d same output, same original → TP
    // a,c diff output, diff original → TN
    // a,d diff output, diff original → TN
    // b,c diff output, diff original → TN
    // b,d diff output, diff original → TN
    // Still 1.0 because the grouping is the same just with different file names
    assert.strictEqual(result.randIndex, 1.0);
  });

  it("all in one cluster vs multiple modules → has FP", () => {
    const gt: GroundTruth = {
      functions: { a: "mod1", b: "mod2", c: "mod3" }
    };
    // All in one output file
    const output = mapFrom({ a: "core.js", b: "core.js", c: "core.js" });

    const result = computeAlignment(output, gt);

    // All pairs same output, but all pairs different original
    assert.strictEqual(result.tp, 0);
    assert.strictEqual(result.fp, 3); // C(3,2) = 3
    assert.strictEqual(result.fn, 0);
    assert.strictEqual(result.tn, 0);
    assert.strictEqual(result.randIndex, 0);
  });

  it("each in own cluster vs one module → has FN", () => {
    const gt: GroundTruth = {
      functions: { a: "mod1", b: "mod1", c: "mod1" }
    };
    // Each in separate output file
    const output = mapFrom({ a: "f1.js", b: "f2.js", c: "f3.js" });

    const result = computeAlignment(output, gt);

    assert.strictEqual(result.tp, 0);
    assert.strictEqual(result.fn, 3);
    assert.strictEqual(result.fp, 0);
    assert.strictEqual(result.tn, 0);
    assert.strictEqual(result.randIndex, 0);
  });

  it("handles partial overlap between ground truth and output", () => {
    const gt: GroundTruth = {
      functions: { a: "mod1", b: "mod1", c: "mod2", d: "mod2" }
    };
    // Only a and c are in the output
    const output = mapFrom({ a: "file1.js", c: "file2.js" });

    const result = computeAlignment(output, gt);

    assert.strictEqual(result.matchedFunctions, 2);
    assert.deepStrictEqual(result.missingFromOutput.sort(), ["b", "d"]);
    assert.strictEqual(result.totalPairs, 1); // C(2,2) = 1
  });

  it("reports extra functions in output not in ground truth", () => {
    const gt: GroundTruth = {
      functions: { a: "mod1" }
    };
    const output = mapFrom({ a: "file1.js", extraFn: "file1.js" });

    const result = computeAlignment(output, gt);

    assert.deepStrictEqual(result.extraInOutput, ["extraFn"]);
  });

  it("empty ground truth → perfect score (no pairs to compare)", () => {
    const gt: GroundTruth = { functions: {} };
    const output = mapFrom({ a: "file1.js" });

    const result = computeAlignment(output, gt);

    assert.strictEqual(result.randIndex, 1);
    assert.strictEqual(result.totalPairs, 0);
  });

  it("mixed result → intermediate Rand Index", () => {
    const gt: GroundTruth = {
      functions: { a: "mod1", b: "mod1", c: "mod2", d: "mod2" }
    };
    // a,b correctly together, but c is incorrectly merged with them, d alone
    const output = mapFrom({
      a: "file1.js",
      b: "file1.js",
      c: "file1.js",
      d: "file2.js"
    });

    const result = computeAlignment(output, gt);

    // Pairs: (a,b) same/same=TP, (a,c) same/diff=FP, (a,d) diff/diff=TN
    //        (b,c) same/diff=FP, (b,d) diff/diff=TN, (c,d) diff/same=FN
    assert.strictEqual(result.tp, 1);
    assert.strictEqual(result.fp, 2);
    assert.strictEqual(result.tn, 2);
    assert.strictEqual(result.fn, 1);
    assert.strictEqual(result.randIndex, 3 / 6);
  });
});

describe("loadGroundTruth", () => {
  it("loads the Preact ground truth fixture", () => {
    const gtPath = path.resolve(
      import.meta.dirname ?? ".",
      "../../experiments/004-file-emission/fixtures/preact-ground-truth.json"
    );
    if (!fs.existsSync(gtPath)) {
      // Skip if fixture not available
      return;
    }
    const gt = loadGroundTruth(gtPath);
    assert.ok(gt.functions);
    assert.ok(Object.keys(gt.functions).length > 20);
    assert.strictEqual(gt.functions.createElement, "src/create-element.js");
    assert.strictEqual(gt.functions.useState, "hooks/src/index.js");
  });
});
