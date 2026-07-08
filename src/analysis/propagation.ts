import type { FingerprintIndex, FunctionNode } from "./types.js";

/**
 * Module-binding reference evidence for cracking same-hash buckets whose
 * members have no call-graph or scope-parent evidence (module-scope
 * arrows like Bun's export getters): a function's identity is WHICH
 * matched binding it references. Ref maps cover the ambiguous functions
 * and their candidates; bindingMatches is the binding cascade's result.
 */
export interface ExternalRefEvidence {
  /** old fn sessionId → old-side module-binding sessionIds it references */
  oldRefs: Map<string, Set<string>>;
  /** new fn sessionId → new-side module-binding sessionIds it references */
  newRefs: Map<string, Set<string>>;
  /** old binding sessionId → new binding sessionId (confirmed matches) */
  bindingMatches: Map<string, string>;
}

export interface PropagationOptions {
  maxIterations?: number; // default: 10
  externalRefEvidence?: ExternalRefEvidence;
}

interface PropagationState {
  matches: Map<string, string>;
  reverseMatches: Map<string, string>;
  ambiguous: Map<string, string[]>;
  oldFunctions: Map<string, FunctionNode>;
  newFunctions: Map<string, FunctionNode>;
  newCallers: Map<string, Set<string>>;
  oldScopeChildren: Map<string, string[]>;
  newScopeChildren: Map<string, string[]>;
  externalRefEvidence?: ExternalRefEvidence;
}

/**
 * Iteratively propagates confirmed matches through the call graph to resolve
 * ambiguous functions. Uses three strategies in priority order:
 *
 * 1. Matched-callee: filter candidates to those calling the same matched callees
 * 2. Matched-caller: filter candidates to those called by the same matched callers
 * 3. Scope-parent: filter by matched enclosing function
 * 4. Scope-ordinal: match by position among same-hash siblings under matched parent
 *
 * Mutates `matches` and `ambiguous` in place. Returns total resolved count and iterations.
 */
export function propagate(
  matches: Map<string, string>,
  ambiguous: Map<string, string[]>,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  options?: PropagationOptions
): { resolved: number; iterations: number } {
  if (ambiguous.size === 0) return { resolved: 0, iterations: 0 };

  const oldFunctions = oldIndex.functions;
  const newFunctions = newIndex.functions;
  if (!oldFunctions || !newFunctions) return { resolved: 0, iterations: 0 };

  const state: PropagationState = {
    matches,
    reverseMatches: buildReverseMatches(matches),
    ambiguous,
    oldFunctions,
    newFunctions,
    newCallers: buildCallersIndex(newFunctions),
    oldScopeChildren: buildScopeChildrenIndex(oldFunctions),
    newScopeChildren: buildScopeChildrenIndex(newFunctions),
    externalRefEvidence: options?.externalRefEvidence
  };

  const maxIterations = options?.maxIterations ?? 10;
  let totalResolved = 0;

  for (let i = 0; i < maxIterations; i++) {
    const newlyResolved = runOneIteration(state);
    totalResolved += newlyResolved;
    if (newlyResolved === 0)
      return { resolved: totalResolved, iterations: i + 1 };
  }

  return { resolved: totalResolved, iterations: maxIterations };
}

/**
 * Runs one propagation iteration over all ambiguous entries.
 * Returns the number of newly resolved functions.
 */
function runOneIteration(state: PropagationState): number {
  let newlyResolved = 0;
  const entries = [...state.ambiguous.entries()];

  for (const [oldId, candidates] of entries) {
    if (!state.ambiguous.has(oldId)) continue;

    const { pool, evidenced } = narrowCandidates(oldId, candidates, state);

    if (pool.length === 1 && evidenced) {
      state.matches.set(oldId, pool[0]);
      state.reverseMatches.set(pool[0], oldId);
      state.ambiguous.delete(oldId);
      newlyResolved++;
    } else if (pool.length > 1 && pool.length < candidates.length) {
      state.ambiguous.set(oldId, pool);
    }
  }

  return newlyResolved;
}

interface Narrowing {
  pool: string[];
  /**
   * True when at least one strategy positively discriminated or confirmed
   * the pool. A pool that shrank to one candidate purely because the others
   * were claimed by other old functions is NOT evidence — matching on it
   * would be an order-dependent guess.
   */
  evidenced: boolean;
}

/**
 * Tries all propagation strategies on an ambiguous function, returning
 * the narrowed candidate pool and whether it is backed by evidence.
 * Candidates already matched to another old function are excluded up
 * front (injectivity).
 */
function narrowCandidates(
  oldId: string,
  candidates: string[],
  state: PropagationState
): Narrowing {
  const oldFn = state.oldFunctions.get(oldId);
  if (!oldFn) return { pool: candidates, evidenced: false };

  const available = candidates.filter(
    (candId) => !state.reverseMatches.has(candId)
  );
  if (available.length === 0) return { pool: available, evidenced: false };

  const narrowed = applyConstraintStrategies(oldFn, available, state);
  if (narrowed === "contradiction") return { pool: [], evidenced: false };
  if (narrowed.pool.length === 1 && narrowed.evidenced) return narrowed;

  // Scope-ordinal matching: position among same-hash siblings under a
  // matched parent (inherently evidenced when it fires).
  if (narrowed.pool.length > 1) {
    const ordinalMatch = tryScopeOrdinalMatch(oldId, narrowed.pool, state);
    if (ordinalMatch) return { pool: [ordinalMatch], evidenced: true };
  }

  return narrowed;
}

/**
 * Runs the matched-callee, matched-caller, and scope-parent constraints in
 * order over the candidate pool. Returns "contradiction" when a constraint
 * with evidence filters every candidate out — a weaker strategy must not
 * match what a stronger one rejected.
 */
function applyConstraintStrategies(
  oldFn: FunctionNode,
  candidates: string[],
  state: PropagationState
): Narrowing | "contradiction" {
  let pool = candidates;
  let evidenced = false;
  const strategies = [
    filterByMatchedCallees,
    filterByMatchedCallers,
    filterByScopeParent,
    filterByMatchedExternalRefs
  ];
  for (const strategy of strategies) {
    const filtered = strategy(oldFn, pool, state);
    if (filtered === null) continue;
    if (filtered.length === 0) return "contradiction";
    // Discriminating (shrank the pool) or confirming (evidence exists and
    // the sole surviving candidate satisfies it) — both are evidence.
    if (filtered.length < pool.length || pool.length === 1) evidenced = true;
    pool = filtered;
    if (pool.length === 1 && evidenced) break;
  }
  return { pool, evidenced };
}

/**
 * Strategy 1: If an old function calls callees that are already matched,
 * filter candidates to those that call the corresponding matched new callees.
 * Returns null when no callees are matched (no evidence).
 */
function filterByMatchedCallees(
  oldFn: FunctionNode,
  candidates: string[],
  state: PropagationState
): string[] | null {
  const matchedCalleeNewIds = getMatchedNewIds(
    oldFn.internalCallees,
    state.matches
  );
  if (matchedCalleeNewIds.length === 0) return null;

  return candidates.filter((candId) => {
    const candFn = state.newFunctions.get(candId);
    if (!candFn) return false;
    const candCalleeIds = new Set(
      [...candFn.internalCallees].map((c) => c.sessionId)
    );
    return matchedCalleeNewIds.every((id) => candCalleeIds.has(id));
  });
}

/**
 * Strategy 2: If an old function's callers are already matched, filter
 * candidates to those that are called by the corresponding matched new callers.
 * Returns null when no callers are matched (no evidence).
 */
function filterByMatchedCallers(
  oldFn: FunctionNode,
  candidates: string[],
  state: PropagationState
): string[] | null {
  const matchedCallerNewIds = getMatchedNewIds(oldFn.callers, state.matches);
  if (matchedCallerNewIds.length === 0) return null;

  return candidates.filter((candId) => {
    const candCallers = state.newCallers.get(candId);
    if (!candCallers) return false;
    return matchedCallerNewIds.every((callerId) => candCallers.has(callerId));
  });
}

/**
 * Strategy 3: If an old function's scopeParent is already matched, filter
 * candidates to those whose scopeParent is the corresponding matched new parent.
 * Returns null when the scope parent is unmatched (no evidence).
 */
function filterByScopeParent(
  oldFn: FunctionNode,
  candidates: string[],
  state: PropagationState
): string[] | null {
  if (!oldFn.scopeParent) return null;

  const matchedParentNewId = state.matches.get(oldFn.scopeParent.sessionId);
  if (!matchedParentNewId) return null;

  return candidates.filter((candId) => {
    const candFn = state.newFunctions.get(candId);
    if (!candFn) return false;
    return candFn.scopeParent?.sessionId === matchedParentNewId;
  });
}

/**
 * Strategy 5: If an old function references module bindings that the
 * binding cascade has matched, filter candidates to those referencing the
 * corresponding new-side bindings. This is the only discriminating signal
 * for module-scope functions with no callees, callers, or matched parent
 * (structurally identical export getters differ ONLY in which binding
 * they return). Returns null without evidence: no ref data for this
 * function, or none of its referenced bindings are matched.
 */
function filterByMatchedExternalRefs(
  oldFn: FunctionNode,
  candidates: string[],
  state: PropagationState
): string[] | null {
  const evidence = state.externalRefEvidence;
  if (!evidence) return null;
  const oldRefIds = evidence.oldRefs.get(oldFn.sessionId);
  if (!oldRefIds || oldRefIds.size === 0) return null;

  const expectedNewIds: string[] = [];
  for (const oldBindingId of oldRefIds) {
    const newBindingId = evidence.bindingMatches.get(oldBindingId);
    if (newBindingId) expectedNewIds.push(newBindingId);
  }
  if (expectedNewIds.length === 0) return null;

  return candidates.filter((candId) => {
    const candRefs = evidence.newRefs.get(candId);
    if (!candRefs) return false;
    return expectedNewIds.every((id) => candRefs.has(id));
  });
}

/**
 * For a set of FunctionNodes, returns the matched new IDs for those that
 * have been confirmed in the match map.
 */
function getMatchedNewIds(
  nodes: Set<FunctionNode>,
  matches: Map<string, string>
): string[] {
  const result: string[] = [];
  for (const node of nodes) {
    const matchedNewId = matches.get(node.sessionId);
    if (matchedNewId) result.push(matchedNewId);
  }
  return result;
}

function buildReverseMatches(
  matches: Map<string, string>
): Map<string, string> {
  const reverse = new Map<string, string>();
  for (const [oldId, newId] of matches) {
    reverse.set(newId, oldId);
  }
  return reverse;
}

/**
 * Strategy 4: Scope-ordinal matching.
 * When multiple candidates share the same matched parent and structuralHash,
 * match by ordinal position among same-hash siblings under that parent.
 * Only matches when old and new sibling counts are equal (safety).
 */
function tryScopeOrdinalMatch(
  oldId: string,
  candidates: string[],
  state: PropagationState
): string | null {
  const oldFn = state.oldFunctions.get(oldId);
  if (!oldFn?.scopeParent) return null;

  const matchedParentNewId = state.matches.get(oldFn.scopeParent.sessionId);
  if (!matchedParentNewId) return null;

  const oldHash = oldFn.fingerprint.structuralHash;

  // Get all old siblings under the same parent with the same hash, sorted by position
  const oldSiblings = (
    state.oldScopeChildren.get(oldFn.scopeParent.sessionId) ?? []
  ).filter((id) => {
    const fn = state.oldFunctions.get(id);
    return fn?.fingerprint.structuralHash === oldHash;
  });

  // Get all new siblings under the matched parent with the same hash, sorted by position
  const newSiblings = (
    state.newScopeChildren.get(matchedParentNewId) ?? []
  ).filter((id) => {
    const fn = state.newFunctions.get(id);
    return fn?.fingerprint.structuralHash === oldHash;
  });

  // Only match when counts are equal (no additions/removals)
  if (oldSiblings.length !== newSiblings.length) return null;
  if (oldSiblings.length === 0) return null;

  const ordinal = oldSiblings.indexOf(oldId);
  if (ordinal === -1) return null;

  const matched = newSiblings[ordinal];
  // Verify the candidate is actually in our candidate list
  if (!candidates.includes(matched)) return null;

  return matched;
}

/**
 * Builds a scope children index: parentId → childIds sorted by source position.
 */
function buildScopeChildrenIndex(
  functions: Map<string, FunctionNode>
): Map<string, string[]> {
  const children = new Map<string, string[]>();

  for (const [id, fn] of functions) {
    if (!fn.scopeParent) continue;
    const parentId = fn.scopeParent.sessionId;
    let list = children.get(parentId);
    if (!list) {
      list = [];
      children.set(parentId, list);
    }
    list.push(id);
  }

  // Sort each group by the stored source position (nodes without loc sort last)
  const positionOf = (id: string): number => {
    const pos = functions.get(id)?.position;
    return pos ? pos.line * 100000 + pos.column : Number.MAX_SAFE_INTEGER;
  };
  for (const list of children.values()) {
    list.sort((a, b) => positionOf(a) - positionOf(b));
  }

  return children;
}

/**
 * Builds a reverse callers index: for each function, the set of function IDs that call it.
 */
function buildCallersIndex(
  functions: Map<string, FunctionNode>
): Map<string, Set<string>> {
  const callers = new Map<string, Set<string>>();

  for (const [id, fn] of functions) {
    for (const callee of fn.internalCallees) {
      let callerSet = callers.get(callee.sessionId);
      if (!callerSet) {
        callerSet = new Set();
        callers.set(callee.sessionId, callerSet);
      }
      callerSet.add(id);
    }
  }

  return callers;
}
