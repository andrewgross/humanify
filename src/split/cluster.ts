import { createHash } from "node:crypto";
import type { FunctionNode } from "../analysis/types.js";
import type { Cluster } from "./types.js";

export interface ClusterResult {
  clusters: Cluster[];
  shared: Set<string>;
  orphans: Set<string>;
}

/**
 * Sort functions deterministically: by exactHash, tiebreak by sessionId.
 */
function sortFunctions(fns: FunctionNode[]): FunctionNode[] {
  return [...fns].sort((a, b) => {
    const hashCmp = a.fingerprint.exactHash.localeCompare(b.fingerprint.exactHash);
    if (hashCmp !== 0) return hashCmp;
    return a.sessionId.localeCompare(b.sessionId);
  });
}

/**
 * Cluster functions based on call graph reachability from root functions.
 *
 * Algorithm:
 * 1. Filter to top-level functions (no scopeParent)
 * 2. Identify roots: top-level functions with callers.size === 0 (among top-level)
 * 3. BFS from each root following internalCallees (only top-level callees)
 * 4. Exclusively-owned → root's cluster; multi-owned → shared
 * 5. Merge circular roots (bidirectional callers)
 * 6. Compute cluster fingerprint: sha256(sorted member exactHashes).slice(0,16)
 */
export function clusterFunctions(functions: FunctionNode[]): ClusterResult {
  // Step 1: Filter to top-level functions only
  const topLevel = functions.filter(fn => !fn.scopeParent);

  if (topLevel.length === 0) {
    return { clusters: [], shared: new Set(), orphans: new Set() };
  }

  // Build a set of top-level sessionIds for filtering callees
  const topLevelIds = new Set(topLevel.map(fn => fn.sessionId));

  // Step 2: Identify roots - top-level functions with no top-level callers
  const sorted = sortFunctions(topLevel);
  const roots: FunctionNode[] = [];
  for (const fn of sorted) {
    const hasTopLevelCaller = Array.from(fn.callers).some(c => topLevelIds.has(c.sessionId));
    if (!hasTopLevelCaller) {
      roots.push(fn);
    }
  }

  // If no roots found (everything calls everything), treat all as roots
  if (roots.length === 0) {
    roots.push(...sorted);
  }

  // Step 5 (early): Merge circular roots before BFS
  // If root A calls root B AND root B calls root A, merge them
  const rootGroups = mergeCircularRoots(roots);

  // Step 3: BFS from each root group following internalCallees (top-level only)
  const reachable = new Map<number, Set<string>>(); // groupIndex → reachable sessionIds

  for (let gi = 0; gi < rootGroups.length; gi++) {
    const group = rootGroups[gi];
    const reached = new Set<string>();
    const queue: FunctionNode[] = [...group];

    // Add roots themselves
    for (const r of group) {
      reached.add(r.sessionId);
    }

    while (queue.length > 0) {
      const current = queue.shift()!;
      for (const callee of current.internalCallees) {
        if (topLevelIds.has(callee.sessionId) && !reached.has(callee.sessionId)) {
          reached.add(callee.sessionId);
          queue.push(callee);
        }
      }
    }

    reachable.set(gi, reached);
  }

  // Step 4: Assign functions to clusters or shared
  const shared = new Set<string>();
  const orphans = new Set<string>();
  const assignments = new Map<string, number>(); // sessionId → group index

  // First assign roots to their own groups
  for (let gi = 0; gi < rootGroups.length; gi++) {
    for (const root of rootGroups[gi]) {
      assignments.set(root.sessionId, gi);
    }
  }

  // Then assign non-root top-level functions
  for (const fn of sorted) {
    if (assignments.has(fn.sessionId)) continue;

    const owners: number[] = [];
    for (const [gi, reached] of reachable) {
      if (reached.has(fn.sessionId)) {
        owners.push(gi);
      }
    }

    if (owners.length === 1) {
      assignments.set(fn.sessionId, owners[0]);
    } else if (owners.length > 1) {
      shared.add(fn.sessionId);
    } else {
      orphans.add(fn.sessionId);
    }
  }

  // Step 6: Build Cluster objects with fingerprints
  const fnBySessionId = new Map<string, FunctionNode>();
  for (const fn of topLevel) {
    fnBySessionId.set(fn.sessionId, fn);
  }

  const clusters: Cluster[] = [];
  for (let gi = 0; gi < rootGroups.length; gi++) {
    const members = new Set<string>();
    for (const [sessionId, groupIdx] of assignments) {
      if (groupIdx === gi) {
        members.add(sessionId);
      }
    }

    const memberHashes = Array.from(members)
      .map(id => fnBySessionId.get(id)!.fingerprint.exactHash)
      .sort();

    const fingerprint = createHash("sha256")
      .update(memberHashes.join(","))
      .digest("hex")
      .slice(0, 16);

    clusters.push({
      id: fingerprint,
      rootFunctions: rootGroups[gi].map(fn => fn.sessionId).sort(),
      members,
      memberHashes,
    });
  }

  // Sort clusters deterministically by ID
  clusters.sort((a, b) => a.id.localeCompare(b.id));

  return { clusters, shared, orphans };
}

/**
 * Merge roots that have bidirectional call relationships.
 * Returns groups of roots that should form a single cluster.
 */
function mergeCircularRoots(roots: FunctionNode[]): FunctionNode[][] {
  const rootSet = new Set(roots.map(r => r.sessionId));
  // Union-Find
  const parent = new Map<string, string>();
  for (const r of roots) {
    parent.set(r.sessionId, r.sessionId);
  }

  function find(id: string): string {
    while (parent.get(id) !== id) {
      parent.set(id, parent.get(parent.get(id)!)!);
      id = parent.get(id)!;
    }
    return id;
  }

  function union(a: string, b: string): void {
    const ra = find(a);
    const rb = find(b);
    if (ra !== rb) {
      // Use lexicographic order for determinism
      if (ra < rb) {
        parent.set(rb, ra);
      } else {
        parent.set(ra, rb);
      }
    }
  }

  // Merge roots that call each other bidirectionally
  for (const rootA of roots) {
    for (const rootB of roots) {
      if (rootA === rootB) continue;
      const aCallsB = rootA.internalCallees.has(rootB);
      const bCallsA = rootB.internalCallees.has(rootA);
      if (aCallsB && bCallsA) {
        union(rootA.sessionId, rootB.sessionId);
      }
    }
  }

  // Group by root
  const groups = new Map<string, FunctionNode[]>();
  const rootById = new Map<string, FunctionNode>();
  for (const r of roots) {
    rootById.set(r.sessionId, r);
  }

  for (const r of roots) {
    const groupId = find(r.sessionId);
    if (!groups.has(groupId)) {
      groups.set(groupId, []);
    }
    groups.get(groupId)!.push(r);
  }

  // Sort groups deterministically
  const result = Array.from(groups.values());
  for (const group of result) {
    group.sort((a, b) => a.sessionId.localeCompare(b.sessionId));
  }
  result.sort((a, b) => a[0].sessionId.localeCompare(b[0].sessionId));

  return result;
}
