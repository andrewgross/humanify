/**
 * The payload that travels from the prior-version matcher to the split, as ONE
 * object rather than a field per layer.
 *
 * Four layers sit between the two — `prior-version.ts`, `prior-transfer.ts`,
 * `plugin.ts`, `unified.ts` — and none of them USES this data; they forward it.
 * When each restated the payload field by field, adding one optional field cost
 * eleven edits across five files, and missing the empty-result branch left the
 * field silently `undefined` on a path that still typechecked. That happened
 * twice (`priorMatchMap`, then `priorStatementTexts`) and is entry #2 in
 * `docs/refactor-backlog-edit-amplification.md`.
 *
 * Both interfaces below take REQUIRED fields on purpose. A producer that forgets
 * one now fails to compile, which is the failure mode the field-per-layer shape
 * could not catch. Absence is expressed by the carry being absent as a whole —
 * no prior version, no carry, every tier a no-op.
 */

/**
 * The half collected while the prior AST is still alive, which is the only
 * moment it can be: re-parsing the prior bundle at split time would hold two
 * full bundle graphs at once, the measured cause of the 2.1.216 split OOM.
 */
export interface MatcherCarry {
  /**
   * Source text of every prior TOP-LEVEL statement, in bundle order — the same
   * order the prior split ledger's `order` array is indexed by, so the two zip
   * into (text, file) pairs. Drives the content-anchor tier, which identifies a
   * statement across releases by the rare string literals it carries when
   * neither its hash nor its name can. Empty ⇒ the tier is a no-op.
   */
  statementTexts: readonly string[];
}

/**
 * The complete payload the split's prior-carried tiers read. Completed by the
 * rename plugin, because `matchMap` cannot exist before then: it is keyed by the
 * FINAL shipped name, which is only settled once every rename pass has run.
 */
export interface PriorCarry extends MatcherCarry {
  /**
   * Cross-version binding identity: new final name → the name its matched prior
   * counterpart carried. Lets a renamed, content-changed statement inherit its
   * prior file when neither the hash tier (content changed) nor the name-vote
   * tier (name flipped) can. Empty ⇒ the identity tiers are a no-op.
   */
  matchMap: ReadonlyMap<string, string>;
}

/** No prior statements were collected — every content-anchor tier abstains. */
export function emptyMatcherCarry(): MatcherCarry {
  return { statementTexts: [] };
}

/** No prior evidence at all — every prior-carried tier abstains.
 *  @internal Fixture builder for stable-split.test.ts and the experiment
 *  harness (experiments/lib/counterfactual.ts) — knip:prod exempt via `tags`
 *  in package.json. */
export function emptyPriorCarry(): PriorCarry {
  return { ...emptyMatcherCarry(), matchMap: new Map() };
}
