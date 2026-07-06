import type { FingerprintIndex, StructuralFeatures } from "./types.js";

export interface CloseMatchResult {
  /** oldId → newId for close matches */
  closeMatches: Map<string, string>;
  /** Similarity score for each close match */
  scores: Map<string, number>;
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
 * Finds close matches between unmatched old functions and unmatched new functions.
 * Uses cosine similarity on structural feature vectors.
 *
 * Each old function is matched to at most one new function (the best match above threshold).
 * Each new function is matched to at most one old function (greedy best-first).
 */
export function findCloseMatches(
  unmatchedOld: string[],
  unmatchedNew: string[],
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  options?: { threshold?: number }
): CloseMatchResult {
  const threshold = options?.threshold ?? 0.8;
  const closeMatches = new Map<string, string>();
  const scores = new Map<string, number>();

  if (unmatchedOld.length === 0 || unmatchedNew.length === 0) {
    return { closeMatches, scores };
  }

  const oldVectors = buildVectorMap(unmatchedOld, oldIndex);
  const newVectors = buildVectorMap(unmatchedNew, newIndex);

  const candidates = scorePairs(oldVectors, newVectors, threshold);
  assignGreedy(candidates, closeMatches, scores);

  return { closeMatches, scores };
}

function buildVectorMap(
  ids: string[],
  index: FingerprintIndex
): Map<string, FeatureVector> {
  const vectors = new Map<string, FeatureVector>();
  for (const id of ids) {
    const fp = index.fingerprints.get(id);
    if (!fp?.features) continue;
    const calleeCount = fp.calleeHashes?.length ?? fp.calleeShapes?.length ?? 0;
    vectors.set(id, computeFeatureVector(fp.features, calleeCount));
  }
  return vectors;
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
 * Greedy best-first assignment: each side matched at most once.
 */
function assignGreedy(
  candidates: Array<{ oldId: string; newId: string; score: number }>,
  closeMatches: Map<string, string>,
  scores: Map<string, number>
): void {
  candidates.sort((a, b) => b.score - a.score);
  const usedNew = new Set<string>();

  for (const { oldId, newId, score } of candidates) {
    if (closeMatches.has(oldId) || usedNew.has(newId)) continue;
    closeMatches.set(oldId, newId);
    scores.set(oldId, score);
    usedNew.add(newId);
  }
}
