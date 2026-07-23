/**
 * Snap LLM name suggestions to prior-version names.
 *
 * A close-matched function's prompt carries the prior code and names, but
 * the LLM freely re-decorates: `identityVal` becomes `identityVar`,
 * `config` becomes `configVar`, `RpcRequestSchema` becomes
 * `rpcRequestSchema` — same meaning, different surface, and every such
 * choice is a diff hunk against the prior release. When a suggestion
 * shares its STEM (name minus trailing Val/Var/Ref/…/digits decorations,
 * case-insensitive) with exactly ONE prior name, the prior name is the
 * right answer — reuse it verbatim.
 *
 * Precision guards: the stem must be non-empty and uniquely held by one
 * prior name (React95/React103/ink8 all stem to "react" — ambiguous,
 * never snapped); an exact prior-name suggestion passes through
 * untouched; validation still runs on the snapped name downstream.
 *
 * A stronger, exact-slot channel (A2) catches full synonym flips
 * (`caughtError` → `decisionOutcome`) that share no stem: when the exact
 * minified slot has a per-slot prior name that PASSED the upstream content
 * agreement gate (`priorNameSnaps`), that name is authoritative and the
 * suggestion snaps to it regardless of stem. The gate — the new binding's
 * definition still corroborates its prior counterpart — is what keeps a
 * genuinely repurposed binding from being snapped.
 */

import { DECORATION_WORDS } from "../llm/validation.js";

/** LLM-authored decorations we strip but never produce. */
const STRIP_ONLY_WORDS = ["Instance", "Obj"];

/**
 * Ladder words we deliberately do NOT strip: `Result` is usually a
 * semantic name part (`compareResult`, `hashResult`), not a decoration.
 * Measured on pair 215→216: stripping it merges 1,690 previously-unique
 * prior stems into ambiguity (dropping their snap entries) to recover
 * only 6 decoration misses. The ladder keeps producing it — as a
 * conflict-breaker its wordiness is a feature — the stripper skips it.
 */
const LADDER_ONLY_WORDS = new Set(["Result"]);

const DECORATION_SUFFIX = new RegExp(
  `(?:${[
    ...DECORATION_WORDS.filter((w) => !LADDER_ONLY_WORDS.has(w)),
    ...STRIP_ONLY_WORDS
  ].join("|")}|_?\\d+)+$`
);

/** Name minus trailing decorations, lowercased. Empty when all decoration. */
export function nameStem(name: string): string {
  return name.replace(DECORATION_SUFFIX, "").toLowerCase();
}

/**
 * Build a stem → prior-name lookup containing only UNIQUE stems.
 * Ambiguous stems (two prior names, one stem) are dropped entirely.
 */
export function buildPriorStemIndex(
  priorNames: readonly string[]
): Map<string, string> {
  const byStem = new Map<string, string | null>();
  for (const name of priorNames) {
    const stem = nameStem(name);
    if (!stem) continue;
    byStem.set(stem, byStem.has(stem) ? null : name);
  }
  const unique = new Map<string, string>();
  for (const [stem, name] of byStem) {
    if (name !== null) unique.set(stem, name);
  }
  return unique;
}

/**
 * Snap one suggestion back to a prior name.
 *
 * Precedence: an exact-slot snap (A2, content-gated upstream) is
 * authoritative — when `oldName` carries one it wins over the LLM's pick and
 * over the stem index. Otherwise fall back to the same-stem decoration snap.
 */
export function snapSuggestionToPrior(
  suggestion: string,
  priorStemIndex: Map<string, string>,
  oldName?: string,
  priorNameSnaps?: Record<string, string>
): string {
  if (oldName && priorNameSnaps) {
    const slotPrior = priorNameSnaps[oldName];
    if (slotPrior) return slotPrior;
  }
  const prior = priorStemIndex.get(nameStem(suggestion));
  return prior ?? suggestion;
}
