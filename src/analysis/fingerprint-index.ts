import {
  buildBindingFullFingerprint,
  buildFullFingerprint,
  calleeShapesEqual,
  computeShingleSet,
  jaccardSimilarity
} from "./function-fingerprint.js";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import {
  analysisCacheForPath,
  analysisCacheForScope
} from "./analysis-cache.js";
import { hashPathWithMapping } from "./structural-hash.js";
import { type ExternalRefEvidence, propagate } from "./propagation.js";
import type {
  CalleeShape,
  FingerprintIndex,
  FunctionFingerprint,
  FunctionNode,
  MatchResult,
  ModuleBindingNode,
  ResolutionStats
} from "./types.js";

/**
 * Builds a fingerprint index from a function graph.
 * The index supports efficient lookup for multi-resolution matching.
 */
export function buildFingerprintIndex(
  functions: Map<string, FunctionNode>
): FingerprintIndex {
  const index: FingerprintIndex = {
    byStructuralHash: new Map(),
    fingerprints: new Map(),
    functions
  };

  for (const [sessionId, fn] of functions) {
    // Build full fingerprint with callee information
    const fingerprint = buildFullFingerprint(fn, functions);
    index.fingerprints.set(sessionId, fingerprint);

    // Index by structuralHash (uniqueHash lookup)
    const structuralHashList =
      index.byStructuralHash.get(fingerprint.structuralHash) ?? [];
    structuralHashList.push(sessionId);
    index.byStructuralHash.set(fingerprint.structuralHash, structuralHashList);
  }

  return index;
}

/**
 * Builds a fingerprint index over module bindings so they can go through
 * the same matchFunctions() cascade as functions. Bindings whose init is
 * unhashable (null fingerprint) can never match across versions — they
 * are excluded.
 */
export function buildBindingFingerprintIndex(
  bindings: ModuleBindingNode[]
): FingerprintIndex {
  const index: FingerprintIndex = {
    byStructuralHash: new Map(),
    fingerprints: new Map(),
    moduleBindings: new Map(bindings.map((b) => [b.sessionId, b]))
  };

  for (const binding of bindings) {
    if (!binding.fingerprint) continue;
    const fingerprint = buildBindingFullFingerprint(binding);
    index.fingerprints.set(binding.sessionId, fingerprint);

    const list = index.byStructuralHash.get(fingerprint.structuralHash) ?? [];
    list.push(binding.sessionId);
    index.byStructuralHash.set(fingerprint.structuralHash, list);
  }

  return index;
}

function filterByMemberKey(
  candidates: string[],
  oldKey: string | undefined,
  newIndex: FingerprintIndex
): string[] {
  if (oldKey === undefined) return candidates;
  // May return [] — every candidate contradicting the old memberKey is a
  // contradiction the caller must stop on, not fall through.
  return candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    return newFp?.memberKey === oldKey;
  });
}

function filterByCalleeShapes(
  candidates: string[],
  oldShapes: CalleeShape[],
  newIndex: FingerprintIndex
): string[] {
  return candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    if (!newFp) return false;
    return calleeShapesEqual(oldShapes, newFp.calleeShapes ?? []);
  });
}

function filterByCallerShapes(
  candidates: string[],
  oldShapes: CalleeShape[],
  newIndex: FingerprintIndex
): string[] {
  return candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    if (!newFp) return false;
    return calleeShapesEqual(oldShapes, newFp.callerShapes ?? []);
  });
}

function filterByCalleeHashes(
  candidates: string[],
  oldHashes: string[],
  newIndex: FingerprintIndex
): string[] {
  return candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    if (!newFp) return false;
    return arraysEqual(oldHashes, newFp.calleeHashes ?? []);
  });
}

function filterByTwoHopShapes(
  candidates: string[],
  oldShapes: string[],
  newIndex: FingerprintIndex
): string[] {
  return candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    if (!newFp) return false;
    return arraysEqual(oldShapes, newFp.twoHopShapes ?? []);
  });
}

/** Which cascade stage produced a match */
type Resolution =
  | "identity"
  | "memberKey"
  | "enclosingStatement"
  | "calleeShapes"
  | "callerShapes"
  | "calleeHashes"
  | "twoHopShapes"
  | "shingleSimilarity"
  | "ambiguous";

/** Minimum Jaccard similarity to accept a shingling tiebreaker */
const SHINGLE_THRESHOLD = 0.5;

/**
 * Try to resolve ambiguity via shingling: compute Jaccard similarity between
 * the old function's shingle set and each candidate's. Pick the best match
 * if it exceeds the threshold and is clearly better than the runner-up.
 */
function tryShingleResolve(
  oldId: string,
  candidates: string[],
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): string | null {
  const oldFn = oldIndex.functions?.get(oldId);
  if (!oldFn || !newIndex.functions) return null;

  const oldShingles = computeShingleSet(oldFn);
  if (oldShingles.size === 0) return null;

  let bestId: string | null = null;
  let bestSim = -1;
  let secondBestSim = -1;

  for (const candId of candidates) {
    const candFn = newIndex.functions.get(candId);
    if (!candFn) continue;

    const sim = jaccardSimilarity(oldShingles, computeShingleSet(candFn));
    if (sim > bestSim) {
      secondBestSim = bestSim;
      bestSim = sim;
      bestId = candId;
    } else if (sim > secondBestSim) {
      secondBestSim = sim;
    }
  }

  // Accept if above threshold and clearly better than runner-up
  if (bestId && bestSim >= SHINGLE_THRESHOLD && bestSim > secondBestSim) {
    return bestId;
  }
  return null;
}

/**
 * Resolves the best match from candidates using the disambiguation cascade.
 * Returns which cascade stage resolved the match, or "ambiguous".
 */
/**
 * Try calleeHash cascade: exact callee hashes, then two-hop shapes.
 * Returns [matchedId, resolution] or null if still ambiguous.
 */
type CalleeHashOutcome =
  | { kind: "match"; id: string; resolution: "calleeHashes" | "twoHopShapes" }
  /** A stage filtered every candidate out — stop, don't widen. */
  | { kind: "contradiction" }
  /** Narrowed (or unchanged) but still >1 candidates. */
  | { kind: "ambiguous"; pool: string[] };

function tryCalleeHashCascade(
  candidates: string[],
  oldFp: FunctionFingerprint,
  newIndex: FingerprintIndex
): CalleeHashOutcome {
  const calleeHashCandidates = filterByCalleeHashes(
    candidates,
    oldFp.calleeHashes ?? [],
    newIndex
  );

  if (calleeHashCandidates.length === 0) return { kind: "contradiction" };
  if (calleeHashCandidates.length === 1)
    return {
      kind: "match",
      id: calleeHashCandidates[0],
      resolution: "calleeHashes"
    };

  const twoHopCandidates = filterByTwoHopShapes(
    calleeHashCandidates,
    oldFp.twoHopShapes ?? [],
    newIndex
  );
  if (twoHopCandidates.length === 0) return { kind: "contradiction" };
  if (twoHopCandidates.length === 1)
    return {
      kind: "match",
      id: twoHopCandidates[0],
      resolution: "twoHopShapes"
    };

  return { kind: "ambiguous", pool: twoHopCandidates };
}

/** Run the caller-supplied identity resolver; record the match when unique. */
function tryIdentityResolve(
  oldId: string,
  candidates: string[],
  resolver:
    | ((oldId: string, candidates: string[]) => string | null)
    | undefined,
  matches: Map<string, string>
): boolean {
  if (!resolver) return false;
  const resolved = resolver(oldId, candidates);
  if (resolved && candidates.includes(resolved)) {
    matches.set(oldId, resolved);
    return true;
  }
  return false;
}

/** Enclosing statements above this loc span carry too much unrelated code
 *  (and cost too much to hash) to serve as identity evidence. */
const MAX_ENCLOSING_STMT_LINES = 50;

/**
 * Rename-invariant hash of a function's ENCLOSING statement, cached on the
 * index. null when there is no usable statement: the function IS the
 * statement (a declaration — zero added context), the statement is huge,
 * or hashing fails.
 */
function getEnclosingStmtHash(
  sessionId: string,
  index: FingerprintIndex
): string | null {
  index.enclosingStmtHashCache ??= new Map();
  const cache = index.enclosingStmtHashCache;
  const cached = cache.get(sessionId);
  if (cached !== undefined) return cached;

  const compute = (): string | null => {
    const fn = index.functions?.get(sessionId);
    if (fn) {
      const stmt = fn.path.getStatementParent();
      if (!stmt || stmt.node === fn.path.node) return null;
      // Several bucket members can share one enclosing statement (multiple
      // arrows in one options object) — memoize per node in the owning
      // AST's cache.
      return hashStatementPath(
        stmt,
        analysisCacheForPath(fn.path).stmtHashByNode
      );
    }
    const mb = index.moduleBindings?.get(sessionId);
    if (mb) return bindingNeighborContextHash(mb);
    return null;
  };

  const value = compute();
  cache.set(sessionId, value);
  return value;
}

/** Hash one statement path with the shared caps and per-AST node cache. */
function hashStatementPath(
  stmt: NodePath | null,
  stmtHashByNode: Map<t.Node, string>
): string | null {
  const node = stmt?.node;
  if (!stmt || !node) return null;
  const loc = node.loc;
  if (!loc || loc.end.line - loc.start.line + 1 > MAX_ENCLOSING_STMT_LINES) {
    return null;
  }
  const known = stmtHashByNode.get(node);
  if (known !== undefined) return known;
  try {
    const { hash } = hashPathWithMapping(stmt);
    stmtHashByNode.set(node, hash);
    return hash;
  } catch {
    return null;
  }
}

/**
 * Context hash for a module binding: the NEIGHBORING statements of its
 * declaration. The declaration itself is the clone (slots normalize the
 * only distinguishing part of `var X = lazy(() => { a = b; })`), so the
 * statements around it carry the identity — exactly like a function's
 * enclosing statement does.
 */
function bindingNeighborContextHash(mb: ModuleBindingNode): string | null {
  const bindingPath = mb.scope.getBinding(mb.name)?.path;
  const stmt = bindingPath?.getStatementParent();
  if (!stmt) return null;
  const stmtHashes = analysisCacheForScope(mb.scope).stmtHashByNode;
  const prev = hashStatementPath(stmt.getPrevSibling(), stmtHashes);
  const next = hashStatementPath(stmt.getNextSibling(), stmtHashes);
  if (prev === null && next === null) return null;
  return `${prev ?? "^"}|${next ?? "$"}`;
}

/** Numeric (line, col) source position from a `file:line:col` sessionId. */
function sessionPosition(sessionId: string): [number, number] {
  const parts = sessionId.split(":");
  return [Number(parts[1]) || 0, Number(parts[2]) || 0];
}

function bySessionPosition(a: string, b: string): number {
  const [al, ac] = sessionPosition(a);
  const [bl, bc] = sessionPosition(b);
  return al - bl || ac - bc;
}

/**
 * Resolve a bucket member by its enclosing statement's rename-invariant
 * hash: structurally identical clones (identity arrows, thunks) carry no
 * internal identity, but the statement AROUND them often does. RESOLVER
 * semantics, not a filter — an enclosing statement legitimately drifts
 * between versions, so no-holder just falls through to the next stage.
 *
 * Unique on both sides → the 1:1 claim. EQUAL counts above one → the
 * members are semantically interchangeable (identical function,
 * identical context), so any bijection is correct and only determinism
 * matters: pair by source ordinal, the same rule statement-align applies
 * to equal-count same-hash statement groups. Unequal counts (an inserted
 * or removed clone) stay ambiguous.
 */
function tryEnclosingStatementResolve(
  oldId: string,
  candidates: string[],
  oldFp: FunctionFingerprint,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): string | null {
  const hash = getEnclosingStmtHash(oldId, oldIndex);
  if (!hash) return null;

  const oldBucket = oldIndex.byStructuralHash.get(oldFp.structuralHash) ?? [];
  const newBucket = newIndex.byStructuralHash.get(oldFp.structuralHash) ?? [];
  const oldHolders = oldBucket.filter(
    (id) => getEnclosingStmtHash(id, oldIndex) === hash
  );
  // The ordinal frame is bucket-level (not candidate-level) so every old
  // member of the group computes the SAME bijection regardless of its own
  // upstream candidate filtering.
  const newHolders = newBucket.filter(
    (id) => getEnclosingStmtHash(id, newIndex) === hash
  );
  if (newHolders.length === 0) return null;
  if (oldHolders.length !== newHolders.length) return null;

  oldHolders.sort(bySessionPosition);
  newHolders.sort(bySessionPosition);
  const match = newHolders[oldHolders.indexOf(oldId)];
  // A member filtered from THIS old's candidates was rejected by stronger
  // evidence upstream (memberKey contradiction) — never claim across it.
  return match !== undefined && candidates.includes(match) ? match : null;
}

function resolveMatch(
  oldId: string,
  candidates: string[],
  oldFp: FunctionFingerprint,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  matches: Map<string, string>,
  ambiguous: Map<string, string[]>,
  maxCascadeDepth: number,
  resolveAmbiguousCandidate?: (
    oldId: string,
    candidates: string[]
  ) => string | null
): Resolution {
  // Caller-supplied identity resolution runs first — it carries evidence
  // the fingerprint fields cannot (e.g. correspondence under an existing
  // match result), which stays discriminating even when candidates wrap
  // structurally identical code.
  if (
    tryIdentityResolve(oldId, candidates, resolveAmbiguousCandidate, matches)
  ) {
    return "identity";
  }

  // memberKey disambiguation: runs before callee shapes
  const mkCandidates = filterByMemberKey(candidates, oldFp.memberKey, newIndex);
  if (mkCandidates.length === 0) {
    // Contradiction: every candidate carries a different memberKey than
    // the old function. Stop — a candidate rejected by strong evidence
    // must not win at a weaker stage.
    ambiguous.set(oldId, candidates);
    return "ambiguous";
  }
  if (mkCandidates.length === 1) {
    matches.set(oldId, mkCandidates[0]);
    return "memberKey";
  }

  const stmtMatch = tryEnclosingStatementResolve(
    oldId,
    mkCandidates,
    oldFp,
    oldIndex,
    newIndex
  );
  if (stmtMatch) {
    matches.set(oldId, stmtMatch);
    return "enclosingStatement";
  }

  if (maxCascadeDepth < 1) {
    ambiguous.set(oldId, mkCandidates);
    return "ambiguous";
  }

  // calleeShapes: blurred callee structural shapes
  const calleeShapeCandidates = filterByCalleeShapes(
    mkCandidates,
    oldFp.calleeShapes ?? [],
    newIndex
  );
  if (calleeShapeCandidates.length === 0) {
    ambiguous.set(oldId, mkCandidates);
    return "ambiguous";
  }
  if (calleeShapeCandidates.length === 1) {
    matches.set(oldId, calleeShapeCandidates[0]);
    return "calleeShapes";
  }

  // callerShapes: blurred caller structural shapes (upstream context)
  const callerShapeCandidates = filterByCallerShapes(
    calleeShapeCandidates,
    oldFp.callerShapes ?? [],
    newIndex
  );
  if (callerShapeCandidates.length === 0) {
    ambiguous.set(oldId, calleeShapeCandidates);
    return "ambiguous";
  }
  if (callerShapeCandidates.length === 1) {
    matches.set(oldId, callerShapeCandidates[0]);
    return "callerShapes";
  }

  return resolveDeepStages(
    oldId,
    callerShapeCandidates,
    oldFp,
    oldIndex,
    newIndex,
    matches,
    ambiguous,
    maxCascadeDepth
  );
}

/** Final cascade stages: calleeHashes + twoHopShapes, then shingle tiebreak. */
function resolveDeepStages(
  oldId: string,
  candidates: string[],
  oldFp: FunctionFingerprint,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  matches: Map<string, string>,
  ambiguous: Map<string, string[]>,
  maxCascadeDepth: number
): Resolution {
  let pool = candidates;
  if (maxCascadeDepth >= 2) {
    const outcome = tryCalleeHashCascade(pool, oldFp, newIndex);
    if (outcome.kind === "match") {
      matches.set(oldId, outcome.id);
      return outcome.resolution;
    }
    if (outcome.kind === "contradiction") {
      ambiguous.set(oldId, pool);
      return "ambiguous";
    }
    pool = outcome.pool;
  }

  // shingleSimilarity: Jaccard similarity tiebreaker
  const shingleMatch = tryShingleResolve(oldId, pool, oldIndex, newIndex);
  if (shingleMatch) {
    matches.set(oldId, shingleMatch);
    return "shingleSimilarity";
  }

  // Still ambiguous
  ambiguous.set(oldId, pool);
  return "ambiguous";
}

/**
 * Options for controlling the matching cascade.
 */
export interface MatchOptions {
  /** Maximum cascade depth (0 = uniqueHash + memberKey only, 1 = also calleeShapes + callerShapes, 2 = also calleeHashes + twoHopShapes + shingle). Default: 2 */
  maxCascadeDepth?: 0 | 1 | 2;

  /** SessionIds to exclude from matching (e.g., Bun CJS wrapper functions that always change between versions) */
  excludeSessionIds?: Set<string>;

  /** Enable call-graph propagation to resolve ambiguous functions using confirmed matches as constraints. Default: false */
  enablePropagation?: boolean;

  /**
   * Caller-supplied disambiguator, tried before all fingerprint stages.
   * Given an ambiguous old-side id and its candidates, return the single
   * matching candidate or null. Used to resolve same-hash module bindings
   * by their correspondence under an existing function match result.
   */
  resolveAmbiguousCandidate?: (
    oldId: string,
    candidates: string[]
  ) => string | null;

  /**
   * Matched-binding reference evidence for propagation (see
   * ExternalRefEvidence). Lets a re-match round crack same-hash function
   * buckets whose only identity is which matched module binding they
   * reference. Only consulted when enablePropagation is set.
   */
  externalRefEvidence?: ExternalRefEvidence;
}

/**
 * Matches functions from an old version to a new version using a disambiguation cascade.
 *
 * Matching strategy:
 * 1. Try uniqueHash: exact localHash match
 * 2. If multiple candidates, try memberKey, then calleeShapes + callerShapes
 * 3. If still multiple, try calleeHashes + twoHopShapes + shingleSimilarity
 */
export function matchFunctions(
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  options?: MatchOptions
): MatchResult {
  const maxCascadeDepth = options?.maxCascadeDepth ?? 2;
  const excludeIds = options?.excludeSessionIds;
  const matches = new Map<string, string>();
  const ambiguous = new Map<string, string[]>();
  const unmatched: string[] = [];
  const stats: ResolutionStats = {
    structuralHashUnique: 0,
    identityResolved: 0,
    memberKeyResolved: 0,
    enclosingStatementResolved: 0,
    calleeShapesResolved: 0,
    callerShapesResolved: 0,
    calleeHashesResolved: 0,
    twoHopShapesResolved: 0,
    shingleSimilarityResolved: 0,
    ordinalResolved: 0,
    injectivityDemoted: 0,
    singletonRejected: 0,
    stillAmbiguous: 0,
    unmatched: 0,
    propagationResolved: 0
  };

  // Resolution stage per matched old id. Stats are accumulated only after
  // injectivity enforcement so demoted matches never count as resolved.
  const resolutions = new Map<string, MatchedResolution>();

  const state: MatchingState = {
    oldIndex,
    newIndex,
    maxCascadeDepth,
    excludeIds,
    resolveAmbiguousCandidate: options?.resolveAmbiguousCandidate,
    matches,
    ambiguous,
    unmatched,
    stats,
    resolutions
  };
  runMatchingPass(state);
  demoteNonInjectiveMatches(state);

  for (const oldId of matches.keys()) {
    const resolution = resolutions.get(oldId);
    if (resolution) stats[RESOLUTION_STAT_KEY[resolution]]++;
  }
  stats.stillAmbiguous = ambiguous.size;

  // Post-pass: call-graph propagation to resolve remaining ambiguity
  if (options?.enablePropagation && ambiguous.size > 0) {
    const { resolved } = propagate(matches, ambiguous, oldIndex, newIndex, {
      externalRefEvidence: options.externalRefEvidence
    });
    stats.propagationResolved = resolved;
    stats.stillAmbiguous -= resolved;
  }

  return { matches, ambiguous, unmatched, resolutionStats: stats };
}

/**
 * Final tie-break for buckets no evidence can crack: when the same
 * structural hash has an equal number of unmatched members on both sides
 * and every member carries identical distinguishing features (memberKey,
 * callee/caller shapes, callee hashes), the members are true twins — the
 * bodies are structurally identical, so any name assignment is
 * semantically valid and the only quality axis is cross-version
 * consistency. Pairing by source order is the stable choice; leaving the
 * bucket ambiguous re-rolls every twin's name through the LLM each hop
 * (observed as permuted/re-synonymed sibling predicates in adjacent-
 * version diffs). Runs AFTER binding alternation and propagation so no
 * genuine evidence is pre-empted; an unequal count (insertion into the
 * bucket) or any feature variation disables the bucket.
 */
export function resolveAmbiguousByOrdinal(
  matchResult: MatchResult,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): number {
  const { matches, ambiguous, resolutionStats } = matchResult;
  const matchedNew = new Set(matches.values());

  const hashes = new Set<string>();
  for (const oldId of ambiguous.keys()) {
    const hash = oldIndex.fingerprints.get(oldId)?.structuralHash;
    if (hash) hashes.add(hash);
  }

  let resolved = 0;
  for (const hash of hashes) {
    resolved += ordinalPairBucket(
      hash,
      matchResult,
      matchedNew,
      oldIndex,
      newIndex
    );
  }
  resolutionStats.ordinalResolved += resolved;
  resolutionStats.stillAmbiguous = ambiguous.size;
  return resolved;
}

/**
 * A certified interchangeable pool (exp036 task B): ambiguous priors
 * sharing one exact candidate set, reciprocal (equal counts, every
 * candidate unmatched), every member on BOTH sides carrying the same
 * non-null evidence key. Members that tie under every computable signal
 * are provably interchangeable — any consistent assignment is as correct
 * as any other, so the only quality axis left is cross-version
 * stability. This CERTIFIES who may enter a stable-assignment tier; it
 * assigns nothing itself. (The leftover-ordinal pairing that assigned
 * by fresh source order failed self-hop — pool composition depends on
 * earlier-tier outcomes — so the assignment must anchor on the PRIOR
 * side; see experiments/036-interchangeable-assignment.)
 */
export interface InterchangeablePool {
  /** Prior-side ids, session-position order (stable per artifact). */
  priors: string[];
  /** Fresh-side candidate ids, session-position order. */
  candidates: string[];
  /** The single evidence key every member shares. */
  evidenceKey: string;
}

export function certifyInterchangeablePools(
  matchResult: MatchResult,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): InterchangeablePool[] {
  const { matches, ambiguous } = matchResult;
  const matchedNew = new Set(matches.values());
  const byCandidates = groupPriorsByCandidateSet(ambiguous);
  const pools: InterchangeablePool[] = [];
  for (const [key, priors] of [...byCandidates.entries()].sort()) {
    const pool = certifyOnePool(
      key.split(","),
      priors,
      matchedNew,
      oldIndex,
      newIndex
    );
    if (pool) pools.push(pool);
  }
  return pools;
}

/** Group ambiguous priors by their sorted candidate-set key. */
function groupPriorsByCandidateSet(
  ambiguous: Map<string, string[]>
): Map<string, string[]> {
  const byCandidates = new Map<string, string[]>();
  for (const [oldId, candidates] of ambiguous) {
    const key = [...candidates].sort().join(",");
    let list = byCandidates.get(key);
    if (!list) {
      list = [];
      byCandidates.set(key, list);
    }
    list.push(oldId);
  }
  return byCandidates;
}

/** The certificate gates for one pool; null when any gate fails. */
function certifyOnePool(
  candidates: string[],
  priors: string[],
  matchedNew: Set<string>,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): InterchangeablePool | null {
  if (priors.length !== candidates.length) return null;
  if (candidates.some((id) => matchedNew.has(id))) return null;
  const keys = new Set<string | null>();
  for (const id of priors) keys.add(evidenceKey(oldIndex, id));
  for (const id of candidates) keys.add(evidenceKey(newIndex, id));
  if (keys.size !== 1) return null;
  const [only] = keys;
  if (only === null || only === undefined) return null;
  return {
    priors: [...priors].sort(bySessionPosition),
    candidates: [...candidates].sort(bySessionPosition),
    evidenceKey: only
  };
}

/** Distinguishing-feature vector of one fingerprint, or null when absent. */
function evidenceKey(index: FingerprintIndex, id: string): string | null {
  const fp = index.fingerprints.get(id);
  if (!fp) return null;
  return JSON.stringify([
    fp.memberKey ?? null,
    fp.calleeShapes ?? [],
    fp.callerShapes ?? [],
    fp.calleeHashes ?? []
  ]);
}

/** Pair one bucket by source order when all ordinal gates hold. */
function ordinalPairBucket(
  hash: string,
  matchResult: MatchResult,
  matchedNew: Set<string>,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): number {
  const { matches, ambiguous } = matchResult;
  const oldBucket = oldIndex.byStructuralHash.get(hash) ?? [];
  const newBucket = newIndex.byStructuralHash.get(hash) ?? [];
  if (oldBucket.length === 0 || oldBucket.length !== newBucket.length) {
    return 0;
  }
  // Every member must still be undecided on both sides — a partially
  // matched bucket means evidence existed for someone, and ordinal
  // pairing of the remainder would shift against it.
  if (oldBucket.some((id) => matches.has(id) || !ambiguous.has(id))) return 0;
  if (newBucket.some((id) => matchedNew.has(id))) return 0;

  const keys = new Set<string | null>();
  for (const id of oldBucket) keys.add(evidenceKey(oldIndex, id));
  for (const id of newBucket) keys.add(evidenceKey(newIndex, id));
  if (keys.size !== 1 || keys.has(null)) return 0;

  const oldOrdered = [...oldBucket].sort(bySessionPosition);
  const newOrdered = [...newBucket].sort(bySessionPosition);
  for (let i = 0; i < oldOrdered.length; i++) {
    matches.set(oldOrdered[i], newOrdered[i]);
    ambiguous.delete(oldOrdered[i]);
    matchedNew.add(newOrdered[i]);
  }
  return oldOrdered.length;
}

/** Resolution stages that produce a match (everything but "ambiguous"). */
type MatchedResolution =
  | Exclude<Resolution, "ambiguous">
  | "structuralHashUnique";

interface MatchingState {
  oldIndex: FingerprintIndex;
  newIndex: FingerprintIndex;
  maxCascadeDepth: number;
  excludeIds?: Set<string>;
  resolveAmbiguousCandidate?: (
    oldId: string,
    candidates: string[]
  ) => string | null;
  matches: Map<string, string>;
  ambiguous: Map<string, string[]>;
  unmatched: string[];
  stats: ResolutionStats;
  resolutions: Map<string, MatchedResolution>;
}

/** Stats field incremented for each resolution stage. */
const RESOLUTION_STAT_KEY: Record<MatchedResolution, keyof ResolutionStats> = {
  structuralHashUnique: "structuralHashUnique",
  identity: "identityResolved",
  memberKey: "memberKeyResolved",
  enclosingStatement: "enclosingStatementResolved",
  calleeShapes: "calleeShapesResolved",
  callerShapes: "callerShapesResolved",
  calleeHashes: "calleeHashesResolved",
  twoHopShapes: "twoHopShapesResolved",
  shingleSimilarity: "shingleSimilarityResolved"
};

/**
 * Contradiction check for zero-corroboration singleton accepts, using only
 * version-stable signals: memberKey (the property key a function is
 * assigned to — hash-external context), propertyAccesses, and
 * externalCalls (known globals / method names — never minified binding
 * names). A signal absent on either side is missing evidence, not an
 * opposing signal; only explicit disagreement rejects.
 */
function singletonContradicts(
  oldFp: FunctionFingerprint,
  newFp: FunctionFingerprint | undefined
): boolean {
  if (!newFp) return false;
  if (
    oldFp.memberKey !== undefined &&
    newFp.memberKey !== undefined &&
    oldFp.memberKey !== newFp.memberKey
  ) {
    return true;
  }
  const oldFeatures = oldFp.features;
  const newFeatures = newFp.features;
  if (!oldFeatures || !newFeatures) return false;
  return (
    !arraysEqual(oldFeatures.propertyAccesses, newFeatures.propertyAccesses) ||
    !arraysEqual(oldFeatures.externalCalls, newFeatures.externalCalls)
  );
}

/** New-side hash-bucket candidates for an old fingerprint, minus exclusions. */
function candidatesForHash(
  structuralHash: string,
  newIndex: FingerprintIndex,
  excludeIds?: Set<string>
): string[] {
  const raw = newIndex.byStructuralHash.get(structuralHash) ?? [];
  return excludeIds ? raw.filter((id) => !excludeIds.has(id)) : raw;
}

/** The per-old-id matching loop: uniqueHash accept or disambiguation cascade. */
function runMatchingPass(state: MatchingState): void {
  for (const [oldId, oldFp] of state.oldIndex.fingerprints) {
    // Skip excluded functions (e.g., Bun CJS wrapper)
    if (state.excludeIds?.has(oldId)) continue;

    const candidates = candidatesForHash(
      oldFp.structuralHash,
      state.newIndex,
      state.excludeIds
    );

    if (candidates.length === 0) {
      state.unmatched.push(oldId);
      state.stats.unmatched++;
      continue;
    }

    if (candidates.length === 1) {
      // A singleton bucket matches with zero cascade corroboration —
      // exactly where a deleted helper and an unrelated added helper
      // auto-match. Reject when a version-stable signal contradicts.
      if (
        singletonContradicts(
          oldFp,
          state.newIndex.fingerprints.get(candidates[0])
        )
      ) {
        state.unmatched.push(oldId);
        state.stats.unmatched++;
        state.stats.singletonRejected++;
        continue;
      }
      state.matches.set(oldId, candidates[0]);
      state.resolutions.set(oldId, "structuralHashUnique");
      continue;
    }

    // Multiple candidates — use disambiguation cascade
    const resolution = resolveMatch(
      oldId,
      candidates,
      oldFp,
      state.oldIndex,
      state.newIndex,
      state.matches,
      state.ambiguous,
      state.maxCascadeDepth,
      state.resolveAmbiguousCandidate
    );
    if (resolution !== "ambiguous") state.resolutions.set(oldId, resolution);
  }
}

/**
 * Enforces injectivity: a new-side function claimed by more than one
 * old-side function is a contradiction — at most one claimant can be the
 * true origin, and picking one by iteration order would silently transfer
 * wrong names. Demote every claimant back to ambiguous with its full
 * hash-bucket candidate list; propagation may later re-resolve claimants
 * that have positive call-graph evidence.
 */
function demoteNonInjectiveMatches(state: MatchingState): void {
  const claimants = new Map<string, string[]>();
  for (const [oldId, newId] of state.matches) {
    const list = claimants.get(newId) ?? [];
    list.push(oldId);
    claimants.set(newId, list);
  }

  for (const [, oldIds] of claimants) {
    if (oldIds.length <= 1) continue;
    for (const oldId of oldIds) {
      state.matches.delete(oldId);
      const oldFp = state.oldIndex.fingerprints.get(oldId);
      const candidates = oldFp
        ? candidatesForHash(
            oldFp.structuralHash,
            state.newIndex,
            state.excludeIds
          )
        : [];
      state.ambiguous.set(oldId, candidates);
      state.stats.injectivityDemoted++;
    }
  }
}

/**
 * Simple array equality check for sorted string arrays.
 */
function arraysEqual(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((val, i) => val === b[i]);
}

/**
 * Statistics about matching results. Not used by the production pipeline
 * — kept for the experiment harnesses (experiments/012's analyze scripts).
 */
export function getMatchStats(result: MatchResult): {
  matched: number;
  ambiguous: number;
  unmatched: number;
  total: number;
  matchRate: number;
} {
  const matched = result.matches.size;
  const ambiguous = result.ambiguous.size;
  const unmatched = result.unmatched.length;
  const total = matched + ambiguous + unmatched;
  const matchRate = total > 0 ? matched / total : 0;

  return { matched, ambiguous, unmatched, total, matchRate };
}

/**
 * Functions in the new index with no match in the old index — likely
 * added in this version. Not used by the production pipeline — kept for
 * the e2e harness (test/e2e/harness/validate.ts).
 */
export function findNewFunctions(
  _oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  matchResult: MatchResult
): string[] {
  const matchedNewIds = new Set(matchResult.matches.values());
  for (const candidates of matchResult.ambiguous.values()) {
    for (const c of candidates) {
      matchedNewIds.add(c);
    }
  }

  const newFunctions: string[] = [];
  for (const newId of newIndex.fingerprints.keys()) {
    if (!matchedNewIds.has(newId)) {
      newFunctions.push(newId);
    }
  }

  return newFunctions;
}
