/**
 * Ambiguity probe — instrumentation for the identity-recovery ceiling.
 *
 * After the match cascade settles, the still-ambiguous buckets are the
 * fns whose blurred evidence (shapes, callee-hash SETS) tied. The open
 * question: how many become uniquely pairable under IDENTITY-grade keys
 * — the set of MATCHED callers / MATCHED callees, translated through
 * the match map — which is strictly stronger than hash sets (two
 * same-hash callees can have different identities).
 *
 * This module only DUMPS the evidence as plain data; the ceiling math
 * lives offline (experiments/034-eval-harness/ceiling-identity-recovery
 * .ts). Enabled by HUMANIFY_AMBIGUITY_PROBE=<path>; written from
 * matchPriorVersion while both graphs are alive.
 */
import * as fs from "node:fs";
import type { FunctionNode, MatchResult } from "../analysis/types.js";

interface FnEvidence {
  /** First line of the function (prior side carries the humanified name). */
  head: string;
  callers: string[];
  callees: string[];
}

export interface AmbiguityProbe {
  /** prior session id → fresh session id (settled matches). */
  matches: Record<string, string>;
  /** prior ambiguous fn → its fresh candidate pool. */
  ambiguous: Record<string, string[]>;
  prior: Record<string, FnEvidence>;
  fresh: Record<string, FnEvidence>;
}

function evidenceOf(fn: FunctionNode): FnEvidence {
  let head = "";
  try {
    const node = fn.path.node;
    head = `${node.type}@${fn.position?.line ?? "?"}`;
    const id = (node as { id?: { name?: string } }).id;
    if (id?.name) head = `${id.name} ${head}`;
  } catch {
    head = "?";
  }
  return {
    head,
    callers: [...fn.callers].map((c) => c.sessionId).sort(),
    callees: [...fn.internalCallees].map((c) => c.sessionId).sort()
  };
}

/**
 * Build the probe: evidence for every fn that participates in an
 * ambiguous bucket (either side), plus the match map for translation.
 */
export function buildAmbiguityProbe(
  matchResult: MatchResult,
  priorById: ReadonlyMap<string, FunctionNode>,
  freshById: ReadonlyMap<string, FunctionNode>
): AmbiguityProbe {
  const probe: AmbiguityProbe = {
    matches: Object.fromEntries(matchResult.matches),
    ambiguous: {},
    prior: {},
    fresh: {}
  };
  for (const [priorId, candidates] of matchResult.ambiguous) {
    probe.ambiguous[priorId] = candidates;
    const priorFn = priorById.get(priorId);
    if (priorFn && !probe.prior[priorId]) {
      probe.prior[priorId] = evidenceOf(priorFn);
    }
    for (const freshId of candidates) {
      const freshFn = freshById.get(freshId);
      if (freshFn && !probe.fresh[freshId]) {
        probe.fresh[freshId] = evidenceOf(freshFn);
      }
    }
  }
  return probe;
}

/** Write the probe when HUMANIFY_AMBIGUITY_PROBE is set; never throws. */
export function maybeWriteAmbiguityProbe(
  matchResult: MatchResult,
  priorById: ReadonlyMap<string, FunctionNode>,
  freshById: ReadonlyMap<string, FunctionNode>
): void {
  const path = process.env.HUMANIFY_AMBIGUITY_PROBE;
  if (!path) return;
  try {
    const probe = buildAmbiguityProbe(matchResult, priorById, freshById);
    fs.writeFileSync(path, JSON.stringify(probe));
  } catch {
    // Instrumentation must never fail the run.
  }
}
