import assert from "node:assert";
import { describe, it } from "node:test";
import type { RenameReport } from "../analysis/types.js";
import {
  buildCoverageSummary,
  type CoverageSummary,
  formatCoverageSummary
} from "./coverage.js";

describe("buildCoverageSummary", () => {
  it("aggregates function reports by type field", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:1:0",
        totalIdentifiers: 5,
        renamedCount: 3,
        outcomes: {
          a: { status: "renamed", newName: "counter", round: 1 },
          b: { status: "renamed", newName: "value", round: 1 },
          c: { status: "renamed", newName: "index", round: 2 },
          d: { status: "missing", attempts: 2 },
          e: { status: "duplicate", conflictedWith: "value", attempts: 2 }
        },
        totalLLMCalls: 2,
        finishReasons: ["stop", "stop"]
      },
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:5:0",
        totalIdentifiers: 2,
        renamedCount: 2,
        outcomes: {
          x: { status: "renamed", newName: "result", round: 1 },
          y: { status: "renamed", newName: "options", round: 1 }
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"]
      }
    ];

    const summary = buildCoverageSummary(reports, 10);

    assert.strictEqual(summary.functions.total, 10);
    assert.strictEqual(summary.functions.llm, 2);
    assert.strictEqual(summary.functions.libraryPrefix, 0);
    assert.strictEqual(summary.functions.notRenamed, 8); // 10 - 2 with reports

    assert.strictEqual(summary.identifiers.total, 7); // 5 + 2
    assert.strictEqual(summary.identifiers.llm, 5); // 3 + 2 renamed via LLM
    assert.strictEqual(summary.identifiers.notRenamed, 2); // missing + duplicate
  });

  it("aggregates module binding reports by type field", () => {
    const reports: RenameReport[] = [
      {
        type: "module-binding",
        strategy: "llm",
        targetId: "module-binding-batch:a,b,c",
        totalIdentifiers: 3,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "config", round: 1 },
          b: { status: "renamed", newName: "store", round: 1 },
          c: { status: "invalid", attempts: 2 }
        },
        totalLLMCalls: 2,
        finishReasons: ["stop", "stop"]
      }
    ];

    const summary = buildCoverageSummary(reports, 5);

    // Module binding counts are per-identifier, not per-batch
    assert.strictEqual(summary.moduleBindings.total, 3);
    assert.strictEqual(summary.moduleBindings.llm, 2);
    assert.strictEqual(summary.moduleBindings.notRenamed, 1);
  });

  it("tracks unchanged outcomes", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 1,
        outcomes: {
          a: { status: "renamed", newName: "counter", round: 1 },
          b: { status: "unchanged", attempts: 2 },
          c: { status: "unchanged", attempts: 2 }
        },
        totalLLMCalls: 2,
        finishReasons: ["stop", "stop"]
      }
    ];

    const summary = buildCoverageSummary(reports, 5);
    assert.strictEqual(summary.identifiers.notRenamed, 2);
    assert.strictEqual(summary.identifiers.llm, 1);
  });

  it("counts module bindings per-identifier across multiple batches", () => {
    const reports: RenameReport[] = [
      {
        type: "module-binding",
        strategy: "llm",
        targetId: "module-binding-batch:a,b,c",
        totalIdentifiers: 3,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "config", round: 1 },
          b: { status: "renamed", newName: "store", round: 1 },
          c: { status: "missing", attempts: 2 }
        },
        totalLLMCalls: 2,
        finishReasons: ["stop", "stop"]
      },
      {
        type: "module-binding",
        strategy: "llm",
        targetId: "module-binding-batch:d,e",
        totalIdentifiers: 2,
        renamedCount: 1,
        outcomes: {
          d: { status: "renamed", newName: "handler", round: 1 },
          e: { status: "unchanged", attempts: 1 }
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"]
      }
    ];

    const summary = buildCoverageSummary(reports, 0);

    // Should count individual bindings, not batch reports
    assert.strictEqual(summary.moduleBindings.total, 5);
    assert.strictEqual(summary.moduleBindings.llm, 3);
    assert.strictEqual(summary.moduleBindings.notRenamed, 2); // 5 - 3
  });

  it("handles empty reports", () => {
    const summary = buildCoverageSummary([], 0);

    assert.strictEqual(summary.functions.total, 0);
    assert.strictEqual(summary.functions.llm, 0);
    assert.strictEqual(summary.identifiers.total, 0);
    assert.strictEqual(summary.identifiers.llm, 0);
  });

  it("counts library-prefix strategy separately from LLM", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "counter", round: 1 },
          b: { status: "renamed", newName: "value", round: 1 },
          c: { status: "missing", attempts: 2 }
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"]
      },
      {
        type: "function",
        strategy: "library-prefix",
        targetId: "fn:2:0",
        totalIdentifiers: 4,
        renamedCount: 4,
        outcomes: {
          x: { status: "renamed", newName: "react_dom_x", round: 1 },
          y: { status: "renamed", newName: "react_dom_y", round: 1 },
          z: { status: "renamed", newName: "react_dom_z", round: 1 },
          w: { status: "renamed", newName: "react_dom_w", round: 1 }
        }
      }
    ];

    // 100 total functions
    const summary = buildCoverageSummary(reports, 100);

    assert.strictEqual(summary.functions.total, 100);
    assert.strictEqual(summary.functions.llm, 1);
    assert.strictEqual(summary.functions.libraryPrefix, 1);
    assert.strictEqual(summary.functions.notRenamed, 98); // 100 - 1 llm - 1 library-prefix

    // Identifiers: 2 LLM renamed + 4 library-prefix renamed
    assert.strictEqual(summary.identifiers.total, 7);
    assert.strictEqual(summary.identifiers.llm, 2);
    assert.strictEqual(summary.identifiers.libraryPrefix, 4);
    assert.strictEqual(summary.identifiers.notRenamed, 1); // 1 missing
  });

  it("does not inflate app function count with library-prefix reports (236% bug)", () => {
    // This reproduces the bug: library param reports were counted as app functions,
    // inflating the "App (renamed)" percentage to 236%
    const reports: RenameReport[] = [
      // 15,292 app functions processed by LLM (simplified to 2 reports)
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:app:1",
        totalIdentifiers: 5,
        renamedCount: 5,
        outcomes: {
          a: { status: "renamed", newName: "init", round: 1 },
          b: { status: "renamed", newName: "render", round: 1 },
          c: { status: "renamed", newName: "update", round: 1 },
          d: { status: "renamed", newName: "cleanup", round: 1 },
          e: { status: "renamed", newName: "config", round: 1 }
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"]
      },
      // 39,708 library functions processed by library-prefix
      {
        type: "function",
        strategy: "library-prefix",
        targetId: "fn:lib:1",
        totalIdentifiers: 3,
        renamedCount: 3,
        outcomes: {
          x: { status: "renamed", newName: "react_dom_x", round: 1 },
          y: { status: "renamed", newName: "react_dom_y", round: 1 },
          z: { status: "renamed", newName: "react_dom_z", round: 1 }
        }
      }
    ];

    const summary = buildCoverageSummary(reports, 55000);

    // LLM should only count 1 function, not be inflated by library-prefix reports
    assert.strictEqual(summary.functions.llm, 1);
    assert.strictEqual(summary.functions.libraryPrefix, 1);
    // Total should be correct
    assert.strictEqual(summary.functions.total, 55000);
  });

  it("counts fallback strategy separately", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "fallback",
        targetId: "fn:1:0",
        totalIdentifiers: 2,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "a_1", round: 1 },
          b: { status: "renamed", newName: "b_1", round: 1 }
        }
      }
    ];

    const summary = buildCoverageSummary(reports, 5);

    assert.strictEqual(summary.functions.fallback, 1);
    assert.strictEqual(summary.identifiers.fallback, 2);
  });
  it("splits notRenamed into nothingToRename and failed with skip reasons", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "counter", round: 1 },
          b: { status: "renamed", newName: "value", round: 1 },
          c: { status: "missing", attempts: 2 }
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"]
      }
    ];

    const skipReasons = { zeroBindings: 5, allPreserved: 3, error: 1 };
    const summary = buildCoverageSummary(
      reports,
      20,
      undefined,
      undefined,
      skipReasons
    );

    // 20 total - 1 llm = 19 notRenamed
    assert.strictEqual(summary.functions.notRenamed, 19);
    // nothingToRename = zeroBindings + allPreserved = 8
    assert.strictEqual(summary.functions.nothingToRename, 8);
    // failed = notRenamed - nothingToRename = 11
    assert.strictEqual(summary.functions.failed, 11);
  });

  it("includes libraryNoMinified in nothingToRename", () => {
    const reports: RenameReport[] = [];
    const skipReasons = { zeroBindings: 10, allPreserved: 5, error: 0 };
    const summary = buildCoverageSummary(
      reports,
      20,
      undefined,
      undefined,
      skipReasons,
      3
    );

    // nothingToRename = 10 + 5 + 3 = 18
    assert.strictEqual(summary.functions.nothingToRename, 18);
    // notRenamed = 20 total, failed = 20 - 18 = 2
    assert.strictEqual(summary.functions.failed, 2);
  });

  it("defaults skip reasons to zero when not provided", () => {
    const reports: RenameReport[] = [];
    const summary = buildCoverageSummary(reports, 10);

    assert.strictEqual(summary.functions.nothingToRename, 0);
    assert.strictEqual(summary.functions.failed, 10);
  });
});

describe("formatCoverageSummary", () => {
  it("formats a summary with all sections", () => {
    const summary: CoverageSummary = {
      functions: {
        total: 100,
        llm: 80,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 20,
        nothingToRename: 0,
        failed: 0
      },
      moduleBindings: {
        total: 20,
        llm: 15,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 5,
        nothingToRename: 0,
        failed: 0
      },
      identifiers: {
        total: 500,
        llm: 400,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 100,
        nothingToRename: 0,
        failed: 0,
        skippedBySkipList: 0
      }
    };

    const output = formatCoverageSummary(summary);

    assert.ok(output.includes("Coverage Summary"), "Should include header");
    assert.ok(output.includes("Functions:"), "Should include functions");
    assert.ok(
      output.includes("Module bindings:"),
      "Should include module bindings"
    );
    assert.ok(output.includes("Identifiers:"), "Should include identifiers");
    assert.ok(output.includes("LLM:"), "Should include LLM line");
  });

  it("omits zero-count breakdown lines", () => {
    const summary: CoverageSummary = {
      functions: {
        total: 10,
        llm: 10,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0
      },
      moduleBindings: {
        total: 0,
        llm: 0,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0
      },
      identifiers: {
        total: 50,
        llm: 50,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0,
        skippedBySkipList: 0
      }
    };

    const output = formatCoverageSummary(summary);

    assert.ok(
      !output.includes("Module bindings:"),
      "Should omit module bindings when total is 0"
    );
    assert.ok(
      !output.includes("Library prefix:"),
      "Should omit library prefix when 0"
    );
    assert.ok(!output.includes("Fallback:"), "Should omit fallback when 0");
    assert.ok(
      !output.includes("Not renamed:"),
      "Should omit not-renamed when 0"
    );
  });

  it("includes LLM stats in output", () => {
    const summary: CoverageSummary = {
      functions: {
        total: 100,
        llm: 80,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 20,
        nothingToRename: 0,
        failed: 0
      },
      moduleBindings: {
        total: 0,
        llm: 0,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0
      },
      identifiers: {
        total: 500,
        llm: 400,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 100,
        nothingToRename: 0,
        failed: 0,
        skippedBySkipList: 0
      },
      llm: {
        totalCalls: 120,
        retries: 5,
        avgResponseTimeMs: 300,
        totalTokens: 2400000,
        inputTokens: 1800000,
        outputTokens: 600000
      },
      elapsedMs: 8040000
    };

    const output = formatCoverageSummary(summary);
    assert.ok(output.includes("120 calls"), "Should include LLM call count");
    assert.ok(output.includes("5 retries"), "Should include retries");
    assert.ok(output.includes("input"), "Should include token breakdown");
    assert.ok(output.includes("output"), "Should include token breakdown");
    assert.ok(output.includes("elapsed"), "Should include elapsed time");
  });

  it("shows library prefix breakdown when present", () => {
    const summary: CoverageSummary = {
      functions: {
        total: 55000,
        llm: 14000,
        libraryPrefix: 39708,
        fallback: 0,
        notRenamed: 1292,
        nothingToRename: 0,
        failed: 0
      },
      moduleBindings: {
        total: 0,
        llm: 0,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0
      },
      identifiers: {
        total: 86190,
        llm: 72000,
        libraryPrefix: 12042,
        fallback: 2148,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0,
        skippedBySkipList: 0
      }
    };

    const output = formatCoverageSummary(summary);
    assert.ok(
      output.includes("Library prefix:"),
      "Should show library prefix line"
    );
    assert.ok(output.includes("39,708"), "Should show library function count");
  });

  it("shows 'Nothing to rename' instead of 'Not renamed' for functions", () => {
    const summary: CoverageSummary = {
      functions: {
        total: 100,
        llm: 80,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 20,
        nothingToRename: 18,
        failed: 2
      },
      moduleBindings: {
        total: 0,
        llm: 0,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0
      },
      identifiers: {
        total: 500,
        llm: 400,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0,
        skippedBySkipList: 0
      }
    };

    const output = formatCoverageSummary(summary);

    assert.ok(
      output.includes("Nothing to rename:"),
      "Should show 'Nothing to rename' for functions"
    );
    assert.ok(
      output.includes("Failed:"),
      "Should show 'Failed' for functions when > 0"
    );
    assert.ok(
      !output.includes("Not renamed:"),
      "Should NOT show 'Not renamed' when nothingToRename/failed are set"
    );
  });

  it("omits Failed line when failed is 0", () => {
    const summary: CoverageSummary = {
      functions: {
        total: 100,
        llm: 80,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 20,
        nothingToRename: 20,
        failed: 0
      },
      moduleBindings: {
        total: 0,
        llm: 0,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0
      },
      identifiers: {
        total: 0,
        llm: 0,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0,
        skippedBySkipList: 0
      }
    };

    const output = formatCoverageSummary(summary);

    assert.ok(
      output.includes("Nothing to rename:"),
      "Should show 'Nothing to rename'"
    );
    assert.ok(!output.includes("Failed:"), "Should omit 'Failed' when 0");
  });

  it("calculates percentages correctly", () => {
    const summary: CoverageSummary = {
      functions: {
        total: 200,
        llm: 100,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 100,
        nothingToRename: 0,
        failed: 0
      },
      moduleBindings: {
        total: 0,
        llm: 0,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 0,
        nothingToRename: 0,
        failed: 0
      },
      identifiers: {
        total: 1000,
        llm: 750,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 250,
        nothingToRename: 0,
        failed: 0,
        skippedBySkipList: 0
      }
    };

    const output = formatCoverageSummary(summary);
    assert.ok(
      output.includes("50.0%"),
      "Should show 50% for LLM functions (100/200)"
    );
    assert.ok(output.includes("75.0%"), "Should show 75% for LLM identifiers");
  });
});
