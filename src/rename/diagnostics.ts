/**
 * Detailed rename diagnostics report.
 *
 * Produces a JSON file with per-identifier outcomes and pattern analysis
 * to help understand why identifiers weren't renamed.
 */

import fs from "node:fs";
import type { RenameAttempt, RenameReport } from "../analysis/types.js";
import type { CoverageSummary } from "./coverage.js";
import type { StrategyTrailReport } from "./strategy-trail.js";

interface UnrenamedEntry {
  name: string;
  functionId: string;
  /** How the target was named: llm / library-prefix / fallback. */
  strategy?: RenameReport["strategy"];
  /** Structural hash of the enclosing function (function renames only). */
  structuralHash?: string;
  suggestion?: string;
  reason: string;
  attempts: number;
  detail?: string;
  /** Per-round attempt history behind this terminal outcome. */
  trail?: RenameAttempt[];
}

interface RenamedEntry {
  name: string;
  newName: string;
  functionId: string;
  strategy?: RenameReport["strategy"];
  structuralHash?: string;
  round: number;
  /** Per-round attempt history (collisions/rejections before success). */
  trail?: RenameAttempt[];
}

export interface TransferStatsEntry {
  attempted: number;
  applied: number;
  skipped: number;
  /** Skip counts broken down by validation rejection reason */
  rejected?: Partial<Record<string, number>>;
}

export interface ThirdPartyClassificationReport {
  bundler: "bun-cjs";
  factoriesDetected: number;
  bindingsSkipped: number;
  functionsSkipped: number;
  namedBy: {
    banner: number;
    url: number;
    carryOver: number;
    llm: number;
    fallback: number;
  };
}

interface DiagnosticsReport {
  timestamp: string;
  coverage: CoverageSummary;
  transferStats?: {
    exactMatch: TransferStatsEntry;
    closeMatch: TransferStatsEntry;
    statementTwin?: TransferStatsEntry;
  };
  thirdPartyClassification?: ThirdPartyClassificationReport;
  unrenamed: {
    unchanged: UnrenamedEntry[];
    missing: UnrenamedEntry[];
    duplicate: UnrenamedEntry[];
    invalid: UnrenamedEntry[];
  };
  renamed: RenamedEntry[];
  /**
   * Per-identifier transfer-tier attempt trails (statement-twin, exact/
   * close match, cascade, votes, pins, retry) with a per-strategy funnel
   * rollup. Identifiers with no applied entry fell through to the LLM —
   * their endgame is in `renamed`/`unrenamed` above.
   */
  strategyTrails?: StrategyTrailReport;
  patterns: {
    topCollisionTargets: Array<{ name: string; count: number }>;
    unchangedIdentifiers: string[];
    lowestCoverageFunctions: Array<{
      functionId: string;
      total: number;
      renamed: number;
      pct: number;
    }>;
    failuresByAttempts: Record<number, number>;
    missingByFinishReason: Record<string, number>;
  };
}

export function buildDiagnosticsReport(
  reports: ReadonlyArray<RenameReport>,
  coverage: CoverageSummary,
  transferStats?: {
    exactMatch: TransferStatsEntry;
    closeMatch: TransferStatsEntry;
    statementTwin?: TransferStatsEntry;
  },
  thirdPartyClassification?: ThirdPartyClassificationReport,
  strategyTrails?: StrategyTrailReport
): DiagnosticsReport {
  const unchanged: UnrenamedEntry[] = [];
  const missing: UnrenamedEntry[] = [];
  const duplicate: UnrenamedEntry[] = [];
  const invalid: UnrenamedEntry[] = [];
  const renamed: RenamedEntry[] = [];

  // Pattern tracking
  const collisionTargets = new Map<string, number>();
  const unchangedNames: string[] = [];
  const failuresByAttempts = new Map<number, number>();
  const missingByFinishReason = new Map<string, number>();

  for (const report of reports) {
    for (const [name, outcome] of Object.entries(report.outcomes)) {
      // Provenance shared by every entry: how/where the rename was decided
      // and the per-round trail that led to the terminal status.
      const provenance = {
        functionId: report.targetId,
        strategy: report.strategy,
        structuralHash: report.structuralHash,
        trail: outcome.trail
      };
      switch (outcome.status) {
        case "renamed":
          renamed.push({
            name,
            newName: outcome.newName,
            round: outcome.round,
            ...provenance
          });
          break;

        case "unchanged":
          unchanged.push({
            name,
            suggestion: outcome.suggestion,
            reason: "LLM returned original name",
            attempts: outcome.attempts,
            ...provenance
          });
          unchangedNames.push(name);
          bumpCount(failuresByAttempts, outcome.attempts);
          break;

        case "missing": {
          missing.push({
            name,
            reason: "LLM did not return this identifier",
            attempts: outcome.attempts,
            detail: outcome.lastFinishReason
              ? `finish_reason: ${outcome.lastFinishReason}`
              : undefined,
            ...provenance
          });
          bumpCount(failuresByAttempts, outcome.attempts);
          const fr = outcome.lastFinishReason ?? "unknown";
          missingByFinishReason.set(
            fr,
            (missingByFinishReason.get(fr) ?? 0) + 1
          );
          break;
        }

        case "duplicate": {
          const target = outcome.conflictedWith;
          duplicate.push({
            name,
            suggestion: outcome.suggestion,
            reason: "Name collision unresolved",
            attempts: outcome.attempts,
            detail: `conflicted with: ${target}`,
            ...provenance
          });
          collisionTargets.set(target, (collisionTargets.get(target) ?? 0) + 1);
          bumpCount(failuresByAttempts, outcome.attempts);
          break;
        }

        case "invalid":
          invalid.push({
            name,
            suggestion: outcome.suggestion,
            reason: "Invalid identifier returned",
            attempts: outcome.attempts,
            ...provenance
          });
          bumpCount(failuresByAttempts, outcome.attempts);
          break;
      }
    }
  }

  // Build pattern analysis
  const topCollisionTargets = [...collisionTargets.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 20)
    .map(([name, count]) => ({ name, count }));

  const lowestCoverageFunctions = reports
    .filter((r) => r.totalIdentifiers > 0)
    .map((r) => ({
      functionId: r.targetId,
      total: r.totalIdentifiers,
      renamed: r.renamedCount,
      pct: Math.round((r.renamedCount / r.totalIdentifiers) * 100)
    }))
    .sort((a, b) => a.pct - b.pct)
    .slice(0, 20);

  const attemptsRecord: Record<number, number> = {};
  for (const [attempts, count] of failuresByAttempts) {
    attemptsRecord[attempts] = count;
  }

  const finishReasonRecord: Record<string, number> = {};
  for (const [reason, count] of missingByFinishReason) {
    finishReasonRecord[reason] = count;
  }

  return {
    timestamp: new Date().toISOString(),
    coverage,
    transferStats,
    thirdPartyClassification,
    unrenamed: { unchanged, missing, duplicate, invalid },
    renamed,
    strategyTrails,
    patterns: {
      topCollisionTargets,
      unchangedIdentifiers: unchangedNames,
      lowestCoverageFunctions,
      failuresByAttempts: attemptsRecord,
      missingByFinishReason: finishReasonRecord
    }
  };
}

function bumpCount(map: Map<number, number>, count: number): void {
  map.set(count, (map.get(count) ?? 0) + 1);
}

export function writeDiagnosticsFile(
  report: DiagnosticsReport,
  path: string
): void {
  fs.writeFileSync(path, `${JSON.stringify(report, null, 2)}\n`);
}
