/**
 * Emission order for the Bun vendor manifest, and the field that keeps the
 * naming tie-break recoverable once order stops encoding it.
 *
 * ## Why the manifest is not written in bundle order
 *
 * `_bun-modules.json` used to be serialized in bundle order. Bun reorders
 * factories freely between builds, and a reordered entry moves as a whole
 * seven-line `{...}` block, so `diff` charges every line of it. exp047 measured
 * that at **4,574 lines on 85→86 alone** — a hop where every entry's CONTENT is
 * byte-identical and nothing but the order changed — and 6,407 across the four
 * gate hops, which was 67% of all remaining vendor churn.
 *
 * Writing entries in the PRIOR RELEASE'S order instead takes that to 1,627:
 *
 * | hop     | bundle order | prior order |
 * | ------- | -----------: | ----------: |
 * | 85→86   |        4,574 |       **0** |
 * | 118→119 |           20 |          20 |
 * | 197→198 |        1,498 |       1,418 |
 * | 215→216 |          315 |         189 |
 *
 * ## Why not simply SORT the file
 *
 * That was the obvious move and it fails the gate. Every content-derived sort
 * key measured (`structuralHash`, `fileName`, `name`, and combinations) takes
 * 85→86 to 0 and REGRESSES 197→198 by 128-496 lines and the 118→119 canary by 8.
 * The mechanism: when an entry's sort key changes, sorting relocates it, turning
 * what bundle order charged as an in-place edit into a delete at one position
 * plus an add at another. Bundle order keeps a content change local; a sort
 * scatters it. Prior-release order keeps it local AND absorbs the reshuffle.
 *
 * ## Why the recovery field is `hashOrdinal` and not `bundleIndex`
 *
 * `priorNameFor` disambiguates same-`structuralHash` groups by a factory's
 * position within its group in bundle order, because re-export shims are
 * structurally identical while proxying different libraries — 129-145 groups per
 * hop have members that disagree about `name`, so losing that tie-break would
 * misname them, and vendor names feed `src/` require paths.
 *
 * The obvious fix — store the bundle index — is a trap, and a measured one:
 * a bundle index records the churn it is meant to make recoverable. With it the
 * manifest diff comes to **7,056 lines, WORSE than the 6,407 baseline**, because
 * 1,132 of 1,592 entries land at a new index on 85→86. It is the same pathology
 * as the `factoryVar` field exp046 deleted for churning on every entry of every
 * release.
 *
 * `hashOrdinal` — the entry's position WITHIN its structuralHash group — is what
 * `priorNameFor` actually indexes with, and it survives a global reshuffle
 * untouched. Measured cost: **zero**, identical to the no-field ideal. It is
 * written only for the 19-26% of entries whose group has two or more members;
 * for a singleton the ordinal is always 0 and carries no information.
 */
import type { BunModulesManifestEntry } from "./adapters/bun.js";
import { envFlag } from "../kill-switches.js";

/**
 * Set to any value to emit exactly the pre-exp047 manifest: bundle order, and no
 * `hashOrdinal` field. Reverting BOTH halves is what makes this usable as the
 * control leg of a same-session A/B — see `annotateHashOrdinals`.
 */
export const MANIFEST_PRIOR_ORDER_OFF_ENV = "HUMANIFY_NO_MANIFEST_PRIOR_ORDER";

/**
 * Stamp each entry whose `structuralHash` is shared with another entry with its
 * ordinal within that group, counted in the order given (which at the call site
 * is bundle order — the order `priorNameFor` walks factories in).
 *
 * Singletons are left alone: their ordinal is always 0, so writing it would add
 * a line per entry to the shipped file for no recoverable information.
 */
export function annotateHashOrdinals(
  entries: BunModulesManifestEntry[]
): BunModulesManifestEntry[] {
  // The kill switch reverts the WHOLE of exp047, field included — it is what
  // produces the pre-exp047 control leg of a same-session A/B, and a control
  // still carrying `hashOrdinal` would not be one.
  if (envFlag(MANIFEST_PRIOR_ORDER_OFF_ENV)) return entries;
  const groupSize = new Map<string, number>();
  for (const e of entries) {
    groupSize.set(e.structuralHash, (groupSize.get(e.structuralHash) ?? 0) + 1);
  }
  const seen = new Map<string, number>();
  return entries.map((e) => {
    const n = seen.get(e.structuralHash) ?? 0;
    seen.set(e.structuralHash, n + 1);
    if ((groupSize.get(e.structuralHash) ?? 1) < 2) return e;
    return { ...e, hashOrdinal: n };
  });
}

/**
 * Reorder `fresh` (in bundle order) to follow `prior`'s emitted order.
 *
 * Correspondence is established in three passes, each consuming prior slots
 * one-to-one so no two fresh entries can claim the same position:
 *
 *   1. `structuralHash` — an unchanged library, which is ~99% of the tree.
 *   2. `name` — the library's content changed, so its hash AND its
 *      `lib_<hash>` filename both rotated, but the carried-over name held.
 *   3. Positional pairing of the leftovers. Whatever is still unmatched on each
 *      side is by construction the set of entries that changed this release, and
 *      their relative bundle order is largely preserved, so pairing the two
 *      leftover lists in order returns a changed entry to the slot its own prior
 *      version held — an in-place edit rather than a relocation. This pass is
 *      what closes the last +10 lines on 197→198; without it that hop regresses.
 *
 * An entry that matches nothing at all -- a genuinely new library on a release
 * that added more entries than it removed -- trails the last anchored entry that
 * preceded it in bundle order, so it stays beside the entries it shipped with
 * rather than collecting at the end of the file.
 *
 * Ordering only. No entry's `name`, `fileName` or any other field is touched —
 * a vendor NAME change rewrites `src/` require paths, so this must be provably
 * name-neutral, and `orderByPriorManifest` returns the very objects it was given.
 */
export function orderByPriorManifest(
  fresh: BunModulesManifestEntry[],
  prior: BunModulesManifestEntry[] | undefined
): BunModulesManifestEntry[] {
  if (!prior || prior.length === 0) return fresh;
  if (envFlag(MANIFEST_PRIOR_ORDER_OFF_ENV)) return fresh;

  const claimed = new Set<number>();
  const poolBy = (key: (e: BunModulesManifestEntry) => string) => {
    const m = new Map<string, number[]>();
    prior.forEach((e, i) => {
      const k = key(e);
      const l = m.get(k);
      if (l) l.push(i);
      else m.set(k, [i]);
    });
    return m;
  };
  const byHash = poolBy((e) => e.structuralHash);
  const byName = poolBy((e) => e.name);
  const claimFrom = (
    pool: Map<string, number[]>,
    k: string
  ): number | undefined => {
    const l = pool.get(k);
    while (l && l.length > 0) {
      const c = l.shift();
      if (c !== undefined && !claimed.has(c)) {
        claimed.add(c);
        return c;
      }
    }
    return undefined;
  };

  // Passes 1 and 2, in bundle order so that ties resolve deterministically.
  const anchors = fresh.map(
    (e) => claimFrom(byHash, e.structuralHash) ?? claimFrom(byName, e.name)
  );

  // Pass 3: pair the leftovers.
  const leftover: number[] = [];
  prior.forEach((_, i) => {
    if (!claimed.has(i)) leftover.push(i);
  });
  // Position is an (anchor, after) pair. An entry with a prior slot sits AT it.
  // An entry with no slot at all -- a genuinely new library on a release that
  // added more entries than it removed -- trails the last anchored entry that
  // preceded it in bundle order, so it stays next to its neighbours instead of
  // collecting at the end of the file. Appending those was measured at +494
  // lines on 197->198 and +8 on the 118->119 canary: relocating an entry is
  // never cheaper than leaving it beside the entries it shipped with.
  let next = 0;
  let lastAnchor = -1;
  let after = 0;
  const placed = fresh.map((entry, bundleIndex) => {
    let anchor = anchors[bundleIndex];
    if (anchor === undefined) {
      const slot = leftover[next];
      if (slot !== undefined) {
        next += 1;
        anchor = slot;
      }
    }
    if (anchor === undefined) {
      after += 1;
      return { entry, bundleIndex, anchor: lastAnchor, after };
    }
    lastAnchor = anchor;
    after = 0;
    return { entry, bundleIndex, anchor, after: 0 };
  });

  return placed
    .sort(
      (a, b) =>
        a.anchor - b.anchor ||
        a.after - b.after ||
        a.bundleIndex - b.bundleIndex
    )
    .map((p) => p.entry);
}
