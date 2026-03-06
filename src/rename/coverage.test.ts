import { describe, it } from "node:test";
import assert from "node:assert";
import { buildCoverageSummary, formatCoverageSummary, type CoverageSummary } from "./coverage.js";
import type { FunctionRenameReport } from "../analysis/types.js";

describe("buildCoverageSummary", () => {
  it("aggregates function reports", () => {
    const reports: FunctionRenameReport[] = [
      {
        functionId: "fn:1:0",
        totalIdentifiers: 5,
        renamedCount: 3,
        outcomes: {
          a: { status: "renamed", newName: "counter", round: 1 },
          b: { status: "renamed", newName: "value", round: 1 },
          c: { status: "renamed", newName: "index", round: 2 },
          d: { status: "missing", rounds: 2 },
          e: { status: "duplicate", conflictedWith: "value", rounds: 2 },
        },
        rounds: 2,
        finishReasons: ["stop", "stop"],
      },
      {
        functionId: "fn:5:0",
        totalIdentifiers: 2,
        renamedCount: 2,
        outcomes: {
          x: { status: "renamed", newName: "result", round: 1 },
          y: { status: "renamed", newName: "options", round: 1 },
        },
        rounds: 1,
        finishReasons: ["stop"],
      },
    ];

    const summary = buildCoverageSummary(reports, 10, 0);

    assert.strictEqual(summary.functions.total, 10);
    assert.strictEqual(summary.functions.renamed, 2); // Both reports have renamedCount > 0
    assert.strictEqual(summary.functions.skipped, 8); // 10 - 2 with reports

    assert.strictEqual(summary.identifiers.total, 7); // 5 + 2
    assert.strictEqual(summary.identifiers.renamed, 5); // 3 + 2
    assert.strictEqual(summary.identifiers.llmMissing, 1);
    assert.strictEqual(summary.identifiers.llmCollision, 1);
    assert.strictEqual(summary.identifiers.llmInvalid, 0);
  });

  it("aggregates module binding reports", () => {
    const reports: FunctionRenameReport[] = [
      {
        functionId: "module-binding-batch:a,b,c",
        totalIdentifiers: 3,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "config", round: 1 },
          b: { status: "renamed", newName: "store", round: 1 },
          c: { status: "invalid", rounds: 2 },
        },
        rounds: 2,
        finishReasons: ["stop", "stop"],
      },
    ];

    const summary = buildCoverageSummary(reports, 5, 1);

    assert.strictEqual(summary.moduleBindings.total, 1);
    assert.strictEqual(summary.moduleBindings.renamed, 1);
    assert.strictEqual(summary.identifiers.llmInvalid, 1);
  });

  it("tracks unchanged outcomes", () => {
    const reports: FunctionRenameReport[] = [
      {
        functionId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 1,
        outcomes: {
          a: { status: "renamed", newName: "counter", round: 1 },
          b: { status: "unchanged", rounds: 2 },
          c: { status: "unchanged", rounds: 2 },
        },
        rounds: 2,
        finishReasons: ["stop", "stop"],
      },
    ];

    const summary = buildCoverageSummary(reports, 5, 0);
    assert.strictEqual(summary.identifiers.llmUnchanged, 2);
    assert.strictEqual(summary.identifiers.renamed, 1);
  });

  it("handles empty reports", () => {
    const summary = buildCoverageSummary([], 0, 0);

    assert.strictEqual(summary.functions.total, 0);
    assert.strictEqual(summary.functions.renamed, 0);
    assert.strictEqual(summary.identifiers.total, 0);
    assert.strictEqual(summary.identifiers.renamed, 0);
  });
});

describe("formatCoverageSummary", () => {
  it("formats a summary with all sections", () => {
    const summary = {
      functions: { total: 100, renamed: 80, skipped: 20 },
      moduleBindings: { total: 20, renamed: 15, skipped: 5 },
      identifiers: {
        total: 500,
        renamed: 400,
        notMinified: 50,
        skippedByHeuristic: 0,
        llmMissing: 30,
        llmCollision: 15,
        llmInvalid: 5,
        llmUnchanged: 0,
      },
    };

    const output = formatCoverageSummary(summary);

    assert.ok(output.includes("Coverage Summary"), "Should include header");
    assert.ok(output.includes("Functions:"), "Should include functions");
    assert.ok(output.includes("Module bindings:"), "Should include module bindings");
    assert.ok(output.includes("Identifiers:"), "Should include identifiers");
    assert.ok(output.includes("LLM missing:"), "Should include missing");
    assert.ok(output.includes("LLM collision:"), "Should include collision");
    assert.ok(output.includes("LLM invalid:"), "Should include invalid");
  });

  it("omits zero-count breakdown lines", () => {
    const summary = {
      functions: { total: 10, renamed: 10, skipped: 0 },
      moduleBindings: { total: 0, renamed: 0, skipped: 0 },
      identifiers: {
        total: 50,
        renamed: 50,
        notMinified: 0,
        skippedByHeuristic: 0,
        llmMissing: 0,
        llmCollision: 0,
        llmInvalid: 0,
        llmUnchanged: 0,
      },
    };

    const output = formatCoverageSummary(summary);

    assert.ok(!output.includes("Module bindings:"), "Should omit module bindings when total is 0");
    assert.ok(!output.includes("LLM missing:"), "Should omit missing when 0");
    assert.ok(!output.includes("LLM collision:"), "Should omit collision when 0");
    assert.ok(!output.includes("LLM invalid:"), "Should omit invalid when 0");
  });

  it("includes unchanged and LLM stats in output", () => {
    const summary: CoverageSummary = {
      functions: { total: 100, renamed: 80, skipped: 20 },
      moduleBindings: { total: 0, renamed: 0, skipped: 0 },
      identifiers: {
        total: 500,
        renamed: 400,
        notMinified: 0,
        skippedByHeuristic: 0,
        llmMissing: 30,
        llmCollision: 15,
        llmInvalid: 5,
        llmUnchanged: 50,
      },
      llm: {
        totalCalls: 120,
        retries: 5,
        avgResponseTimeMs: 300,
        totalTokens: 2400000,
        inputTokens: 1800000,
        outputTokens: 600000,
      },
      elapsedMs: 8040000,
    };

    const output = formatCoverageSummary(summary);
    assert.ok(output.includes("LLM unchanged:"), "Should include unchanged line");
    assert.ok(output.includes("120 calls"), "Should include LLM call count");
    assert.ok(output.includes("5 retries"), "Should include retries");
    assert.ok(output.includes("input"), "Should include token breakdown");
    assert.ok(output.includes("output"), "Should include token breakdown");
    assert.ok(output.includes("elapsed"), "Should include elapsed time");
  });

  it("calculates percentages correctly", () => {
    const summary = {
      functions: { total: 200, renamed: 100, skipped: 100 },
      moduleBindings: { total: 0, renamed: 0, skipped: 0 },
      identifiers: {
        total: 1000,
        renamed: 750,
        notMinified: 0,
        skippedByHeuristic: 0,
        llmMissing: 250,
        llmCollision: 0,
        llmInvalid: 0,
        llmUnchanged: 0,
      },
    };

    const output = formatCoverageSummary(summary);
    assert.ok(output.includes("50.0%"), "Should show 50% for functions");
    assert.ok(output.includes("75.0%"), "Should show 75% for identifiers");
  });
});
