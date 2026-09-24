import type { FingerprintIndex, StructuralFeatures } from "./types.js";
import { fingerprintFeatures } from "./types.js";

export interface CloseMatchResult {
  /** oldId → newId for close matches */
  closeMatches: Map<string, string>;
  /** Similarity score for each close match */
  scores: Map<string, number>;
  /**
   * Ids that could not be scored at all because their fingerprint carries no
   * `features` — NOT ids that were scored and found dissimilar.
   *
   * Without these, an empty `closeMatches` has two very different causes that
   * look identical: "these are genuinely not close" and "nothing here was
   * eligible in the first place". Every fingerprint from
   * `buildBindingFullFingerprint` is in the second category, so a caller that
   * starts passing binding ids gets a silent no-op. That is the same shape as
   * the dead `singletonContradicts` guard, which reported 11,094
   * zero-corroboration accepts as `singletonRejected: 0`.
   */
  skippedOld: number;
  skippedNew: number;
}

/**
 * Fixed-length numeric feature vector for fuzzy function comparison.
 * DECKARD-inspired: these features capture function shape without
 * being sensitive to identifier renaming.
 */
interface FeatureVector {
  arity: number;
  complexity: number;
  returnCount: number;
  loopCount: number;
  branchCount: number;
  tryCount: number;
  calleeCount: number;
  externalCallCount: number;
  stringLiteralCount: number;
  propertyAccessCount: number;
  numericLiteralCount: number;
  hasRestParam: number; // 0 or 1
}

const FEATURE_KEYS: (keyof FeatureVector)[] = [
  "arity",
  "complexity",
  "returnCount",
  "loopCount",
  "branchCount",
  "tryCount",
  "calleeCount",
  "externalCallCount",
  "stringLiteralCount",
  "propertyAccessCount",
  "numericLiteralCount",
  "hasRestParam"
];

/**
 * Computes a numeric feature vector from a function's structural features
 * and fingerprint data.
 */
export function computeFeatureVector(
  features: StructuralFeatures,
  calleeCount: number
): FeatureVector {
  return {
    arity: features.arity,
    complexity: features.complexity,
    returnCount: features.returnCount,
    loopCount: features.loopCount,
    branchCount: features.branchCount,
    tryCount: features.tryCount,
    calleeCount,
    externalCallCount: features.externalCalls.length,
    stringLiteralCount: features.stringLiterals.length,
    propertyAccessCount: features.propertyAccesses.length,
    numericLiteralCount: features.numericLiterals.length,
    hasRestParam: features.hasRestParam ? 1 : 0
  };
}

/**
 * Cosine similarity between two feature vectors. Returns 0-1.
 */
function cosineSimilarity(a: FeatureVector, b: FeatureVector): number {
  let dotProduct = 0;
  let normA = 0;
  let normB = 0;

  for (const key of FEATURE_KEYS) {
    const va = a[key];
    const vb = b[key];
    dotProduct += va * vb;
    normA += va * va;
    normB += vb * vb;
  }

  if (normA === 0 || normB === 0) return 0;
  return dotProduct / (Math.sqrt(normA) * Math.sqrt(normB));
}

/**
 * Observation sink for the artifact dump (WP2.2's gate): the tier's
 * candidate list and its assignment, handed back untouched. The dump's
 * recorder derives per-candidate outcomes (won / abstained-taken /
 * abstained-tie) from these two with `deriveCloseAssignmentEvents` —
 * derivation, not instrumentation, so this needs no hook inside the
 * greedy loop. Filled by `findCloseMatches` when `options.trace` is
 * passed; purely write-only from this side.
 */
export interface CloseMatchTrace {
  /** `scorePairs`' output, in its iteration order (pre-sort). */
  candidates: Array<{ oldId: string; newId: string; score: number }>;
  /** The assignment: oldId → newId, in decision order. */
  closeMatches: Map<string, string>;
  /** The assigned scores: oldId → score. */
  scores: Map<string, number>;
}

/**
 * Finds close matches between unmatched old functions and unmatched new functions.
 * Uses cosine similarity on structural feature vectors.
 *
 * Each old function is matched to at most one new function (the best match above threshold).
 * Each new function is matched to at most one old function (greedy best-first).
 *
 * `options.trace` (dump instrumentation, armed-only): filled with the
 * candidate list and the assignment before returning. Reading it changes
 * nothing — the tier's decisions are made before the sink is touched.
 */
export function findCloseMatches(
  unmatchedOld: string[],
  unmatchedNew: string[],
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  options?: { threshold?: number; trace?: CloseMatchTrace }
): CloseMatchResult {
  const threshold = options?.threshold ?? 0.8;
  const closeMatches = new Map<string, string>();
  const scores = new Map<string, number>();

  if (unmatchedOld.length === 0 || unmatchedNew.length === 0) {
    if (options?.trace) {
      options.trace.candidates = [];
      options.trace.closeMatches = closeMatches;
      options.trace.scores = scores;
    }
    return { closeMatches, scores, skippedOld: 0, skippedNew: 0 };
  }

  const old = buildVectorMap(unmatchedOld, oldIndex);
  const fresh = buildVectorMap(unmatchedNew, newIndex);

  const candidates = scorePairs(old.vectors, fresh.vectors, threshold);
  assignGreedy(candidates, closeMatches, scores);
  if (options?.trace) {
    options.trace.candidates = candidates;
    options.trace.closeMatches = closeMatches;
    options.trace.scores = scores;
  }

  return {
    closeMatches,
    scores,
    skippedOld: old.skipped,
    skippedNew: fresh.skipped
  };
}

/**
 * Vectors for the ids that can be scored, plus a count of the ids that
 * cannot. The count is returned rather than swallowed because the two are
 * not the same fact — see `CloseMatchResult.skippedOld`.
 */
function buildVectorMap(
  ids: string[],
  index: FingerprintIndex
): { vectors: Map<string, FeatureVector>; skipped: number } {
  const vectors = new Map<string, FeatureVector>();
  let skipped = 0;
  for (const id of ids) {
    const fp = index.fingerprints.get(id);
    const features = fp ? fingerprintFeatures(fp) : undefined;
    if (!fp || !features) {
      skipped++;
      continue;
    }
    const calleeCount = fp.calleeHashes?.length ?? fp.calleeShapes?.length ?? 0;
    vectors.set(id, computeFeatureVector(features, calleeCount));
  }
  return { vectors, skipped };
}

/**
 * Max candidates kept per old function. Bounds the pair matrix to
 * O(old × K) — unbounded, ~8K×8K unmatched functions on a real bundle
 * materialize tens of millions of pairs. Greedy assignment only ever
 * gives an old function a candidate it ranked highly, so dropping its
 * 4th-best and beyond loses at most weak tail assignments.
 */
export const CLOSE_MATCH_TOP_K = 3;

/**
 * Computes cosine similarity for (old, new) pairs above threshold,
 * keeping the top-K per old function. Exported for tests.
 */
export function scorePairs(
  oldVectors: Map<string, FeatureVector>,
  newVectors: Map<string, FeatureVector>,
  threshold: number
): Array<{ oldId: string; newId: string; score: number }> {
  const candidates: Array<{ oldId: string; newId: string; score: number }> = [];

  for (const [oldId, oldVec] of oldVectors) {
    const top: Array<{ oldId: string; newId: string; score: number }> = [];
    for (const [newId, newVec] of newVectors) {
      const score = cosineSimilarity(oldVec, newVec);
      if (score < threshold) continue;
      insertTopK(top, { oldId, newId, score });
    }
    candidates.push(...top);
  }

  return candidates;
}

/** Insert into a descending-sorted list capped at CLOSE_MATCH_TOP_K. */
function insertTopK(
  list: Array<{ oldId: string; newId: string; score: number }>,
  candidate: { oldId: string; newId: string; score: number }
): void {
  let i = list.length;
  while (i > 0 && list[i - 1].score < candidate.score) i--;
  list.splice(i, 0, candidate);
  if (list.length > CLOSE_MATCH_TOP_K) list.pop();
}

/**
 * Greedy best-first assignment: each side matched at most once, and a
 * pair that TIES with another still-available pair sharing either
 * endpoint abstains — every cascade tier requires best > second-best,
 * and this tier used to resolve an exact tie by Map insertion order,
 * which cross-version can cross-pair same-shaped siblings and present
 * the coin flip as a match. Equal-score pairs with disjoint endpoints
 * are not in contention and still match.
 */
function assignGreedy(
  candidates: Array<{ oldId: string; newId: string; score: number }>,
  closeMatches: Map<string, string>,
  scores: Map<string, number>
): void {
  candidates.sort((a, b) => b.score - a.score);
  const usedNew = new Set<string>();

  for (let i = 0; i < candidates.length; i++) {
    const { oldId, newId, score } = candidates[i];
    if (closeMatches.has(oldId) || usedNew.has(newId)) continue;
    if (tiedRival(candidates, i, closeMatches, usedNew)) continue;
    closeMatches.set(oldId, newId);
    scores.set(oldId, score);
    usedNew.add(newId);
  }
}

/** An UNSPENT equal-score pair sharing an endpoint with candidates[i].
 * Scanned both directions: an earlier rival that abstained because of
 * THIS pair must make this pair abstain too (mutual). */
function tiedRival(
  candidates: Array<{ oldId: string; newId: string; score: number }>,
  i: number,
  closeMatches: Map<string, string>,
  usedNew: Set<string>
): boolean {
  const c = candidates[i];
  const contends = (r: (typeof candidates)[number]) =>
    !closeMatches.has(r.oldId) &&
    !usedNew.has(r.newId) &&
    (r.oldId === c.oldId || r.newId === c.newId);
  for (let j = i + 1; j < candidates.length; j++) {
    if (candidates[j].score !== c.score) break;
    if (contends(candidates[j])) return true;
  }
  for (let j = i - 1; j >= 0; j--) {
    if (candidates[j].score !== c.score) break;
    if (contends(candidates[j])) return true;
  }
  return false;
}

// ---------------------------------------------------------------------------
// Assignment outcomes — the dump's per-candidate verdicts (WP2.2's gate).
// The Rust mirror of this function is matching::close_dump's
// `derive_close_assignment_events` — keep the two in sync (a drift shows
// up as the gate reporting outcomes the assignments contradict; the dump
// asserts the won set against the real assignment to catch that).
// ---------------------------------------------------------------------------

/** One candidate's fate in the greedy assignment. */
export type CloseAssignmentOutcome =
  | "won"
  | "abstained:taken"
  | "abstained:tie";

/** One candidate, its 1-based rank in the decision order, and its fate. */
export interface CloseAssignmentEvent {
  candidate: { oldId: string; newId: string; score: number };
  rank: number;
  outcome: CloseAssignmentOutcome;
}

/**
 * Derives every candidate's outcome from the trace's two facts — the
 * candidate list and the won set — without re-running the greedy loop.
 * The derivation is exact: the assignment marks an endpoint used ONLY
 * when it wins a pair, so replaying the decision order against the won
 * set reproduces each skip's cause. A candidate whose endpoints are both
 * still free and that is not in the won set is exactly one the tie rule
 * abstained (the only other reason to skip is a taken endpoint, which
 * the replay observes directly).
 */
export function deriveCloseAssignmentEvents(
  candidates: Array<{ oldId: string; newId: string; score: number }>,
  closeMatches: ReadonlyMap<string, string>
): CloseAssignmentEvent[] {
  // The assignment's own sort: descending score, STABLE (ES2019) — the
  // decision order the ranks name.
  const sorted = [...candidates].sort((a, b) => b.score - a.score);
  const won = new Set(
    [...closeMatches].map(([oldId, newId]) => `${oldId}\u0000${newId}`)
  );
  const usedOld = new Set<string>();
  const usedNew = new Set<string>();
  const events: CloseAssignmentEvent[] = [];
  for (let i = 0; i < sorted.length; i++) {
    const candidate = sorted[i];
    const key = `${candidate.oldId}\u0000${candidate.newId}`;
    if (won.has(key)) {
      usedOld.add(candidate.oldId);
      usedNew.add(candidate.newId);
      events.push({ candidate, rank: i + 1, outcome: "won" });
    } else if (usedOld.has(candidate.oldId) || usedNew.has(candidate.newId)) {
      events.push({ candidate, rank: i + 1, outcome: "abstained:taken" });
    } else {
      events.push({ candidate, rank: i + 1, outcome: "abstained:tie" });
    }
  }
  return events;
}

/**
 * The f64's IEEE bits, hex — the dump's score identity column. A tie
 * abstention keys on EXACT float equality, so the gate compares the bits,
 * not the shortest-roundtrip decimal (which is lossless but opaque in a
 * diff).
 */
export function f64BitsHex(score: number): string {
  const buf = new ArrayBuffer(8);
  const view = new DataView(buf);
  view.setFloat64(0, score);
  return `0x${view.getBigUint64(0).toString(16)}`;
}
