import type { FunctionNode } from "../analysis/types.js";
import type { Cluster } from "./types.js";

/** Add all members of a cluster to a sessionId → cluster index map. */
function indexClusterMembers(
  clusterIdx: number,
  members: Set<string>,
  clusterOf: Map<string, number>
): void {
  for (const member of members) {
    clusterOf.set(member, clusterIdx);
  }
}

/** Build a map from sessionId to cluster index. */
function buildClusterOf(clusters: Cluster[]): Map<string, number> {
  const clusterOf = new Map<string, number>();
  for (let i = 0; i < clusters.length; i++) {
    indexClusterMembers(i, clusters[i].members, clusterOf);
  }
  return clusterOf;
}

/** Record a single call edge between two cluster indices. */
function recordEdge(
  ci: number,
  cj: number,
  intra: number[],
  inter: number[]
): void {
  if (ci === cj) {
    intra[ci]++;
  } else {
    inter[ci]++;
    inter[cj]++;
  }
}

/** Count intra-cluster and inter-cluster edges for each cluster. */
function countEdges(
  allFunctions: FunctionNode[],
  clusterOf: Map<string, number>,
  clusterCount: number
): { intra: number[]; inter: number[] } {
  const intra = new Array(clusterCount).fill(0);
  const inter = new Array(clusterCount).fill(0);

  for (const fn of allFunctions) {
    if (fn.scopeParent) continue;
    const ci = clusterOf.get(fn.sessionId);
    if (ci === undefined) continue;

    for (const callee of fn.internalCallees) {
      if (callee.scopeParent) continue;
      const cj = clusterOf.get(callee.sessionId);
      if (cj === undefined) continue;
      recordEdge(ci, cj, intra, inter);
    }
  }

  return { intra, inter };
}

/** Compute the MQ sum from intra/inter edge arrays. */
function computeMQSum(intra: number[], inter: number[]): number {
  let sum = 0;
  for (let i = 0; i < intra.length; i++) {
    const denom = intra[i] + 0.5 * inter[i];
    if (denom > 0) {
      sum += intra[i] / denom;
    }
  }
  return sum;
}

/**
 * Compute Bunch's Modularization Quality (MQ) metric.
 *
 * MQ = (1/k) * sum(MF_i) where:
 * - k = number of clusters
 * - MF_i = intra_i / (intra_i + 0.5 * inter_i) if denominator > 0, else 0
 * - intra_i = edges within cluster i
 * - inter_i = edges between cluster i and other clusters
 *
 * Higher MQ = better modularization. Range: [0, 1] for typical partitions.
 */
export function computeMQ(
  clusters: Cluster[],
  allFunctions: FunctionNode[]
): number {
  if (clusters.length === 0) return 0;

  const clusterOf = buildClusterOf(clusters);
  const { intra, inter } = countEdges(allFunctions, clusterOf, clusters.length);
  return computeMQSum(intra, inter) / clusters.length;
}
