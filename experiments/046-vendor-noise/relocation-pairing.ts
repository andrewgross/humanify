/**
 * Which vendor file that LOST its path is the same library as one that
 * GAINED a path? (2026-10-06, decision B.)
 *
 * Vendor file paths are humanify's own draws. When a library changed a
 * little (an edited string, a small code change) its exact-content carry
 * fails, the pipeline names it afresh, and it can land at a new path. The
 * scorer used to charge that as the whole prior file removed plus the whole
 * fresh file added — the eslint-plugin-security case: a 151-line text module
 * with a real 6-line edit read 6 lines when both versions drew the same path
 * and 303 when they did not, and that path draw was booked as REAL
 * dependency change (~2,000 lines per run, /work/vendor-mapping-2026-10-06).
 *
 * ## The rule
 *
 * Candidates are only files the scorer would otherwise charge as truly
 * removed (prior side) and truly added (fresh side) — never a file still at
 * its own path. Each is reduced to its CONTENT tokens: the rename-invariant
 * stream `serializePathTokens` already produced for the content signature
 * (one masking owner), with binding slots collapsed to one token (an inserted
 * binding renumbers every later slot), syntax punctuation and field labels
 * dropped (they are shared by every file and would inflate any similarity),
 * and string / template text split into words (a text module is ONE literal
 * token; its edit must cost a few words, not the whole file). Similarity is
 * the Jaccard index of 5-token shingles.
 *
 * A pair is made only when the two files are each other's best candidate
 * (mutual best), the score clears `MIN_SCORE`, and it beats every competing
 * candidate EITHER file had by `MIN_MARGIN`. Two near-equal candidates make
 * no pair — ambiguity is charged the old way, never guessed. Files whose
 * content changed at a SURVIVING path compete without being pairable (see
 * `pairRelocated`), so a path swap cannot be mistaken for a move.
 *
 * ## The thresholds, from data (2026-10-06)
 *
 * Six labels x four hops (candidate-f616f33b/5d4b2d9a/c5ac0e98-scratch,
 * ref-scratch-0f338ffa-r1..r3). With NO thresholds the rule made 644
 * mutual-best pairs; against an independent truth (each highlight.js
 * grammar's own `name:` field, 2.1.198, five labels) 462 were right and 2
 * wrong, both at score 0.10. A third wrong pair (0.66, margin 0.57) was a
 * path swap, and is what the contenders fix. At 0.5 / 0.2 the rule makes 394
 * pairs: 280 checked against that truth, 0 wrong; a random 25 of the
 * unchecked label's pairs judged by the same field, 0 wrong; the six
 * distinct non-grammar pairs (2.1.119 git prompts, 2.1.216 colour scripts
 * and the eslint-plugin-security text, the highlight.js core) judged by
 * reading the diff, 0 wrong. Lowering the score to 0.3 adds ~90 pairs, all
 * one-line grammars whose whole-file diff is 2-4 lines either way, and
 * leaves the vendorReal band where it is (32), so the conservative 0.5
 * stands (precision over recall).
 */

/** Pair only above this similarity. */
const MIN_SCORE = 0.5;
/** ...and only when it beats every competing candidate by this much. */
const MIN_MARGIN = 0.2;

/**
 * ...and only files with at least this many content shingles: a tiny file
 * (`module.exports = require("x")`, a two-line shim) is mostly shared
 * syntax, so any two look alike, and charging one whole costs a few lines.
 * It refused no pair on the six labels above; it keeps two unrelated
 * one-statement fixtures (vendor-churn.test.ts) from pairing.
 */
const MIN_SHINGLES = 20;

const SHINGLE = 5;

/** The content-bearing part of one `serializePathTokens` token. */
function contentOf(tok: string, out: string[]): void {
  if (/^\$\d+$/.test(tok)) {
    out.push("$");
  } else if (/^L\d+$/.test(tok)) {
    out.push("L");
  } else if (tok.startsWith("S=") || tok.startsWith("Q=")) {
    out.push(tok.slice(0, 2));
    let text: string;
    try {
      text = String(JSON.parse(tok.slice(2)));
    } catch {
      text = tok.slice(2);
    }
    for (const w of text.split(/\s+/)) if (w) out.push(`w=${w}`);
  } else if (/^[INBR]=/.test(tok) || tok.endsWith("{") || tok.startsWith('"')) {
    // identifiers/keys, numbers, regexps, node types, operators and kinds
    out.push(tok);
  }
  // everything else is punctuation, field labels or flag values
}

/** 5-token shingles of a file's content tokens. */
export function contentShingles(toks: string[]): Set<string> {
  const c: string[] = [];
  for (const tok of toks) contentOf(tok, c);
  const out = new Set<string>();
  if (c.length < SHINGLE) {
    out.add(c.join("\u0000"));
    return out;
  }
  for (let i = 0; i + SHINGLE <= c.length; i++) {
    out.add(c.slice(i, i + SHINGLE).join("\u0000"));
  }
  return out;
}

function jaccard(a: Set<string>, b: Set<string>): number {
  const [small, large] = a.size <= b.size ? [a, b] : [b, a];
  let inter = 0;
  for (const x of small) if (large.has(x)) inter++;
  const union = a.size + b.size - inter;
  return union === 0 ? 0 : inter / union;
}

/**
 * Jaccard can never exceed min/max of the set sizes, so a pair whose bound
 * is under this is not worth intersecting; its bound stands in for its score
 * (an OVER-estimate, so it can only shrink a margin, never invent a pair).
 */
const SKIP_BOUND = 0.1;

interface ScoredPair {
  prior: string;
  fresh: string;
  score: number;
  runnerUp: number;
}

export interface PairingOptions {
  minScore?: number;
  minMargin?: number;
  /** Files with fewer content shingles than this never pair. */
  minShingles?: number;
}

interface Entry {
  shingles: Set<string>;
  pairable: boolean;
}

/**
 * Mutual-best pairing of removed (prior) to added (fresh) files.
 *
 * `contenders` are files whose content changed but whose path survived (a
 * same-path real change). They are never paired, but they COMPETE: a fresh
 * file whose best match is a contender has a better explanation than any
 * removed file, so it makes no pair. Without them a path SWAP misleads —
 * measured on candidate-f616f33b 197->198, where the IRPF90 grammar moved
 * to a new path while the Fortran grammar took its old one, and the
 * removed Fortran file (0.66 similar to IRPF90) was paired to the moved
 * IRPF90 one.
 *
 * Returns every pair made, sorted by fresh path. Deterministic; ties never
 * pair.
 */
export function pairRelocated(
  removed: Map<string, Set<string>>,
  added: Map<string, Set<string>>,
  opts: PairingOptions = {},
  contenders: {
    prior?: Map<string, Set<string>>;
    fresh?: Map<string, Set<string>>;
  } = {}
): ScoredPair[] {
  const minScore = opts.minScore ?? MIN_SCORE;
  const minMargin = opts.minMargin ?? MIN_MARGIN;
  const minShingles = opts.minShingles ?? MIN_SHINGLES;
  // `+path` may pair; `=path` only competes.
  const side = (
    pairable: Map<string, Set<string>>,
    others: Map<string, Set<string>> | undefined
  ): Map<string, Entry> => {
    const out = new Map<string, Entry>();
    for (const [k, sh] of pairable) {
      out.set(`+${k}`, { shingles: sh, pairable: sh.size >= minShingles });
    }
    for (const [k, sh] of others ?? []) {
      out.set(`=${k}`, { shingles: sh, pairable: false });
    }
    return out;
  };
  const prior = side(removed, contenders.prior);
  const fresh = side(added, contenders.fresh);
  const priorKeys = [...prior.keys()].sort();
  const freshKeys = [...fresh.keys()].sort();

  const memo = new Map<string, number>();
  const score = (p: string, f: string): number => {
    const k = `${p}\u0000${f}`;
    let s = memo.get(k);
    if (s === undefined) {
      s = similarity(
        (prior.get(p) as Entry).shingles,
        (fresh.get(f) as Entry).shingles
      );
      memo.set(k, s);
    }
    return s;
  };

  const out: ScoredPair[] = [];
  for (const f of freshKeys) {
    if (!fresh.get(f)?.pairable) continue;
    const ft = top2(priorKeys, (p) => score(p, f));
    if (ft.best < minScore || !prior.get(ft.bestOther)?.pairable) continue;
    const p = ft.bestOther;
    const pt = top2(freshKeys, (g) => score(p, g));
    if (pt.bestOther !== f) continue;
    const runnerUp = Math.max(ft.second, pt.second);
    if (ft.best - runnerUp < minMargin) continue;
    out.push({
      prior: p.slice(1),
      fresh: f.slice(1),
      score: ft.best,
      runnerUp
    });
  }
  return out;
}

/** Jaccard, or its size bound when that bound is too low to matter. */
function similarity(a: Set<string>, b: Set<string>): number {
  const bound = Math.min(a.size, b.size) / Math.max(a.size, b.size, 1);
  return bound < SKIP_BOUND ? bound : jaccard(a, b);
}

/** Best and second-best score over `others` (a tie makes second = best). */
function top2(others: string[], scoreOf: (o: string) => number) {
  let best = -1;
  let bestOther = "";
  let second = 0;
  for (const o of others) {
    const s = scoreOf(o);
    if (s > best) {
      second = Math.max(second, best);
      best = s;
      bestOther = o;
    } else {
      second = Math.max(second, s);
    }
  }
  return { best, bestOther, second };
}
