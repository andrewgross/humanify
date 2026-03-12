import { describe, it } from "node:test";
import assert from "node:assert";
import { buildDiagnosticsReport, type DiagnosticsReport } from "./diagnostics.js";
import type { FunctionRenameReport } from "../analysis/types.js";
import type { CoverageSummary } from "./coverage.js";

const emptyCoverage: CoverageSummary = {
  functions: { total: 0, renamed: 0, library: 0, noMinifiedIds: 0 },
  moduleBindings: { total: 0, renamed: 0, skipped: 0 },
  identifiers: {
    total: 0, renamed: 0, notMinified: 0, skippedByHeuristic: 0,
    llmMissing: 0, llmCollision: 0, llmInvalid: 0, llmUnchanged: 0,
  },
};

describe("buildDiagnosticsReport", () => {
  it("categorizes outcomes correctly", () => {
    const reports: FunctionRenameReport[] = [
      {
        functionId: "fn:1:0",
        totalIdentifiers: 6,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "counter", round: 1 },
          b: { status: "renamed", newName: "value", round: 2 },
          c: { status: "unchanged", attempts: 2, suggestion: "c" },
          d: { status: "missing", attempts: 2, lastFinishReason: "length" },
          e: { status: "duplicate", conflictedWith: "data", attempts: 2, suggestion: "data" },
          f: { status: "invalid", attempts: 1, suggestion: "123bad" },
        },
        totalLLMCalls: 2,
        finishReasons: ["stop", "stop"],
      },
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    assert.strictEqual(diag.renamed.length, 2);
    assert.strictEqual(diag.unrenamed.unchanged.length, 1);
    assert.strictEqual(diag.unrenamed.missing.length, 1);
    assert.strictEqual(diag.unrenamed.duplicate.length, 1);
    assert.strictEqual(diag.unrenamed.invalid.length, 1);

    // Check renamed entries
    assert.deepStrictEqual(diag.renamed[0], {
      name: "a", newName: "counter", functionId: "fn:1:0", round: 1,
    });

    // Check suggestion is preserved
    assert.strictEqual(diag.unrenamed.unchanged[0].suggestion, "c");
    assert.strictEqual(diag.unrenamed.duplicate[0].suggestion, "data");
    assert.strictEqual(diag.unrenamed.invalid[0].suggestion, "123bad");

    // Check detail for missing
    assert.strictEqual(diag.unrenamed.missing[0].detail, "finish_reason: length");
  });

  it("computes top collision targets", () => {
    const reports: FunctionRenameReport[] = [
      {
        functionId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 0,
        outcomes: {
          a: { status: "duplicate", conflictedWith: "data", attempts: 1, suggestion: "data" },
          b: { status: "duplicate", conflictedWith: "data", attempts: 1, suggestion: "data" },
          c: { status: "duplicate", conflictedWith: "value", attempts: 1, suggestion: "value" },
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"],
      },
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    assert.strictEqual(diag.patterns.topCollisionTargets.length, 2);
    assert.strictEqual(diag.patterns.topCollisionTargets[0].name, "data");
    assert.strictEqual(diag.patterns.topCollisionTargets[0].count, 2);
    assert.strictEqual(diag.patterns.topCollisionTargets[1].name, "value");
    assert.strictEqual(diag.patterns.topCollisionTargets[1].count, 1);
  });

  it("computes lowest coverage functions", () => {
    const reports: FunctionRenameReport[] = [
      {
        functionId: "fn:1:0",
        totalIdentifiers: 10,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "x", round: 1 },
          b: { status: "renamed", newName: "y", round: 1 },
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"],
      },
      {
        functionId: "fn:5:0",
        totalIdentifiers: 4,
        renamedCount: 4,
        outcomes: {
          c: { status: "renamed", newName: "w", round: 1 },
          d: { status: "renamed", newName: "z", round: 1 },
          e: { status: "renamed", newName: "q", round: 1 },
          f: { status: "renamed", newName: "r", round: 1 },
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"],
      },
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    assert.strictEqual(diag.patterns.lowestCoverageFunctions[0].functionId, "fn:1:0");
    assert.strictEqual(diag.patterns.lowestCoverageFunctions[0].pct, 20);
  });

  it("tracks failure attempt distribution", () => {
    const reports: FunctionRenameReport[] = [
      {
        functionId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 0,
        outcomes: {
          a: { status: "missing", attempts: 1 },
          b: { status: "missing", attempts: 3 },
          c: { status: "unchanged", attempts: 1 },
        },
        totalLLMCalls: 3,
        finishReasons: ["stop", "stop", "stop"],
      },
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    assert.strictEqual(diag.patterns.failuresByAttempts[1], 2); // a + c
    assert.strictEqual(diag.patterns.failuresByAttempts[3], 1); // b
  });

  it("tracks missing by finish reason", () => {
    const reports: FunctionRenameReport[] = [
      {
        functionId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 0,
        outcomes: {
          a: { status: "missing", attempts: 1, lastFinishReason: "length" },
          b: { status: "missing", attempts: 1, lastFinishReason: "length" },
          c: { status: "missing", attempts: 1 },
        },
        totalLLMCalls: 1,
        finishReasons: ["length"],
      },
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    assert.strictEqual(diag.patterns.missingByFinishReason["length"], 2);
    assert.strictEqual(diag.patterns.missingByFinishReason["unknown"], 1);
  });

  it("handles empty reports", () => {
    const diag = buildDiagnosticsReport([], emptyCoverage);

    assert.strictEqual(diag.renamed.length, 0);
    assert.strictEqual(diag.unrenamed.unchanged.length, 0);
    assert.strictEqual(diag.unrenamed.missing.length, 0);
    assert.strictEqual(diag.unrenamed.duplicate.length, 0);
    assert.strictEqual(diag.unrenamed.invalid.length, 0);
    assert.strictEqual(diag.patterns.topCollisionTargets.length, 0);
    assert.strictEqual(diag.patterns.unchangedIdentifiers.length, 0);
    assert.strictEqual(diag.patterns.lowestCoverageFunctions.length, 0);
    assert.ok(diag.timestamp);
  });

  it("preserves coverage data", () => {
    const coverage: CoverageSummary = {
      functions: { total: 10, renamed: 8, library: 0, noMinifiedIds: 2 },
      moduleBindings: { total: 5, renamed: 3, skipped: 2 },
      identifiers: {
        total: 100, renamed: 80, notMinified: 0, skippedByHeuristic: 5,
        llmMissing: 10, llmCollision: 3, llmInvalid: 2, llmUnchanged: 0,
      },
    };

    const diag = buildDiagnosticsReport([], coverage);

    assert.deepStrictEqual(diag.coverage, coverage);
  });
});
