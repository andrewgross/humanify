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

export interface IdentifierLedger {
  /** Every binding in the final AST (census scope walk). */
  totalBindings?: number;
  /** Bindings settled by each mechanical transfer strategy (trail). */
  transferSettled: Record<string, number>;
  /** LLM-path outcomes (coverage identifiers). */
  llmNamed: number;
  libraryPrefix: number;
  fallback: number;
  notRenamed: number;
  /** Still-minted bindings in the shipped output — the bottom line. */
  remainingMinted?: number;
  /** Ground-truth end-state accounting (exp035 task D). */
  terminalState?: TerminalStateRollup;
}

/**
 * Per-binding END-state accounting from bookkeeping, not name shape:
 * who named each binding LAST (trail `terminalBy`, spanning transfer
 * tiers, floor passes, reconcile, and the sweep), plus a bookkeeping
 * join over the census — every still-minted name should be explained
 * by a recorded decision (applied TO it, kept after refusals, or an
 * LLM outcome). `mintedUnaccounted` is the honest residue the shape
 * classifier papers over; drive it to zero and the classifier becomes
 * redundant.
 */
export interface TerminalStateRollup {
  /** Every binding in the final AST — the 100% denominator. */
  totalBindings?: number;
  /** Bindings whose LAST applied rename came from each pass. */
  namedByTier: Record<string, number>;
  /** LLM-path applied renames (coverage identifiers). */
  llmNamed: number;
  /** Census names explained by recorded decisions (multiset by name). */
  mintedAccounted: number;
  /** Census names with NO bookkeeping trace — the gap. */
  mintedUnaccounted: string[];
}

/** Multiset credit pools for the census bookkeeping join. */
function creditPools(
  trails: StrategyTrailReport | undefined,
  renamed: RenamedEntry[],
  unrenamedNames: string[]
): Map<string, number> {
  const credits = new Map<string, number>();
  const add = (name: string) => credits.set(name, (credits.get(name) ?? 0) + 1);
  for (const entry of trails?.trails ?? []) {
    const lastApplied = [...entry.trail]
      .reverse()
      .find((a) => a.outcome === "applied");
    if (lastApplied?.newName) {
      // Renamed TO this name by a recorded pass (twin `_`, restores,
      // decorated applies).
      add(lastApplied.newName);
    } else if (entry.trail.length > 0) {
      // Touched but never renamed — the binding KEPT its name through
      // refusals/abstains (h06Result after a still-below-floor refusal).
      add(entry.oldName);
    }
  }
  for (const r of renamed) add(r.newName);
  for (const name of unrenamedNames) add(name);
  return credits;
}

function buildTerminalState(
  coverage: CoverageSummary,
  trails: StrategyTrailReport | undefined,
  renamed: RenamedEntry[],
  unrenamedNames: string[]
): TerminalStateRollup {
  const namedByTier: Record<string, number> = {};
  for (const entry of trails?.trails ?? []) {
    const tier = entry.terminalBy ?? entry.settledBy;
    if (!tier) continue;
    namedByTier[tier] = (namedByTier[tier] ?? 0) + 1;
  }
  const credits = creditPools(trails, renamed, unrenamedNames);
  const unaccounted: string[] = [];
  let accounted = 0;
  for (const name of coverage.mintedCensus?.names ?? []) {
    const left = credits.get(name) ?? 0;
    if (left > 0) {
      credits.set(name, left - 1);
      accounted += 1;
    } else {
      unaccounted.push(name);
    }
  }
  return {
    totalBindings: coverage.mintedCensus?.totalBindings,
    namedByTier,
    llmNamed: coverage.identifiers.llm,
    mintedAccounted: accounted,
    mintedUnaccounted: unaccounted
  };
}

function buildIdentifierLedger(
  coverage: CoverageSummary,
  strategyTrails: StrategyTrailReport | undefined,
  renamed: RenamedEntry[],
  unrenamedNames: string[]
): IdentifierLedger {
  const transferSettled: Record<string, number> = {};
  for (const entry of strategyTrails?.trails ?? []) {
    if (!entry.settledBy) continue;
    transferSettled[entry.settledBy] =
      (transferSettled[entry.settledBy] ?? 0) + 1;
  }
  return {
    totalBindings: coverage.mintedCensus?.totalBindings,
    transferSettled,
    llmNamed: coverage.identifiers.llm,
    libraryPrefix: coverage.identifiers.libraryPrefix,
    fallback: coverage.identifiers.fallback,
    notRenamed: coverage.identifiers.notRenamed,
    remainingMinted: coverage.mintedCensus?.total,
    terminalState: buildTerminalState(
      coverage,
      strategyTrails,
      renamed,
      unrenamedNames
    )
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
  /**
   * Totals-first identifier accounting: the full binding population at
   * the top, per-path naming counts, and the still-minted REMAINING at
   * the bottom. Buckets are independent measurements (transfer counts
   * come from the trail, LLM counts from reports, remaining from the
   * census walk) — not a strict partition.
   */
  identifierLedger?: IdentifierLedger;
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
    identifierLedger: buildIdentifierLedger(
      coverage,
      strategyTrails,
      renamed,
      [...unchanged, ...missing, ...duplicate, ...invalid].map((e) => e.name)
    ),
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
