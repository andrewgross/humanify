/**
 * Prove that a recorded fact is actually READ.
 *
 * ## The failure this exists to make impossible
 *
 * Every measurement bug this project has shipped has one shape: **the fact
 * was recorded and nothing consulted it.** Boot and self-hop verdicts were
 * write-only for their whole life, and a reference labelled valid carried an
 * unread self-hop violation. `preflight-status.json` was written and read by
 * nothing on the day it was added. `noise-bands.json` records the commit it
 * was measured at, and the leaderboard applied those bands to labels from a
 * different commit without ever looking — turning an UNMEASURED delta into a
 * confident verdict on the exact metric under dispute.
 *
 * `run-manifest.ts` already states the rule: *a fact nobody reads is worth
 * what a fact nobody recorded is worth.* This is the enforcement. Nothing
 * else in the repo can check it, because the existing guards are checklists
 * of known incidents and a checklist cannot catch the next one — the
 * verdict-file guard was two hardcoded names and sailed straight past a
 * third file added years-of-lessons later.
 *
 * ## Why a Proxy and not a grep
 *
 * The bands failure was FIELD level: the file was read, `provenance.commit`
 * was not. A source grep cannot see that — `commit` appears everywhere — and
 * a file-level guard passes while the bug is live. Recording actual property
 * access during a real call is the only check that distinguishes "read" from
 * "in a file something opened".
 *
 * ## How to use it
 *
 * Build a fully-populated record, wrap it, run it through EVERY consumer,
 * and compare `leafPaths(record)` against what was touched. Anything unread
 * is either a missing reader or a fact recorded purely for reproduction —
 * and the second kind has to be declared, so the distinction stays a
 * decision somebody made rather than an oversight.
 */

/**
 * Dotted paths of every leaf in a record — `provenance.commit`,
 * `config.cache.enabled`, `boots[].ok`.
 *
 * An array of RECORDS descends into its first element under a `[]` segment,
 * because that is where the facts live: the boot verdicts were a list, and
 * what went unread for their whole life was `ok` on each element, not the
 * list itself. Homogeneity is assumed — these are serialised records, not
 * arbitrary data — and an EMPTY array is a leaf, since nothing can be said
 * about fields that have no instance to carry them.
 */
export function leafPaths(value: unknown, prefix = ""): string[] {
  const self = prefix ? [prefix] : [];
  if (Array.isArray(value)) {
    return isRecord(value[0]) ? leafPaths(value[0], `${prefix}[]`) : self;
  }
  if (!isRecord(value)) return self;
  const out: string[] = [];
  for (const [k, v] of Object.entries(value)) {
    out.push(...leafPaths(v, prefix ? `${prefix}.${k}` : k));
  }
  return out.length > 0 ? out : self;
}

/** A plain record we can descend into — not null, not an array, not a Date. */
function isRecord(v: unknown): v is Record<string, unknown> {
  return (
    v !== null &&
    typeof v === "object" &&
    !Array.isArray(v) &&
    !(v instanceof Date)
  );
}

export interface ReadTracker<T> {
  /** Pass THIS to the consumers, not the original. */
  proxy: T;
  /** Dotted paths touched so far, including intermediate objects. */
  read: Set<string>;
}

/**
 * Wrap a record so every property access is recorded.
 *
 * Reading an intermediate object (`m.provenance`) records that path too, but
 * does NOT mark its leaves read — reaching for the container is not the same
 * as consulting the value, and the bands bug lived exactly in that gap.
 */
export function trackReads<T extends object>(record: T): ReadTracker<T> {
  const read = new Set<string>();
  const wrap = (value: unknown, prefix: string): unknown => {
    if (value === null || typeof value !== "object") return value;
    if (Array.isArray(value)) {
      // Indices are not facts, and neither are `length`/`filter`/`map`. Only
      // ELEMENT fields count, so an index access hands back a wrapped element
      // under `prefix[]` and everything else passes through untouched. Array
      // methods are deliberately not bound: called on the proxy they reach
      // their elements THROUGH it, so `boots.filter(b => b.ok)` records
      // `boots[].ok` exactly as a hand-written loop would.
      return new Proxy(value, {
        get(target, prop, receiver) {
          const v = Reflect.get(target, prop, receiver);
          if (typeof prop === "string" && /^\d+$/.test(prop)) {
            return wrap(v, `${prefix}[]`);
          }
          return v;
        }
      });
    }
    return new Proxy(value as Record<string, unknown>, {
      get(target, prop, receiver) {
        if (typeof prop !== "string")
          return Reflect.get(target, prop, receiver);
        const path = prefix ? `${prefix}.${prop}` : prop;
        read.add(path);
        return wrap(Reflect.get(target, prop, receiver), path);
      }
    });
  };
  return { proxy: wrap(record, "") as T, read };
}

/**
 * Leaves of `record` that no consumer touched.
 *
 * `archival` names paths recorded deliberately for reproduction or forensics
 * rather than for judgement — a run's node version is not something a warning
 * should fire on, but losing it makes a run unreproducible. Each entry is a
 * decision on the record, which is the point: an undeclared unread field is
 * an oversight, and a declared one is a choice someone can argue with.
 */
export function unreadLeaves(
  record: object,
  tracker: ReadTracker<object>,
  archival: readonly string[] = []
): string[] {
  const declared = new Set(archival);
  return leafPaths(record).filter(
    (p) => !tracker.read.has(p) && !declared.has(p)
  );
}
