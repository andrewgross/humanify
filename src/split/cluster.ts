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
export interface ClusterOptions {
  /** Clusters with this many members or fewer get merged. 0 = no merging. */
  minClusterSize?: number;
  /** Maximum rounds of merge + reabsorb. */
  maxMergeRounds?: number;
}

export function clusterFunctions(functions: FunctionNode[], options?: ClusterOptions): ClusterResult {
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

  // Post-processing: merge small clusters and reabsorb shared
  const minSize = options?.minClusterSize ?? 0;
  const maxRounds = options?.maxMergeRounds ?? 10;

  if (minSize > 0) {
    return mergeAndReabsorb(clusters, shared, orphans, fnBySessionId, topLevelIds, minSize, maxRounds);
  }

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

/**
 * Post-process: merge small clusters into their most-connected neighbor,
 * then reabsorb shared functions that now have a single owner.
 * Repeats until stable or maxRounds reached.
 */
function mergeAndReabsorb(
  clusters: Cluster[],
  shared: Set<string>,
  orphans: Set<string>,
  fnBySessionId: Map<string, FunctionNode>,
  topLevelIds: Set<string>,
  minSize: number,
  maxRounds: number,
): ClusterResult {
  let currentClusters = [...clusters];
  let currentShared = new Set(shared);

  for (let round = 0; round < maxRounds; round++) {
    const beforeCount = currentClusters.length;
    const beforeShared = currentShared.size;

    // Phase 1: Merge small clusters
    currentClusters = mergeSmallClusters(currentClusters, currentShared, fnBySessionId, topLevelIds, minSize);

    // Phase 2: Reabsorb shared functions
    currentShared = reabsorbShared(currentClusters, currentShared, fnBySessionId, topLevelIds);

    // Check stability
    if (currentClusters.length === beforeCount && currentShared.size === beforeShared) {
      break;
    }
  }

  // Recompute fingerprints after merging
  for (const cluster of currentClusters) {
    cluster.memberHashes = Array.from(cluster.members)
      .map(id => fnBySessionId.get(id)!.fingerprint.exactHash)
      .sort();
    cluster.id = createHash("sha256")
      .update(cluster.memberHashes.join(","))
      .digest("hex")
      .slice(0, 16);
  }

  currentClusters.sort((a, b) => a.id.localeCompare(b.id));
  return { clusters: currentClusters, shared: currentShared, orphans };
}

/**
 * Merge clusters with <= minSize members into their most-connected neighbor.
 * "Most connected" = cluster with most call edges to/from this cluster's members.
 */
function mergeSmallClusters(
  clusters: Cluster[],
  shared: Set<string>,
  fnBySessionId: Map<string, FunctionNode>,
  topLevelIds: Set<string>,
  minSize: number,
): Cluster[] {
  // Build member → cluster index map
  const clusterOf = new Map<string, number>();
  for (let i = 0; i < clusters.length; i++) {
    for (const member of clusters[i].members) {
      clusterOf.set(member, i);
    }
  }

  // Find merge targets for small clusters
  const mergeInto = new Map<number, number>(); // small cluster idx → target cluster idx

  // Process small clusters in deterministic order (by cluster ID)
  for (let i = 0; i < clusters.length; i++) {
    if (clusters[i].members.size > minSize) continue;

    // Count edges to each other cluster
    const edgeCounts = new Map<number, number>();
    for (const memberId of clusters[i].members) {
      const fn = fnBySessionId.get(memberId);
      if (fn == null) continue;

      // Count outgoing edges (callees)
      for (const callee of fn.internalCallees) {
        if (!topLevelIds.has(callee.sessionId)) continue;
        const targetCluster = clusterOf.get(callee.sessionId);
        if (targetCluster !== undefined && targetCluster !== i) {
          edgeCounts.set(targetCluster, (edgeCounts.get(targetCluster) ?? 0) + 1);
        }
      }

      // Count incoming edges (callers)
      for (const caller of fn.callers) {
        if (!topLevelIds.has(caller.sessionId)) continue;
        const targetCluster = clusterOf.get(caller.sessionId);
        if (targetCluster !== undefined && targetCluster !== i) {
          edgeCounts.set(targetCluster, (edgeCounts.get(targetCluster) ?? 0) + 1);
        }
      }

      // Count edges through shared functions: if this member calls a shared fn
      // that is also called by another cluster, that's an indirect connection
      for (const callee of fn.internalCallees) {
        if (shared.has(callee.sessionId)) {
          // This shared function connects us to clusters that also call it
          const sharedFn = fnBySessionId.get(callee.sessionId);
          if (sharedFn == null) continue;
          for (const sharedCaller of sharedFn.callers) {
            if (!topLevelIds.has(sharedCaller.sessionId)) continue;
            const targetCluster = clusterOf.get(sharedCaller.sessionId);
            if (targetCluster !== undefined && targetCluster !== i) {
              edgeCounts.set(targetCluster, (edgeCounts.get(targetCluster) ?? 0) + 1);
            }
          }
        }
      }
      for (const caller of fn.callers) {
        if (shared.has(caller.sessionId)) {
          const sharedFn = fnBySessionId.get(caller.sessionId);
          if (sharedFn == null) continue;
          for (const sharedCallee of sharedFn.internalCallees) {
            if (!topLevelIds.has(sharedCallee.sessionId)) continue;
            const targetCluster = clusterOf.get(sharedCallee.sessionId);
            if (targetCluster !== undefined && targetCluster !== i) {
              edgeCounts.set(targetCluster, (edgeCounts.get(targetCluster) ?? 0) + 1);
            }
          }
        }
      }
    }

    if (edgeCounts.size === 0) continue;

    // Find the cluster with most edges, tiebreak by cluster ID
    let bestTarget = -1;
    let bestCount = 0;
    for (const [target, count] of edgeCounts) {
      if (count > bestCount || (count === bestCount && (bestTarget === -1 || clusters[target].id < clusters[bestTarget].id))) {
        bestTarget = target;
        bestCount = count;
      }
    }

    if (bestTarget >= 0) {
      mergeInto.set(i, bestTarget);
    }
  }

  // Resolve merge chains (A→B→C becomes A→C)
  for (const [from, to] of mergeInto) {
    let target = to;
    const visited = new Set([from]);
    while (mergeInto.has(target) && !visited.has(target)) {
      visited.add(target);
      target = mergeInto.get(target)!;
    }
    mergeInto.set(from, target);
  }

  // Execute merges
  const merged = new Set<number>();
  for (const [from, to] of mergeInto) {
    if (from === to) continue;
    // Move all members from 'from' to 'to'
    for (const member of clusters[from].members) {
      clusters[to].members.add(member);
    }
    // Merge root functions
    for (const root of clusters[from].rootFunctions) {
      if (!clusters[to].rootFunctions.includes(root)) {
        clusters[to].rootFunctions.push(root);
      }
    }
    clusters[to].rootFunctions.sort();
    merged.add(from);
  }

  // Remove merged clusters
  return clusters.filter((_, i) => !merged.has(i));
}

/**
 * Re-evaluate shared functions after merging. If all callers of a shared
 * function now belong to a single cluster, absorb it into that cluster.
 */
function reabsorbShared(
  clusters: Cluster[],
  shared: Set<string>,
  fnBySessionId: Map<string, FunctionNode>,
  topLevelIds: Set<string>,
): Set<string> {
  // Build member → cluster index map
  const clusterOf = new Map<string, number>();
  for (let i = 0; i < clusters.length; i++) {
    for (const member of clusters[i].members) {
      clusterOf.set(member, i);
    }
  }

  const stillShared = new Set<string>();

  for (const sessionId of shared) {
    const fn = fnBySessionId.get(sessionId);
    if (fn == null) {
      stillShared.add(sessionId);
      continue;
    }

    // Find which clusters reference this shared function (as caller or callee)
    const referencingClusters = new Set<number>();

    for (const caller of fn.callers) {
      if (!topLevelIds.has(caller.sessionId)) continue;
      const ci = clusterOf.get(caller.sessionId);
      if (ci !== undefined) referencingClusters.add(ci);
    }

    for (const callee of fn.internalCallees) {
      if (!topLevelIds.has(callee.sessionId)) continue;
      const ci = clusterOf.get(callee.sessionId);
      if (ci !== undefined) referencingClusters.add(ci);
    }

    if (referencingClusters.size === 1) {
      // All references come from one cluster — absorb
      const targetCluster = referencingClusters.values().next().value!;
      clusters[targetCluster].members.add(sessionId);
      clusterOf.set(sessionId, targetCluster);
    } else {
      stillShared.add(sessionId);
    }
  }

  return stillShared;
}
