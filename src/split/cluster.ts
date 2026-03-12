import { createHash } from "node:crypto";
import type { FunctionNode } from "../analysis/types.js";
import type { Cluster } from "./types.js";

interface ClusterResult {
  clusters: Cluster[];
  shared: Set<string>;
  orphans: Set<string>;
}

/**
 * Sort functions deterministically: by exactHash, tiebreak by sessionId.
 */
function sortFunctions(fns: FunctionNode[]): FunctionNode[] {
  return [...fns].sort((a, b) => {
    const hashCmp = a.fingerprint.exactHash.localeCompare(
      b.fingerprint.exactHash
    );
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
  /** Merge isolated singletons (no edges) into nearest cluster by source line proximity. */
  proximityFallback?: boolean;
}

/** Find root functions among top-level functions (those with no top-level callers). */
function findRoots(
  sorted: FunctionNode[],
  topLevelIds: Set<string>
): FunctionNode[] {
  const roots: FunctionNode[] = [];
  for (const fn of sorted) {
    const hasTopLevelCaller = Array.from(fn.callers).some((c) =>
      topLevelIds.has(c.sessionId)
    );
    if (!hasTopLevelCaller) {
      roots.push(fn);
    }
  }
  return roots;
}

/** BFS from a root group, returning all reachable top-level sessionIds. */
function bfsReachable(
  group: FunctionNode[],
  topLevelIds: Set<string>
): Set<string> {
  const reached = new Set<string>();
  const queue: FunctionNode[] = [...group];
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
  return reached;
}

/** Assign non-root top-level functions to clusters, shared, or orphans. */
function assignNonRoots(
  sorted: FunctionNode[],
  assignments: Map<string, number>,
  reachable: Map<number, Set<string>>,
  shared: Set<string>,
  orphans: Set<string>
): void {
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
}

/** Build Cluster objects from root groups and assignments. */
function buildClusters(
  rootGroups: FunctionNode[][],
  assignments: Map<string, number>,
  fnBySessionId: Map<string, FunctionNode>
): Cluster[] {
  const clusters: Cluster[] = [];
  for (let gi = 0; gi < rootGroups.length; gi++) {
    const members = new Set<string>();
    for (const [sessionId, groupIdx] of assignments) {
      if (groupIdx === gi) {
        members.add(sessionId);
      }
    }

    const memberHashes = Array.from(members)
      .map((id) => fnBySessionId.get(id)!.fingerprint.exactHash)
      .sort();

    const fingerprint = createHash("sha256")
      .update(memberHashes.join(","))
      .digest("hex")
      .slice(0, 16);

    clusters.push({
      id: fingerprint,
      rootFunctions: rootGroups[gi].map((fn) => fn.sessionId).sort(),
      members,
      memberHashes
    });
  }
  return clusters;
}

export function clusterFunctions(
  functions: FunctionNode[],
  options?: ClusterOptions
): ClusterResult {
  // Step 1: Filter to top-level functions only
  const topLevel = functions.filter((fn) => !fn.scopeParent);

  if (topLevel.length === 0) {
    return { clusters: [], shared: new Set(), orphans: new Set() };
  }

  // Build a set of top-level sessionIds for filtering callees
  const topLevelIds = new Set(topLevel.map((fn) => fn.sessionId));

  // Step 2: Identify roots - top-level functions with no top-level callers
  const sorted = sortFunctions(topLevel);
  let roots = findRoots(sorted, topLevelIds);

  // If no roots found (everything calls everything), treat all as roots
  if (roots.length === 0) {
    roots = [...sorted];
  }

  // Step 5 (early): Merge circular roots before BFS
  const rootGroups = mergeCircularRoots(roots);

  // Step 3: BFS from each root group
  const reachable = new Map<number, Set<string>>();
  for (let gi = 0; gi < rootGroups.length; gi++) {
    reachable.set(gi, bfsReachable(rootGroups[gi], topLevelIds));
  }

  // Step 4: Assign functions to clusters or shared
  const shared = new Set<string>();
  const orphans = new Set<string>();
  const assignments = new Map<string, number>();

  // First assign roots to their own groups
  for (let gi = 0; gi < rootGroups.length; gi++) {
    for (const root of rootGroups[gi]) {
      assignments.set(root.sessionId, gi);
    }
  }

  // Then assign non-root top-level functions
  assignNonRoots(sorted, assignments, reachable, shared, orphans);

  // Step 6: Build Cluster objects with fingerprints
  const fnBySessionId = new Map<string, FunctionNode>();
  for (const fn of topLevel) {
    fnBySessionId.set(fn.sessionId, fn);
  }

  const clusters = buildClusters(rootGroups, assignments, fnBySessionId);

  // Sort clusters deterministically by ID
  clusters.sort((a, b) => a.id.localeCompare(b.id));

  // Post-processing: merge small clusters and reabsorb shared
  const minSize = options?.minClusterSize ?? 0;
  const maxRounds = options?.maxMergeRounds ?? 10;

  let result: ClusterResult;
  if (minSize > 0) {
    result = mergeAndReabsorb(
      clusters,
      shared,
      orphans,
      fnBySessionId,
      topLevelIds,
      minSize,
      maxRounds
    );
  } else {
    result = { clusters, shared, orphans };
  }

  // Post-processing: merge isolated small clusters by source proximity
  if (options?.proximityFallback) {
    result = mergeByProximity(result, fnBySessionId, minSize);
  }

  return result;
}

/** Union-Find: path-compressed find. */
function ufFind(parent: Map<string, string>, id: string): string {
  while (parent.get(id) !== id) {
    parent.set(id, parent.get(parent.get(id)!)!);
    id = parent.get(id)!;
  }
  return id;
}

/** Union-Find: union by lexicographic order for determinism. */
function ufUnion(parent: Map<string, string>, a: string, b: string): void {
  const ra = ufFind(parent, a);
  const rb = ufFind(parent, b);
  if (ra !== rb) {
    if (ra < rb) {
      parent.set(rb, ra);
    } else {
      parent.set(ra, rb);
    }
  }
}

/**
 * Merge roots that have bidirectional call relationships.
 * Returns groups of roots that should form a single cluster.
 */
function mergeCircularRoots(roots: FunctionNode[]): FunctionNode[][] {
  const parent = new Map<string, string>();
  for (const r of roots) {
    parent.set(r.sessionId, r.sessionId);
  }

  // Merge roots that call each other bidirectionally
  for (const rootA of roots) {
    for (const rootB of roots) {
      if (rootA === rootB) continue;
      const aCallsB = rootA.internalCallees.has(rootB);
      const bCallsA = rootB.internalCallees.has(rootA);
      if (aCallsB && bCallsA) {
        ufUnion(parent, rootA.sessionId, rootB.sessionId);
      }
    }
  }

  // Group by root
  const groups = new Map<string, FunctionNode[]>();
  for (const r of roots) {
    const groupId = ufFind(parent, r.sessionId);
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
  maxRounds: number
): ClusterResult {
  let currentClusters = [...clusters];
  let currentShared = new Set(shared);

  for (let round = 0; round < maxRounds; round++) {
    const beforeCount = currentClusters.length;
    const beforeShared = currentShared.size;

    // Phase 1: Merge small clusters
    currentClusters = mergeSmallClusters(
      currentClusters,
      currentShared,
      fnBySessionId,
      topLevelIds,
      minSize
    );

    // Phase 2: Reabsorb shared functions
    currentShared = reabsorbShared(
      currentClusters,
      currentShared,
      fnBySessionId,
      topLevelIds
    );

    // Check stability
    if (
      currentClusters.length === beforeCount &&
      currentShared.size === beforeShared
    ) {
      break;
    }
  }

  // Recompute fingerprints after merging
  for (const cluster of currentClusters) {
    cluster.memberHashes = Array.from(cluster.members)
      .map((id) => fnBySessionId.get(id)!.fingerprint.exactHash)
      .sort();
    cluster.id = createHash("sha256")
      .update(cluster.memberHashes.join(","))
      .digest("hex")
      .slice(0, 16);
  }

  currentClusters.sort((a, b) => a.id.localeCompare(b.id));
  return { clusters: currentClusters, shared: currentShared, orphans };
}

/** Increment edgeCounts for a neighbor node if it belongs to a different cluster. */
function addEdgeToNeighbor(
  neighborId: string,
  clusterIdx: number,
  topLevelIds: Set<string>,
  clusterOf: Map<string, number>,
  edgeCounts: Map<number, number>
): void {
  if (!topLevelIds.has(neighborId)) return;
  const targetCluster = clusterOf.get(neighborId);
  if (targetCluster !== undefined && targetCluster !== clusterIdx) {
    edgeCounts.set(targetCluster, (edgeCounts.get(targetCluster) ?? 0) + 1);
  }
}

/** Count indirect edges through a shared intermediary node. */
function countEdgesThroughShared(
  sharedId: string,
  clusterIdx: number,
  fnBySessionId: Map<string, FunctionNode>,
  topLevelIds: Set<string>,
  clusterOf: Map<string, number>,
  edgeCounts: Map<number, number>,
  useCallers: boolean
): void {
  const sharedFn = fnBySessionId.get(sharedId);
  if (sharedFn == null) return;
  const neighbors = useCallers ? sharedFn.callers : sharedFn.internalCallees;
  for (const neighbor of neighbors) {
    addEdgeToNeighbor(
      neighbor.sessionId,
      clusterIdx,
      topLevelIds,
      clusterOf,
      edgeCounts
    );
  }
}

/**
 * Count inter-cluster call edges for a single cluster member.
 * Includes direct edges and edges through shared functions.
 */
function countEdgesForMember(
  memberId: string,
  clusterIdx: number,
  fnBySessionId: Map<string, FunctionNode>,
  topLevelIds: Set<string>,
  clusterOf: Map<string, number>,
  shared: Set<string>,
  edgeCounts: Map<number, number>
): void {
  const fn = fnBySessionId.get(memberId);
  if (fn == null) return;

  // Count outgoing edges (callees)
  for (const callee of fn.internalCallees) {
    addEdgeToNeighbor(
      callee.sessionId,
      clusterIdx,
      topLevelIds,
      clusterOf,
      edgeCounts
    );
  }

  // Count incoming edges (callers)
  for (const caller of fn.callers) {
    addEdgeToNeighbor(
      caller.sessionId,
      clusterIdx,
      topLevelIds,
      clusterOf,
      edgeCounts
    );
  }

  // Count edges through shared callees (useCallers=true: shared callee's callers are peers)
  for (const callee of fn.internalCallees) {
    if (!shared.has(callee.sessionId)) continue;
    countEdgesThroughShared(
      callee.sessionId,
      clusterIdx,
      fnBySessionId,
      topLevelIds,
      clusterOf,
      edgeCounts,
      true
    );
  }

  // Count edges through shared callers (useCallers=false: shared caller's callees are peers)
  for (const caller of fn.callers) {
    if (!shared.has(caller.sessionId)) continue;
    countEdgesThroughShared(
      caller.sessionId,
      clusterIdx,
      fnBySessionId,
      topLevelIds,
      clusterOf,
      edgeCounts,
      false
    );
  }
}

/** Find the best merge target by highest edge count, tiebreak by cluster ID. */
function findBestMergeTarget(
  edgeCounts: Map<number, number>,
  clusters: Cluster[]
): number {
  let bestTarget = -1;
  let bestCount = 0;
  for (const [target, count] of edgeCounts) {
    if (
      count > bestCount ||
      (count === bestCount &&
        (bestTarget === -1 || clusters[target].id < clusters[bestTarget].id))
    ) {
      bestTarget = target;
      bestCount = count;
    }
  }
  return bestTarget;
}

/** Resolve merge chains: A→B→C becomes A→C. */
function resolveMergeChains(mergeInto: Map<number, number>): void {
  for (const [from, to] of mergeInto) {
    let target = to;
    const visited = new Set([from]);
    while (mergeInto.has(target) && !visited.has(target)) {
      visited.add(target);
      target = mergeInto.get(target)!;
    }
    mergeInto.set(from, target);
  }
}

/** Execute the merges specified by mergeInto map. Returns set of absorbed indices. */
function executeMerges(
  clusters: Cluster[],
  mergeInto: Map<number, number>
): Set<number> {
  const merged = new Set<number>();
  for (const [from, to] of mergeInto) {
    if (from === to) continue;
    for (const member of clusters[from].members) {
      clusters[to].members.add(member);
    }
    for (const root of clusters[from].rootFunctions) {
      if (!clusters[to].rootFunctions.includes(root)) {
        clusters[to].rootFunctions.push(root);
      }
    }
    clusters[to].rootFunctions.sort();
    merged.add(from);
  }
  return merged;
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
  minSize: number
): Cluster[] {
  // Build member → cluster index map
  const clusterOf = new Map<string, number>();
  for (let i = 0; i < clusters.length; i++) {
    for (const member of clusters[i].members) {
      clusterOf.set(member, i);
    }
  }

  // Find merge targets for small clusters
  const mergeInto = new Map<number, number>();

  for (let i = 0; i < clusters.length; i++) {
    if (clusters[i].members.size > minSize) continue;

    const edgeCounts = new Map<number, number>();
    for (const memberId of clusters[i].members) {
      countEdgesForMember(
        memberId,
        i,
        fnBySessionId,
        topLevelIds,
        clusterOf,
        shared,
        edgeCounts
      );
    }

    if (edgeCounts.size === 0) continue;

    const bestTarget = findBestMergeTarget(edgeCounts, clusters);
    if (bestTarget >= 0) {
      mergeInto.set(i, bestTarget);
    }
  }

  // Resolve merge chains (A→B→C becomes A→C)
  resolveMergeChains(mergeInto);

  // Execute merges and filter out absorbed clusters
  const merged = executeMerges(clusters, mergeInto);
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
  topLevelIds: Set<string>
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

    // Find which clusters reference this shared function
    const referencingClusters = collectReferencingClusters(
      fn,
      topLevelIds,
      clusterOf
    );

    if (referencingClusters.size === 1) {
      const targetCluster = referencingClusters.values().next().value!;
      clusters[targetCluster].members.add(sessionId);
      clusterOf.set(sessionId, targetCluster);
    } else {
      stillShared.add(sessionId);
    }
  }

  return stillShared;
}

/** Collect cluster indices that reference a given function (as caller or callee). */
function collectReferencingClusters(
  fn: FunctionNode,
  topLevelIds: Set<string>,
  clusterOf: Map<string, number>
): Set<number> {
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

  return referencingClusters;
}

/** Compute median start line (centroid) for a cluster. */
function computeClusterCentroid(
  cluster: Cluster,
  fnBySessionId: Map<string, FunctionNode>
): number | undefined {
  const lines: number[] = [];
  for (const memberId of cluster.members) {
    const fn = fnBySessionId.get(memberId);
    const line = fn?.path.node.loc?.start.line;
    if (line !== undefined) lines.push(line);
  }
  if (lines.length === 0) return undefined;
  lines.sort((a, b) => a - b);
  return lines[Math.floor(lines.length / 2)];
}

/** Find the nearest target cluster to a source centroid. */
function findNearestTarget(
  sourceCentroid: number,
  targets: number[],
  centroids: Map<number, number>,
  clusters: Cluster[]
): number {
  let bestTarget = -1;
  let bestDist = Infinity;
  for (const ti of targets) {
    const centroid = centroids.get(ti);
    if (centroid === undefined) continue;
    const dist = Math.abs(sourceCentroid - centroid);
    if (
      dist < bestDist ||
      (dist === bestDist &&
        (bestTarget === -1 || clusters[ti].id < clusters[bestTarget].id))
    ) {
      bestTarget = ti;
      bestDist = dist;
    }
  }
  return bestTarget;
}

/** Classify cluster indices into sources (small) and targets (large). */
function classifyProximityClusters(
  clusters: Cluster[],
  threshold: number
): { sources: number[]; targets: number[] } {
  const maxSize = Math.max(...clusters.map((c) => c.members.size));
  const sources: number[] = [];
  const targets: number[] = [];
  for (let i = 0; i < clusters.length; i++) {
    if (
      clusters[i].members.size <= threshold &&
      clusters[i].members.size < maxSize
    ) {
      sources.push(i);
    } else {
      targets.push(i);
    }
  }
  return { sources, targets };
}

/** Recompute fingerprints for all clusters in place and sort by ID. */
function recomputeFingerprints(
  clusters: Cluster[],
  fnBySessionId: Map<string, FunctionNode>
): void {
  for (const cluster of clusters) {
    cluster.memberHashes = Array.from(cluster.members)
      .map((id) => fnBySessionId.get(id)!.fingerprint.exactHash)
      .sort();
    cluster.id = createHash("sha256")
      .update(cluster.memberHashes.join(","))
      .digest("hex")
      .slice(0, 16);
  }
  clusters.sort((a, b) => a.id.localeCompare(b.id));
}

/** Merge sources into nearest targets by centroid proximity. Returns set of merged indices. */
function mergeSourcesIntoTargets(
  clusters: Cluster[],
  sources: number[],
  targets: number[],
  centroids: Map<number, number>,
  fnBySessionId: Map<string, FunctionNode>
): Set<number> {
  const merged = new Set<number>();
  for (const si of sources) {
    const sourceCentroid = computeClusterCentroid(clusters[si], fnBySessionId);
    if (sourceCentroid === undefined) continue;
    const bestTarget = findNearestTarget(
      sourceCentroid,
      targets,
      centroids,
      clusters
    );
    if (bestTarget >= 0) {
      for (const member of clusters[si].members) {
        clusters[bestTarget].members.add(member);
      }
      merged.add(si);
    }
  }
  return merged;
}

/**
 * Merge isolated clusters (no inter-cluster call edges) into the nearest
 * connected cluster by source line proximity.
 */
function mergeByProximity(
  result: ClusterResult,
  fnBySessionId: Map<string, FunctionNode>,
  minSize: number
): ClusterResult {
  const clusters = [...result.clusters];
  const shared = new Set(result.shared);
  const orphans = new Set(result.orphans);

  const threshold = minSize > 0 ? minSize : 1;
  const { sources, targets } = classifyProximityClusters(clusters, threshold);

  if (targets.length === 0 || sources.length === 0) {
    return result;
  }

  // Compute centroid for each target cluster
  const centroids = new Map<number, number>();
  for (const ti of targets) {
    const centroid = computeClusterCentroid(clusters[ti], fnBySessionId);
    if (centroid !== undefined) centroids.set(ti, centroid);
  }

  const merged = mergeSourcesIntoTargets(
    clusters,
    sources,
    targets,
    centroids,
    fnBySessionId
  );
  const remaining = clusters.filter((_, i) => !merged.has(i));
  recomputeFingerprints(remaining, fnBySessionId);
  return { clusters: remaining, shared, orphans };
}
