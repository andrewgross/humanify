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
          d: { status: "missing", attempts: 2 },
          e: { status: "duplicate", conflictedWith: "value", attempts: 2 },
        },
        totalLLMCalls: 2,
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
        totalLLMCalls: 1,
        finishReasons: ["stop"],
      },
    ];

    const summary = buildCoverageSummary(reports, 10, 0);

    assert.strictEqual(summary.functions.total, 10);
    assert.strictEqual(summary.functions.renamed, 2); // Both reports have renamedCount > 0
    assert.strictEqual(summary.functions.library, 0); // No library count passed
    assert.strictEqual(summary.functions.noMinifiedIds, 8); // 10 - 0 library - 2 with reports

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
          c: { status: "invalid", attempts: 2 },
        },
        totalLLMCalls: 2,
        finishReasons: ["stop", "stop"],
      },
    ];

    const summary = buildCoverageSummary(reports, 5, 1);

    // Module binding counts are per-identifier, not per-batch
    assert.strictEqual(summary.moduleBindings.total, 3);
    assert.strictEqual(summary.moduleBindings.renamed, 2);
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
          b: { status: "unchanged", attempts: 2 },
          c: { status: "unchanged", attempts: 2 },
        },
        totalLLMCalls: 2,
        finishReasons: ["stop", "stop"],
      },
    ];

    const summary = buildCoverageSummary(reports, 5, 0);
    assert.strictEqual(summary.identifiers.llmUnchanged, 2);
    assert.strictEqual(summary.identifiers.renamed, 1);
  });

  it("counts module bindings per-identifier across multiple batches", () => {
    const reports: FunctionRenameReport[] = [
      {
        functionId: "module-binding-batch:a,b,c",
        totalIdentifiers: 3,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "config", round: 1 },
          b: { status: "renamed", newName: "store", round: 1 },
          c: { status: "missing", attempts: 2 },
        },
        totalLLMCalls: 2,
        finishReasons: ["stop", "stop"],
      },
      {
        functionId: "module-binding-batch:d,e",
        totalIdentifiers: 2,
        renamedCount: 1,
        outcomes: {
          d: { status: "renamed", newName: "handler", round: 1 },
          e: { status: "unchanged", attempts: 1 },
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"],
      },
    ];

    const summary = buildCoverageSummary(reports, 0, 0);

    // Should count individual bindings, not batch reports
    assert.strictEqual(summary.moduleBindings.total, 5);
    assert.strictEqual(summary.moduleBindings.renamed, 3);
    assert.strictEqual(summary.moduleBindings.skipped, 2); // 5 - 3
  });

  it("handles empty reports", () => {
    const summary = buildCoverageSummary([], 0, 0);

    assert.strictEqual(summary.functions.total, 0);
    assert.strictEqual(summary.functions.renamed, 0);
    assert.strictEqual(summary.identifiers.total, 0);
    assert.strictEqual(summary.identifiers.renamed, 0);
  });

  it("tracks library functions separately", () => {
    const reports: FunctionRenameReport[] = [
      {
        functionId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "counter", round: 1 },
          b: { status: "renamed", newName: "value", round: 1 },
          c: { status: "missing", attempts: 2 },
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"],
      },
    ];

    // 100 total functions, 80 are library, 1 has a report
    const summary = buildCoverageSummary(reports, 100, 0, undefined, 0, 80);

    assert.strictEqual(summary.functions.total, 100);
    assert.strictEqual(summary.functions.library, 80);
    assert.strictEqual(summary.functions.renamed, 1);
    assert.strictEqual(summary.functions.noMinifiedIds, 19); // 100 - 80 library - 1 with report
  });
});

describe("formatCoverageSummary", () => {
  it("formats a summary with all sections", () => {
    const summary = {
      functions: { total: 100, renamed: 80, library: 0, noMinifiedIds: 20 },
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
      functions: { total: 10, renamed: 10, library: 0, noMinifiedIds: 0 },
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
      functions: { total: 100, renamed: 80, library: 0, noMinifiedIds: 20 },
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

  it("shows library function breakdown when present", () => {
    const summary: CoverageSummary = {
      functions: { total: 55000, renamed: 14000, library: 39708, noMinifiedIds: 1292 },
      moduleBindings: { total: 0, renamed: 0, skipped: 0 },
      identifiers: {
        total: 86190,
        renamed: 86164,
        notMinified: 0,
        skippedByHeuristic: 0,
        llmMissing: 26,
        llmCollision: 0,
        llmInvalid: 0,
        llmUnchanged: 0,
      },
    };

    const output = formatCoverageSummary(summary);
    assert.ok(output.includes("Library:"), "Should show library count");
    assert.ok(output.includes("39,708"), "Should show library function count");
    assert.ok(output.includes("skipped"), "Should indicate library functions were skipped");
    assert.ok(output.includes("App (renamed):"), "Should show app renamed count");
    assert.ok(output.includes("no minified ids"), "Should show no-minified-ids count");
    // App count = 55000 - 39708 = 15292
    assert.ok(output.includes("15,292"), "Should show app function count");
  });

  it("calculates percentages correctly", () => {
    const summary = {
      functions: { total: 200, renamed: 100, library: 0, noMinifiedIds: 100 },
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
    assert.ok(output.includes("50.0%"), "Should show 50% for app functions (100/200)");
    assert.ok(output.includes("75.0%"), "Should show 75% for identifiers");
  });
});
