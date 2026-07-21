import type { Identifier } from "@babel/types";

/**
 * A module binding the prior-version matcher mapped to a prior counterpart,
 * paired with the live declaration identifier whose `.name` becomes the FINAL
 * shipped name once every rename pass has run.
 */
export interface MatchedBindingRef {
  /** Live declaration identifier — read `.name` after renames settle. */
  identifier: Identifier;
  /** The prior-version name this binding was matched to. */
  priorName: string;
}

/**
 * Build the split's binding-identity map `{final name -> prior name}` from the
 * bindings the matcher mapped across versions.
 *
 * Only FLIPPED bindings are useful: when `final === prior` the name was pinned,
 * so the split's name-vote already sends it to the right (own) prior file —
 * nothing to inherit. A final name that resolves to two different priors is
 * ambiguous (untrustworthy identity key) and dropped, so a downstream tier
 * never pins on a contested name.
 *
 * The map is a best-effort signal: the split's identity tier abstains on any
 * name absent from the split input, so a stale key is a harmless no-op — it can
 * only reduce reach, never mis-file a statement.
 */
export function buildPriorMatchMap(
  refs: readonly MatchedBindingRef[]
): Map<string, string> {
  // null marks a final name seen with conflicting priors — poisoned, dropped.
  const resolved = new Map<string, string | null>();
  for (const { identifier, priorName } of refs) {
    const finalName = identifier.name;
    if (finalName === priorName) continue;
    const prev = resolved.get(finalName);
    if (prev === undefined) resolved.set(finalName, priorName);
    else if (prev !== null && prev !== priorName) resolved.set(finalName, null);
  }
  const map = new Map<string, string>();
  for (const [finalName, priorName] of resolved) {
    if (priorName !== null) map.set(finalName, priorName);
  }
  return map;
}
