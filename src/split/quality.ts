import type { FunctionNode } from "../analysis/types.js";
import type { Cluster } from "./types.js";

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
export function computeMQ(clusters: Cluster[], allFunctions: FunctionNode[]): number {
  if (clusters.length === 0) return 0;

  // Build sessionId → cluster index map
  const clusterOf = new Map<string, number>();
  for (let i = 0; i < clusters.length; i++) {
    for (const member of clusters[i].members) {
      clusterOf.set(member, i);
    }
  }

  // Build sessionId → FunctionNode map (top-level only)
  const fnById = new Map<string, FunctionNode>();
  for (const fn of allFunctions) {
    if (!fn.scopeParent) {
      fnById.set(fn.sessionId, fn);
    }
  }

  // Count intra and inter edges per cluster
  const intra = new Array(clusters.length).fill(0);
  const inter = new Array(clusters.length).fill(0);

  for (const fn of allFunctions) {
    if (fn.scopeParent) continue;
    const ci = clusterOf.get(fn.sessionId);
    if (ci === undefined) continue;

    for (const callee of fn.internalCallees) {
      if (callee.scopeParent) continue;
      const cj = clusterOf.get(callee.sessionId);
      if (cj === undefined) continue;

      if (ci === cj) {
        intra[ci]++;
      } else {
        inter[ci]++;
        inter[cj]++;
      }
    }
  }

  // Compute MQ
  let sum = 0;
  for (let i = 0; i < clusters.length; i++) {
    const denom = intra[i] + 0.5 * inter[i];
    if (denom > 0) {
      sum += intra[i] / denom;
    }
  }

  return sum / clusters.length;
}
