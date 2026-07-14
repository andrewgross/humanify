/**
 * Cross-version stability (P5). Clustering runs ONCE on a baseline; every
 * release after inherits each statement's file from the ledger by
 * declared-name vote (the proven exp020/023 mechanism — path-string
 * agnostic, so it works unchanged for clustered paths). The ONLY new
 * decision is where a genuinely-new binding lands. stable-split uses
 * textual locality (`assignment[i-1]`), which is right only for contiguous
 * budget files; under fine clustering a new statement's textual neighbor
 * may be an unrelated file. This module adds REFERENCE-AFFINITY placement:
 * put the new binding in the file its already-placed references live in —
 * grow the existing cluster. Provably cannot move inherited code (it only
 * decides abstaining statements).
 */

import * as t from "@babel/types";
import type { RefGraph } from "./graph.js";

export interface Ledger {
  /** declared name → ordered list of the files its occurrences landed in */
  nameToFiles: Map<string, string[]>;
}

function declaredNames(stmt: t.Statement): string[] {
  return Object.keys(t.getBindingIdentifiers(stmt, false));
}

export function buildLedger(body: t.Statement[], order: string[]): Ledger {
  const nameToFiles = new Map<string, string[]>();
  for (let i = 0; i < body.length; i++) {
    for (const nm of declaredNames(body[i])) {
      const l = nameToFiles.get(nm) ?? [];
      l.push(order[i]);
      nameToFiles.set(nm, l);
    }
  }
  return { nameToFiles };
}

function countOccurrences(body: t.Statement[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const stmt of body) {
    for (const n of declaredNames(stmt))
      counts.set(n, (counts.get(n) ?? 0) + 1);
  }
  return counts;
}

/** One name's vote (from stable-split.voteFor): unanimous file, or the kth
 * prior file when occurrence counts match, else abstain. */
function voteFor(
  name: string,
  ordinal: number,
  priorNames: Map<string, string[]>,
  newCounts: Map<string, number>
): string | undefined {
  const files = priorNames.get(name);
  if (!files || files.length === 0) return undefined;
  if (files.every((f) => f === files[0])) return files[0];
  if (newCounts.get(name) === files.length && ordinal < files.length) {
    return files[ordinal];
  }
  return undefined;
}

/** Placement for an abstaining (new/ambiguous) statement i, given the paths
 * assigned so far. Returns a file path. */
export type PlaceNew = (
  i: number,
  assignment: Array<string | undefined>
) => string;

/** stable-split's current rule: follow the preceding placed neighbor. */
export function textualLocality(fallback: string): PlaceNew {
  return (i, assignment) =>
    i > 0 ? (assignment[i - 1] ?? fallback) : fallback;
}

/** Reference-affinity: the file most of this statement's ALREADY-PLACED
 * references (both directions) live in, IDF-weighted; textual locality as a
 * last resort when nothing it touches is placed yet. */
export function referenceAffinity(
  g: RefGraph,
  reverseRefs: Array<number[]>,
  fallback: string
): PlaceNew {
  return (i, assignment) => {
    const tally = new Map<string, number>();
    for (const j of g.refs[i]) {
      const f = assignment[j];
      if (f) tally.set(f, (tally.get(f) ?? 0) + g.idf[j]);
    }
    for (const k of reverseRefs[i]) {
      const f = assignment[k];
      if (f) tally.set(f, (tally.get(f) ?? 0) + g.idf[i]);
    }
    let best: string | undefined;
    let bestW = -1;
    for (const [f, w] of tally) {
      if (w > bestW) {
        bestW = w;
        best = f;
      }
    }
    return best ?? (i > 0 ? (assignment[i - 1] ?? fallback) : fallback);
  };
}

export function reverseRefsOf(g: RefGraph): Array<number[]> {
  const rev: Array<number[]> = Array.from({ length: g.n }, () => []);
  for (let i = 0; i < g.n; i++) for (const j of g.refs[i]) rev[j].push(i);
  return rev;
}

export interface InheritStats {
  inherited: number;
  placed: number;
}

/** Inherit files from the ledger; abstaining statements go to placeNew.
 * `placedMask[i]` marks a statement that abstained (was placed, not
 * inherited) — the same set regardless of placeNew, since abstention
 * depends only on the ledger votes. */
export function inherit(
  body: t.Statement[],
  ledger: Ledger,
  placeNew: PlaceNew
): { order: string[]; stats: InheritStats; placedMask: boolean[] } {
  const priorNames = ledger.nameToFiles;
  const newCounts = countOccurrences(body);
  const seen = new Map<string, number>();
  const assignment: Array<string | undefined> = new Array(body.length);
  const placedMask = new Array<boolean>(body.length).fill(false);
  const stats: InheritStats = { inherited: 0, placed: 0 };
  for (let i = 0; i < body.length; i++) {
    const votes = new Set<string>();
    for (const nm of declaredNames(body[i])) {
      const ord = seen.get(nm) ?? 0;
      seen.set(nm, ord + 1);
      const f = voteFor(nm, ord, priorNames, newCounts);
      if (f) votes.add(f);
    }
    if (votes.size === 1) {
      assignment[i] = [...votes][0];
      stats.inherited++;
    } else {
      assignment[i] = placeNew(i, assignment);
      placedMask[i] = true;
      stats.placed++;
    }
  }
  return { order: assignment as string[], stats, placedMask };
}
