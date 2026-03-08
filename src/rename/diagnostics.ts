/**
 * Detailed rename diagnostics report.
 *
 * Produces a JSON file with per-identifier outcomes and pattern analysis
 * to help understand why identifiers weren't renamed.
 */

import fs from "fs";
import type { FunctionRenameReport, IdentifierOutcome } from "../analysis/types.js";
import type { CoverageSummary } from "./coverage.js";

export interface UnrenamedEntry {
  name: string;
  functionId: string;
  suggestion?: string;
  reason: string;
  rounds: number;
  detail?: string;
}

export interface RenamedEntry {
  name: string;
  newName: string;
  functionId: string;
  round: number;
}

export interface DiagnosticsReport {
  timestamp: string;
  coverage: CoverageSummary;
  unrenamed: {
    unchanged: UnrenamedEntry[];
    missing: UnrenamedEntry[];
    duplicate: UnrenamedEntry[];
    invalid: UnrenamedEntry[];
  };
  renamed: RenamedEntry[];
  patterns: {
    topCollisionTargets: Array<{ name: string; count: number }>;
    unchangedIdentifiers: string[];
    lowestCoverageFunctions: Array<{ functionId: string; total: number; renamed: number; pct: number }>;
    failuresByRound: Record<number, number>;
    missingByFinishReason: Record<string, number>;
  };
}

export function buildDiagnosticsReport(
  reports: ReadonlyArray<FunctionRenameReport>,
  coverage: CoverageSummary
): DiagnosticsReport {
  const unchanged: UnrenamedEntry[] = [];
  const missing: UnrenamedEntry[] = [];
  const duplicate: UnrenamedEntry[] = [];
  const invalid: UnrenamedEntry[] = [];
  const renamed: RenamedEntry[] = [];

  // Pattern tracking
  const collisionTargets = new Map<string, number>();
  const unchangedNames: string[] = [];
  const failuresByRound = new Map<number, number>();
  const missingByFinishReason = new Map<string, number>();

  for (const report of reports) {
    for (const [name, outcome] of Object.entries(report.outcomes)) {
      switch (outcome.status) {
        case "renamed":
          renamed.push({
            name,
            newName: outcome.newName,
            functionId: report.functionId,
            round: outcome.round,
          });
          break;

        case "unchanged":
          unchanged.push({
            name,
            functionId: report.functionId,
            suggestion: outcome.suggestion,
            reason: "LLM returned original name",
            rounds: outcome.rounds,
          });
          unchangedNames.push(name);
          bumpRound(failuresByRound, outcome.rounds);
          break;

        case "missing":
          missing.push({
            name,
            functionId: report.functionId,
            reason: "LLM did not return this identifier",
            rounds: outcome.rounds,
            detail: outcome.lastFinishReason
              ? `finish_reason: ${outcome.lastFinishReason}`
              : undefined,
          });
          bumpRound(failuresByRound, outcome.rounds);
          const fr = outcome.lastFinishReason ?? "unknown";
          missingByFinishReason.set(fr, (missingByFinishReason.get(fr) ?? 0) + 1);
          break;

        case "duplicate": {
          const target = outcome.conflictedWith;
          duplicate.push({
            name,
            functionId: report.functionId,
            suggestion: outcome.suggestion,
            reason: "Name collision unresolved",
            rounds: outcome.rounds,
            detail: `conflicted with: ${target}`,
          });
          collisionTargets.set(target, (collisionTargets.get(target) ?? 0) + 1);
          bumpRound(failuresByRound, outcome.rounds);
          break;
        }

        case "invalid":
          invalid.push({
            name,
            functionId: report.functionId,
            suggestion: outcome.suggestion,
            reason: "Invalid identifier returned",
            rounds: outcome.rounds,
          });
          bumpRound(failuresByRound, outcome.rounds);
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
    .filter(r => r.totalIdentifiers > 0)
    .map(r => ({
      functionId: r.functionId,
      total: r.totalIdentifiers,
      renamed: r.renamedCount,
      pct: Math.round((r.renamedCount / r.totalIdentifiers) * 100),
    }))
    .sort((a, b) => a.pct - b.pct)
    .slice(0, 20);

  const roundsRecord: Record<number, number> = {};
  for (const [round, count] of failuresByRound) {
    roundsRecord[round] = count;
  }

  const finishReasonRecord: Record<string, number> = {};
  for (const [reason, count] of missingByFinishReason) {
    finishReasonRecord[reason] = count;
  }

  return {
    timestamp: new Date().toISOString(),
    coverage,
    unrenamed: { unchanged, missing, duplicate, invalid },
    renamed,
    patterns: {
      topCollisionTargets,
      unchangedIdentifiers: unchangedNames,
      lowestCoverageFunctions,
      failuresByRound: roundsRecord,
      missingByFinishReason: finishReasonRecord,
    },
  };
}

function bumpRound(map: Map<number, number>, round: number): void {
  map.set(round, (map.get(round) ?? 0) + 1);
}

export function writeDiagnosticsFile(report: DiagnosticsReport, path: string): void {
  fs.writeFileSync(path, JSON.stringify(report, null, 2) + "\n");
}
