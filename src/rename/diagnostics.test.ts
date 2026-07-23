import assert from "node:assert";
import { describe, it } from "node:test";
import type { RenameReport } from "../analysis/types.js";
import type { CoverageSummary } from "./coverage.js";
import { buildDiagnosticsReport } from "./diagnostics.js";

const emptyCoverage: CoverageSummary = {
  functions: {
    total: 0,
    llm: 0,
    libraryPrefix: 0,
    fallback: 0,
    notRenamed: 0,
    nothingToRename: 0,
    cached: 0,
    closeMatch: 0,
    alreadyNamed: 0,
    failed: 0
  },
  moduleBindings: {
    total: 0,
    llm: 0,
    libraryPrefix: 0,
    fallback: 0,
    notRenamed: 0,
    nothingToRename: 0,
    cached: 0,
    closeMatch: 0,
    alreadyNamed: 0,
    failed: 0
  },
  identifiers: {
    total: 0,
    llm: 0,
    libraryPrefix: 0,
    fallback: 0,
    notRenamed: 0,
    nothingToRename: 0,
    cached: 0,
    closeMatch: 0,
    alreadyNamed: 0,
    failed: 0,
    skippedBySkipList: 0
  }
};

describe("buildDiagnosticsReport", () => {
  it("categorizes outcomes correctly", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:1:0",
        totalIdentifiers: 6,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "counter", round: 1 },
          b: { status: "renamed", newName: "value", round: 2 },
          c: { status: "unchanged", attempts: 2, suggestion: "c" },
          d: { status: "missing", attempts: 2, lastFinishReason: "length" },
          e: {
            status: "duplicate",
            conflictedWith: "data",
            attempts: 2,
            suggestion: "data"
          },
          f: { status: "invalid", attempts: 1, suggestion: "123bad" }
        },
        totalLLMCalls: 2,
        finishReasons: ["stop", "stop"]
      }
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    assert.strictEqual(diag.renamed.length, 2);
    assert.strictEqual(diag.unrenamed.unchanged.length, 1);
    assert.strictEqual(diag.unrenamed.missing.length, 1);
    assert.strictEqual(diag.unrenamed.duplicate.length, 1);
    assert.strictEqual(diag.unrenamed.invalid.length, 1);

    // Check renamed entries (now carry provenance fields too)
    assert.deepStrictEqual(diag.renamed[0], {
      name: "a",
      newName: "counter",
      round: 1,
      functionId: "fn:1:0",
      strategy: "llm",
      structuralHash: undefined,
      trail: undefined
    });

    // Check suggestion is preserved
    assert.strictEqual(diag.unrenamed.unchanged[0].suggestion, "c");
    assert.strictEqual(diag.unrenamed.duplicate[0].suggestion, "data");
    assert.strictEqual(diag.unrenamed.invalid[0].suggestion, "123bad");

    // Check detail for missing
    assert.strictEqual(
      diag.unrenamed.missing[0].detail,
      "finish_reason: length"
    );
  });

  it("surfaces provenance: strategy, structuralHash, and the attempt trail", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:1:0",
        structuralHash: "abc123",
        totalIdentifiers: 2,
        renamedCount: 1,
        outcomes: {
          // succeeded on round 2 after a first-round collision
          a: {
            status: "renamed",
            newName: "counter",
            round: 2,
            trail: [
              { round: 1, proposed: "data", result: "duplicate" },
              { round: 2, proposed: "counter", result: "applied" }
            ]
          },
          // never resolved — two collisions
          b: {
            status: "duplicate",
            conflictedWith: "value",
            attempts: 2,
            suggestion: "value",
            trail: [
              { round: 1, proposed: "value", result: "duplicate" },
              { round: 2, proposed: "value", result: "duplicate" }
            ]
          }
        },
        totalLLMCalls: 2,
        finishReasons: ["stop", "stop"]
      }
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    const a = diag.renamed[0];
    assert.strictEqual(a.strategy, "llm");
    assert.strictEqual(a.structuralHash, "abc123");
    assert.deepStrictEqual(a.trail, [
      { round: 1, proposed: "data", result: "duplicate" },
      { round: 2, proposed: "counter", result: "applied" }
    ]);

    const b = diag.unrenamed.duplicate[0];
    assert.strictEqual(b.structuralHash, "abc123");
    assert.strictEqual(b.trail?.length, 2);
    assert.strictEqual(b.trail?.[0].result, "duplicate");
  });

  it("computes top collision targets", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 0,
        outcomes: {
          a: {
            status: "duplicate",
            conflictedWith: "data",
            attempts: 1,
            suggestion: "data"
          },
          b: {
            status: "duplicate",
            conflictedWith: "data",
            attempts: 1,
            suggestion: "data"
          },
          c: {
            status: "duplicate",
            conflictedWith: "value",
            attempts: 1,
            suggestion: "value"
          }
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"]
      }
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    assert.strictEqual(diag.patterns.topCollisionTargets.length, 2);
    assert.strictEqual(diag.patterns.topCollisionTargets[0].name, "data");
    assert.strictEqual(diag.patterns.topCollisionTargets[0].count, 2);
    assert.strictEqual(diag.patterns.topCollisionTargets[1].name, "value");
    assert.strictEqual(diag.patterns.topCollisionTargets[1].count, 1);
  });

  it("computes lowest coverage functions", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:1:0",
        totalIdentifiers: 10,
        renamedCount: 2,
        outcomes: {
          a: { status: "renamed", newName: "x", round: 1 },
          b: { status: "renamed", newName: "y", round: 1 }
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"]
      },
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:5:0",
        totalIdentifiers: 4,
        renamedCount: 4,
        outcomes: {
          c: { status: "renamed", newName: "w", round: 1 },
          d: { status: "renamed", newName: "z", round: 1 },
          e: { status: "renamed", newName: "q", round: 1 },
          f: { status: "renamed", newName: "r", round: 1 }
        },
        totalLLMCalls: 1,
        finishReasons: ["stop"]
      }
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    assert.strictEqual(
      diag.patterns.lowestCoverageFunctions[0].functionId,
      "fn:1:0"
    );
    assert.strictEqual(diag.patterns.lowestCoverageFunctions[0].pct, 20);
  });

  it("tracks failure attempt distribution", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 0,
        outcomes: {
          a: { status: "missing", attempts: 1 },
          b: { status: "missing", attempts: 3 },
          c: { status: "unchanged", attempts: 1 }
        },
        totalLLMCalls: 3,
        finishReasons: ["stop", "stop", "stop"]
      }
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    assert.strictEqual(diag.patterns.failuresByAttempts[1], 2); // a + c
    assert.strictEqual(diag.patterns.failuresByAttempts[3], 1); // b
  });

  it("tracks missing by finish reason", () => {
    const reports: RenameReport[] = [
      {
        type: "function",
        strategy: "llm",
        targetId: "fn:1:0",
        totalIdentifiers: 3,
        renamedCount: 0,
        outcomes: {
          a: { status: "missing", attempts: 1, lastFinishReason: "length" },
          b: { status: "missing", attempts: 1, lastFinishReason: "length" },
          c: { status: "missing", attempts: 1 }
        },
        totalLLMCalls: 1,
        finishReasons: ["length"]
      }
    ];

    const diag = buildDiagnosticsReport(reports, emptyCoverage);

    assert.strictEqual(diag.patterns.missingByFinishReason.length, 2);
    assert.strictEqual(diag.patterns.missingByFinishReason.unknown, 1);
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
      functions: {
        total: 10,
        llm: 8,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 2,
        nothingToRename: 0,
        cached: 0,
        closeMatch: 0,
        alreadyNamed: 0,
        failed: 0
      },
      moduleBindings: {
        total: 5,
        llm: 3,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 2,
        nothingToRename: 0,
        cached: 0,
        closeMatch: 0,
        alreadyNamed: 0,
        failed: 0
      },
      identifiers: {
        total: 100,
        llm: 80,
        libraryPrefix: 0,
        fallback: 0,
        notRenamed: 20,
        nothingToRename: 0,
        cached: 0,
        closeMatch: 0,
        alreadyNamed: 0,
        failed: 0,
        skippedBySkipList: 5
      }
    };

    const diag = buildDiagnosticsReport([], coverage);

    assert.deepStrictEqual(diag.coverage, coverage);
  });

  it("includes transfer stats when provided", () => {
    const transferStats = {
      exactMatch: { attempted: 69, applied: 65, skipped: 4 },
      closeMatch: { attempted: 86, applied: 80, skipped: 6 }
    };

    const diag = buildDiagnosticsReport([], emptyCoverage, transferStats);

    assert.deepStrictEqual(diag.transferStats, transferStats);
  });

  it("omits transfer stats when not provided", () => {
    const diag = buildDiagnosticsReport([], emptyCoverage);

    assert.strictEqual(diag.transferStats, undefined);
  });

  it("includes third-party classification report when provided", () => {
    const thirdParty = {
      bundler: "bun-cjs" as const,
      factoriesDetected: 6,
      bindingsSkipped: 42,
      functionsSkipped: 23,
      namedBy: { banner: 0, url: 0, carryOver: 0, llm: 0, fallback: 6 }
    };

    const diag = buildDiagnosticsReport(
      [],
      emptyCoverage,
      undefined,
      thirdParty
    );

    assert.deepStrictEqual(diag.thirdPartyClassification, thirdParty);
  });

  it("omits third-party classification when not provided", () => {
    const diag = buildDiagnosticsReport([], emptyCoverage);

    assert.strictEqual(diag.thirdPartyClassification, undefined);
  });
});

describe("identifier ledger — terminal state", () => {
  it("accounts every census name from bookkeeping and lists the residue", () => {
    const coverage: CoverageSummary = {
      ...emptyCoverage,
      identifiers: { ...emptyCoverage.identifiers, llm: 3 },
      mintedCensus: {
        total: 4,
        decorated: 1,
        totalBindings: 10,
        names: ["_", "h06Result", "zz9Mystery", "T7Class"],
        decoratedNames: ["fsPromises_"],
        byFamily: {
          classExprId: 0,
          fnExprId: 0,
          param: 1,
          fnDecl: 0,
          varOther: 3
        },
        derivableExprIds: 0,
        zeroRefExprIds: 0
      }
    };
    const reports: RenameReport[] = [
      {
        type: "module-binding",
        strategy: "llm",
        targetId: "module-binding-batch:Yde",
        totalIdentifiers: 1,
        renamedCount: 1,
        outcomes: {
          Yde: { status: "renamed", newName: "fsPromises_", round: 3 }
        },
        totalLLMCalls: 3,
        finishReasons: []
      } as unknown as RenameReport
    ];
    const trails = {
      trails: [
        {
          // statement-twin inherited the `_` convention name.
          oldName: "q7",
          loc: "1:0",
          trail: [
            {
              strategy: "statement-twin",
              outcome: "applied" as const,
              newName: "_"
            }
          ],
          settledBy: "statement-twin",
          terminalBy: "statement-twin",
          postSettleAttempts: 0,
          postSettleVotes: 0
        },
        {
          // sweep refused the stem echo — binding KEPT h06Result.
          oldName: "h06Result",
          loc: "2:0",
          trail: [
            {
              strategy: "coverage-sweep",
              outcome: "abstained" as const,
              reason: "still-below-floor"
            }
          ],
          postSettleAttempts: 0,
          postSettleVotes: 0
        },
        {
          // reconcile restored the fossil onto a fresh binding.
          oldName: "iIn",
          loc: "3:0",
          trail: [
            {
              strategy: "reconcile-asymmetric",
              outcome: "applied" as const,
              newName: "T7Class"
            }
          ],
          terminalBy: "reconcile-asymmetric",
          postSettleAttempts: 0,
          postSettleVotes: 0
        }
      ],
      funnel: {}
    };
    const report = buildDiagnosticsReport(
      reports,
      coverage,
      undefined,
      undefined,
      trails
    );
    const ts = report.identifierLedger?.terminalState;
    assert.ok(ts, "terminal state must be present");
    assert.strictEqual(ts.totalBindings, 10);
    assert.strictEqual(ts.namedByTier["statement-twin"], 1);
    assert.strictEqual(ts.namedByTier["reconcile-asymmetric"], 1);
    assert.strictEqual(ts.llmNamed, 3);
    // _, h06Result, T7Class accounted via the trail (fsPromises_ sits in
    // decoratedNames, outside the minted join); zz9Mystery has no
    // bookkeeping trace.
    assert.strictEqual(ts.mintedAccounted, 3);
    assert.deepStrictEqual(ts.mintedUnaccounted, ["zz9Mystery"]);
  });
});
