/**
 * Proximity-based agglomerative merging of fine-grained clusters.
 *
 * esbuild places each original module's code contiguously in the bundle.
 * Functions from the same module are adjacent. We exploit this by merging
 * clusters whose functions are spatially close in the bundle.
 *
 * Algorithm:
 *   1. Start with N fine-grained clusters (from call-graph clustering)
 *   2. For each cluster, compute its "span" (min/max line numbers)
 *   3. Repeatedly merge the pair of clusters with the smallest gap between spans
 *   4. Stop when target count is reached or min gap exceeds threshold
 */
import type { FunctionNode } from "../src/analysis/types.js";

// ── Direct proximity grouping (bypass call graph) ───────────────────

export interface DirectGroupingOptions {
  /** Target number of output groups. */
  targetCount: number;
}

/**
 * Group functions into clusters by source line proximity alone.
 *
 * Algorithm: sort by start line, find the (targetCount - 1) largest gaps
 * between consecutive functions, split there. No call graph needed.
 */
export function groupByProximity(
  functions: FunctionNode[],
  options: DirectGroupingOptions
): Map<string, string> {
  const topLevel = functions
    .filter((fn) => !fn.scopeParent)
    .sort(
      (a, b) =>
        (a.path.node.loc?.start.line ?? 0) - (b.path.node.loc?.start.line ?? 0)
    );

  if (topLevel.length === 0) return new Map();

  // Compute gaps between consecutive functions
  const gaps: Array<{ index: number; gap: number }> = [];
  for (let i = 1; i < topLevel.length; i++) {
    const prevEnd = topLevel[i - 1].path.node.loc?.end.line ?? 0;
    const nextStart = topLevel[i].path.node.loc?.start.line ?? 0;
    gaps.push({ index: i, gap: nextStart - prevEnd });
  }

  // Find the (targetCount - 1) largest gaps as split points
  const numSplits = Math.min(options.targetCount - 1, gaps.length);
  const sortedGaps = [...gaps].sort((a, b) => b.gap - a.gap);
  const splitIndices = new Set(
    sortedGaps.slice(0, numSplits).map((g) => g.index)
  );

  // Assign functions to groups
  const clusterFileMap = new Map<string, string>();
  let groupIdx = 0;
  let groupName = `group_${groupIdx}`;

  for (let i = 0; i < topLevel.length; i++) {
    if (splitIndices.has(i)) {
      groupIdx++;
      groupName = `group_${groupIdx}`;
    }
    clusterFileMap.set(topLevel[i].sessionId, groupName);
  }

  return clusterFileMap;
}

// ── Agglomerative proximity merge (post-clustering) ─────────────────

export interface ProximityMergeOptions {
  /** Target number of output clusters. 0 = use gapThreshold instead. */
  targetCount?: number;
  /** Max gap (in lines) for merging. Pairs with gap > this are not merged. */
  gapThreshold?: number;
  /** Fallback: target average lines per file. Used to compute targetCount if not specified. */
  targetAvgFileSize?: number;
}

interface ClusterSpan {
  /** Cluster key (output filename) */
  key: string;
  /** Function IDs in this cluster */
  members: string[];
  /** Minimum line number of any function in the cluster */
  minLine: number;
  /** Maximum line number (end) of any function in the cluster */
  maxLine: number;
}

/** Build span for each cluster from the cluster→file map. */
function buildClusterSpans(
  functions: FunctionNode[],
  clusterFileMap: Map<string, string>
): Map<string, ClusterSpan> {
  const spans = new Map<string, ClusterSpan>();

  for (const fn of functions) {
    if (fn.scopeParent) continue;

    const file = clusterFileMap.get(fn.sessionId);
    if (!file) continue;

    const startLine = fn.path.node.loc?.start.line ?? 0;
    const endLine = fn.path.node.loc?.end.line ?? startLine;

    const span = spans.get(file);
    if (span) {
      span.members.push(fn.sessionId);
      span.minLine = Math.min(span.minLine, startLine);
      span.maxLine = Math.max(span.maxLine, endLine);
    } else {
      spans.set(file, {
        key: file,
        members: [fn.sessionId],
        minLine: startLine,
        maxLine: endLine
      });
    }
  }

  return spans;
}

/** Compute gap between two spans (negative if overlapping). */
function spanGap(a: ClusterSpan, b: ClusterSpan): number {
  if (a.maxLine < b.minLine) return b.minLine - a.maxLine;
  if (b.maxLine < a.minLine) return a.minLine - b.maxLine;
  return 0; // overlapping
}

/** Merge span b into span a. */
function mergeSpans(a: ClusterSpan, b: ClusterSpan): void {
  a.members.push(...b.members);
  a.minLine = Math.min(a.minLine, b.minLine);
  a.maxLine = Math.max(a.maxLine, b.maxLine);
}

/**
 * Merge fine-grained clusters based on source line proximity.
 *
 * Returns a new clusterFileMap with merged assignments.
 */
export function mergeByProximity(
  functions: FunctionNode[],
  clusterFileMap: Map<string, string>,
  totalLines: number,
  options: ProximityMergeOptions = {}
): Map<string, string> {
  const spans = buildClusterSpans(functions, clusterFileMap);

  // Determine target count
  let targetCount = options.targetCount ?? 0;
  if (targetCount === 0 && options.targetAvgFileSize) {
    targetCount = Math.max(
      1,
      Math.ceil(totalLines / options.targetAvgFileSize)
    );
  }
  if (targetCount === 0) {
    // Default: sqrt(originalClusterCount), clamped to reasonable range
    targetCount = Math.max(3, Math.min(50, Math.ceil(Math.sqrt(spans.size))));
  }

  const gapThreshold = options.gapThreshold ?? Infinity;

  // Convert to array for iteration
  let clusters = Array.from(spans.values());

  // Agglomerative merging: repeatedly merge closest pair
  while (clusters.length > targetCount) {
    // Find closest pair
    let bestI = -1;
    let bestJ = -1;
    let bestGap = Infinity;

    for (let i = 0; i < clusters.length; i++) {
      for (let j = i + 1; j < clusters.length; j++) {
        const gap = spanGap(clusters[i], clusters[j]);
        if (gap < bestGap) {
          bestGap = gap;
          bestI = i;
          bestJ = j;
        }
      }
    }

    if (bestGap > gapThreshold) break;
    if (bestI === -1) break;

    // Merge j into i
    mergeSpans(clusters[bestI], clusters[bestJ]);
    clusters.splice(bestJ, 1);
  }

  // Build new clusterFileMap from merged clusters
  const newMap = new Map<string, string>();
  for (const cluster of clusters) {
    // Use the key of the cluster (name from the largest original cluster)
    for (const member of cluster.members) {
      newMap.set(member, cluster.key);
    }
  }

  return newMap;
}
