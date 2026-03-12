/**
 * Detailed rename diagnostics report.
 *
 * Produces a JSON file with per-identifier outcomes and pattern analysis
 * to help understand why identifiers weren't renamed.
 */

import fs from "node:fs";
import type { RenameReport } from "../analysis/types.js";
import type { CoverageSummary } from "./coverage.js";

interface UnrenamedEntry {
  name: string;
  functionId: string;
  suggestion?: string;
  reason: string;
  attempts: number;
  detail?: string;
}

interface RenamedEntry {
  name: string;
  newName: string;
  functionId: string;
  round: number;
}

interface DiagnosticsReport {
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
  const failuresByAttempts = new Map<number, number>();
  const missingByFinishReason = new Map<string, number>();

  for (const report of reports) {
    for (const [name, outcome] of Object.entries(report.outcomes)) {
      switch (outcome.status) {
        case "renamed":
          renamed.push({
            name,
            newName: outcome.newName,
            functionId: report.targetId,
            round: outcome.round
          });
          break;

        case "unchanged":
          unchanged.push({
            name,
            functionId: report.targetId,
            suggestion: outcome.suggestion,
            reason: "LLM returned original name",
            attempts: outcome.attempts
          });
          unchangedNames.push(name);
          bumpCount(failuresByAttempts, outcome.attempts);
          break;

        case "missing": {
          missing.push({
            name,
            functionId: report.targetId,
            reason: "LLM did not return this identifier",
            attempts: outcome.attempts,
            detail: outcome.lastFinishReason
              ? `finish_reason: ${outcome.lastFinishReason}`
              : undefined
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
            functionId: report.targetId,
            suggestion: outcome.suggestion,
            reason: "Name collision unresolved",
            attempts: outcome.attempts,
            detail: `conflicted with: ${target}`
          });
          collisionTargets.set(target, (collisionTargets.get(target) ?? 0) + 1);
          bumpCount(failuresByAttempts, outcome.attempts);
          break;
        }

        case "invalid":
          invalid.push({
            name,
            functionId: report.targetId,
            suggestion: outcome.suggestion,
            reason: "Invalid identifier returned",
            attempts: outcome.attempts
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
    unrenamed: { unchanged, missing, duplicate, invalid },
    renamed,
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
