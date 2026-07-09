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
 */

const DECORATION_SUFFIX =
  /(?:Val|Var|Value|Ref|Instance|Data|Obj|Item|_?\d+)+$/;

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
 * Snap one suggestion: returns the unique same-stem prior name, or the
 * suggestion unchanged.
 */
export function snapSuggestionToPrior(
  suggestion: string,
  priorStemIndex: Map<string, string>
): string {
  const prior = priorStemIndex.get(nameStem(suggestion));
  return prior ?? suggestion;
}
