import type { FingerprintIndex, FunctionNode } from "./types.js";

export interface PropagationOptions {
  maxIterations?: number; // default: 10
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
    newScopeChildren: buildScopeChildrenIndex(newFunctions)
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

    const narrowed = narrowCandidates(oldId, candidates, state);

    if (narrowed.length === 1) {
      state.matches.set(oldId, narrowed[0]);
      state.reverseMatches.set(narrowed[0], oldId);
      state.ambiguous.delete(oldId);
      newlyResolved++;
    } else if (narrowed.length > 1 && narrowed.length < candidates.length) {
      state.ambiguous.set(oldId, narrowed);
    }
  }

  return newlyResolved;
}

/**
 * Tries all propagation strategies on an ambiguous function, returning
 * the narrowed candidate list.
 */
function narrowCandidates(
  oldId: string,
  candidates: string[],
  state: PropagationState
): string[] {
  const oldFn = state.oldFunctions.get(oldId);
  if (!oldFn) return candidates;

  // Strategy 1: Matched-callee constraint
  let narrowed = filterByMatchedCallees(oldFn, candidates, state);
  if (narrowed.length === 1) return narrowed;

  // Strategy 2: Matched-caller constraint
  const callerNarrowed = filterByMatchedCallers(
    oldFn,
    narrowed.length > 0 ? narrowed : candidates,
    state
  );
  if (callerNarrowed.length === 1) return callerNarrowed;
  if (callerNarrowed.length > 0) narrowed = callerNarrowed;

  // Strategy 3: Scope-parent constraint
  const scopeNarrowed = filterByScopeParent(
    oldFn,
    narrowed.length > 0 ? narrowed : candidates,
    state
  );
  if (scopeNarrowed.length === 1) return scopeNarrowed;

  // Strategy 4: Scope-ordinal matching
  // When scope-parent filter narrowed but didn't resolve to 1
  if (scopeNarrowed.length > 1) {
    const ordinalMatch = tryScopeOrdinalMatch(oldId, scopeNarrowed, state);
    if (ordinalMatch) return [ordinalMatch];
    return scopeNarrowed;
  }

  return narrowed.length > 0 ? narrowed : candidates;
}

/**
 * Strategy 1: If an old function calls callees that are already matched,
 * filter candidates to those that call the corresponding matched new callees.
 */
function filterByMatchedCallees(
  oldFn: FunctionNode,
  candidates: string[],
  state: PropagationState
): string[] {
  const matchedCalleeNewIds = getMatchedNewIds(
    oldFn.internalCallees,
    state.matches
  );
  if (matchedCalleeNewIds.length === 0) return candidates;

  const filtered = candidates.filter((candId) => {
    const candFn = state.newFunctions.get(candId);
    if (!candFn) return false;
    const candCalleeIds = new Set(
      [...candFn.internalCallees].map((c) => c.sessionId)
    );
    return matchedCalleeNewIds.every((id) => candCalleeIds.has(id));
  });

  return filtered.length > 0 ? filtered : candidates;
}

/**
 * Strategy 2: If an old function's callers are already matched, filter
 * candidates to those that are called by the corresponding matched new callers.
 */
function filterByMatchedCallers(
  oldFn: FunctionNode,
  candidates: string[],
  state: PropagationState
): string[] {
  const matchedCallerNewIds = getMatchedNewIds(oldFn.callers, state.matches);
  if (matchedCallerNewIds.length === 0) return candidates;

  const filtered = candidates.filter((candId) => {
    const candCallers = state.newCallers.get(candId);
    if (!candCallers) return false;
    return matchedCallerNewIds.every((callerId) => candCallers.has(callerId));
  });

  return filtered.length > 0 ? filtered : candidates;
}

/**
 * Strategy 3: If an old function's scopeParent is already matched, filter
 * candidates to those whose scopeParent is the corresponding matched new parent.
 */
function filterByScopeParent(
  oldFn: FunctionNode,
  candidates: string[],
  state: PropagationState
): string[] {
  if (!oldFn.scopeParent) return candidates;

  const matchedParentNewId = state.matches.get(oldFn.scopeParent.sessionId);
  if (!matchedParentNewId) return candidates;

  const filtered = candidates.filter((candId) => {
    const candFn = state.newFunctions.get(candId);
    if (!candFn) return false;
    return candFn.scopeParent?.sessionId === matchedParentNewId;
  });

  return filtered.length > 0 ? filtered : candidates;
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
 * Extracts a sortable position number from a sessionId.
 * sessionId format: "filepath:line:column"
 */
function extractPosition(sessionId: string): number {
  const parts = sessionId.split(":");
  const col = parseInt(parts[parts.length - 1], 10);
  const line = parseInt(parts[parts.length - 2], 10);
  return line * 100000 + col;
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

  // Sort each group by source position
  for (const list of children.values()) {
    list.sort((a, b) => extractPosition(a) - extractPosition(b));
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
