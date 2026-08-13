/**
 * Prior-provenance carry registry (exp066).
 *
 * Andrew's rule: a name read from OUR OWN prior output was already
 * processed once — "looks minified" is not grounds to refuse the carry.
 * The old below-floor refusal bought a fresh LLM ask EVERY hop (the
 * exp065 fs2/n0e loop: three agreeing votes, refused, re-asked forever)
 * in exchange for hoping the name improves; it rarely did. Stability and
 * quality are now split: carry tiers ALWAYS apply the prior name and
 * register the binding here when the name is below the naming floor, and
 * the LLM coverage sweep — a first-contact namer, not a re-roller of
 * carried identities — consults this registry and skips them. Deliberate
 * consequences, accepted with eyes open (outputs are not permanent):
 *
 *   - A carried below-floor name is FROZEN until a deterministic
 *     improvement pass exists for its shape. It stays visible in the
 *     minted census (`remainingMinted`), so the zero-leftovers goal is
 *     tracked by measurement, never silently traded away.
 *   - The __m-poisoning class (exp035) is bounded by what the prior
 *     output actually contains, and the scored run's minted census is
 *     the regression alarm.
 *
 * Keyed by the binding's Identifier node (same identity discipline as
 * the strategy trail), but ALWAYS ON — the sweep's behavior depends on
 * it, so it must not be a diagnostics-only channel.
 */
import type { Binding } from "@babel/traverse";
import type * as t from "@babel/types";

class CarriedNamesRegistry {
  private carried = new WeakSet<t.Identifier>();
  private count = 0;

  /** Start a run with an empty registry. */
  reset(_enabled: boolean): void {
    this.carried = new WeakSet();
    this.count = 0;
  }

  /** Record that this binding's settled name was carried from the prior
   * output despite being below the naming floor. */
  record(binding: Binding): void {
    if (this.carried.has(binding.identifier)) return;
    this.carried.add(binding.identifier);
    this.count++;
  }

  isCarried(binding: Binding): boolean {
    return this.carried.has(binding.identifier);
  }

  /** Carries recorded this run — the rule-11 trail: an empty registry
   * cannot have exempted anything from the sweep. */
  recordedCount(): number {
    return this.count;
  }
}

export const carriedNames = new CarriedNamesRegistry();
