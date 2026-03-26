/**
 * Split adapter using call-graph reachability clustering.
 *
 * This is the fallback adapter — it always supports any input.
 * Groups functions by analyzing which functions call which, building
 * clusters from root functions and their transitive callees.
 */
import * as t from "@babel/types";
import type { FunctionNode } from "../../analysis/types.js";
import { clusterFunctions } from "../cluster.js";
import { collectReferencedNames } from "../emitter.js";
import type { DetectionResult } from "../module-detect.js";
import { nameCluster } from "../naming.js";
import type { Cluster, ParsedFile } from "../types.js";
import type { SplitAdapter, SplitAdapterOptions } from "./types.js";

export class CallGraphAdapter implements SplitAdapter {
  name = "call-graph" as const;

  supports(_detection: DetectionResult): boolean {
    return true;
  }

  groupFunctions(
    functions: FunctionNode[],
    parsedFiles: ParsedFile[],
    _detection: DetectionResult,
    options?: SplitAdapterOptions
  ): Map<string, string> {
    const { clusters, shared, orphans } = clusterFunctions(functions, options);
    reassignPublicOrphans(parsedFiles, functions, clusters, orphans);
    const functionNames = buildFunctionNameMap(functions);
    return buildClusterFileMap(clusters, shared, orphans, functionNames);
  }
}

// ── Helpers (moved from index.ts) ───────────────────────────────────

/** Build a map from function sessionId to function name. */
export function buildFunctionNameMap(
  allFunctions: FunctionNode[]
): Map<string, string> {
  const functionNames = new Map<string, string>();
  for (const fn of allFunctions) {
    const node = fn.path.node;
    if ("id" in node && node.id && node.id.name) {
      functionNames.set(fn.sessionId, node.id.name);
    }
  }
  return functionNames;
}

/** Build sessionId -> output filename map from clusters, shared, and orphans. */
export function buildClusterFileMap(
  clusters: Cluster[],
  shared: Set<string>,
  orphans: Set<string>,
  functionNames: Map<string, string>
): Map<string, string> {
  const clusterFileMap = new Map<string, string>();

  for (const cluster of clusters) {
    const fileName = nameCluster(cluster, functionNames);
    for (const member of cluster.members) {
      clusterFileMap.set(member, fileName);
    }
  }

  for (const sessionId of shared) {
    clusterFileMap.set(sessionId, "shared.js");
  }

  for (const sessionId of orphans) {
    clusterFileMap.set(sessionId, "orphans.js");
  }

  return clusterFileMap;
}

/** Add export specifier local names from a barrel export to the set. */
function addBarrelExportSpecifiers(
  stmt: t.ExportNamedDeclaration,
  names: Set<string>
): void {
  for (const spec of stmt.specifiers) {
    if (t.isExportSpecifier(spec)) {
      names.add(spec.local.name);
    }
  }
}

/** Collect barrel export names (export { ... }) from all parsed files. */
function collectBarrelExportNames(parsedFiles: ParsedFile[]): Set<string> {
  const barrelExportNames = new Set<string>();
  for (const { ast } of parsedFiles) {
    for (const stmt of ast.program.body) {
      if (
        t.isExportNamedDeclaration(stmt) &&
        !stmt.declaration &&
        stmt.specifiers.length > 0
      ) {
        addBarrelExportSpecifiers(stmt, barrelExportNames);
      }
    }
  }
  return barrelExportNames;
}

/** Build a map from orphan sessionId to function name. */
function buildOrphanNames(
  allFunctions: FunctionNode[],
  orphans: Set<string>
): Map<string, string> {
  const orphanNames = new Map<string, string>();
  for (const fn of allFunctions) {
    if (!orphans.has(fn.sessionId)) continue;
    const node = fn.path.node;
    if ("id" in node && node.id && node.id.name) {
      orphanNames.set(fn.sessionId, node.id.name);
    }
  }
  return orphanNames;
}

/** Build a map from function name to cluster index. */
function buildNameToClusterIdx(
  clusters: Cluster[],
  allFunctions: FunctionNode[]
): Map<string, number> {
  const nameToClusterIdx = new Map<string, number>();
  for (let ci = 0; ci < clusters.length; ci++) {
    for (const memberId of clusters[ci].members) {
      const fn = allFunctions.find((f) => f.sessionId === memberId);
      if (fn) {
        const node = fn.path.node;
        if ("id" in node && node.id && node.id.name) {
          nameToClusterIdx.set(node.id.name, ci);
        }
      }
    }
  }
  return nameToClusterIdx;
}

/** Pick the cluster with the highest count; tiebreak by cluster ID. */
function pickBestClusterByCount(
  clusterCounts: Map<number, number>,
  clusters: Cluster[]
): number {
  let bestCluster = -1;
  let bestCount = 0;
  for (const [ci, count] of clusterCounts) {
    if (
      count > bestCount ||
      (count === bestCount &&
        (bestCluster === -1 || clusters[ci].id < clusters[bestCluster].id))
    ) {
      bestCluster = ci;
      bestCount = count;
    }
  }
  return bestCluster;
}

/** Pick the largest cluster; tiebreak by cluster ID. */
function pickLargestCluster(clusters: Cluster[]): number {
  let bestCluster = -1;
  let maxSize = 0;
  for (let ci = 0; ci < clusters.length; ci++) {
    if (
      clusters[ci].members.size > maxSize ||
      (clusters[ci].members.size === maxSize &&
        (bestCluster === -1 || clusters[ci].id < clusters[bestCluster].id))
    ) {
      maxSize = clusters[ci].members.size;
      bestCluster = ci;
    }
  }
  return bestCluster;
}

/** Find best cluster for an orphan by reference counting then size fallback. */
function findBestClusterForOrphan(
  fn: FunctionNode,
  clusters: Cluster[],
  nameToClusterIdx: Map<string, number>
): number {
  const bodyNode = fn.path.node;
  const refs = collectReferencedNames(
    t.isFunctionDeclaration(bodyNode)
      ? bodyNode
      : t.expressionStatement(bodyNode as t.Expression)
  );

  const clusterCounts = new Map<number, number>();
  for (const ref of refs) {
    const ci = nameToClusterIdx.get(ref);
    if (ci !== undefined) {
      clusterCounts.set(ci, (clusterCounts.get(ci) ?? 0) + 1);
    }
  }

  if (clusterCounts.size > 0) {
    return pickBestClusterByCount(clusterCounts, clusters);
  }

  return pickLargestCluster(clusters);
}

/**
 * Reassign orphan functions that are public exports to the best-matching cluster.
 */
function reassignPublicOrphans(
  parsedFiles: ParsedFile[],
  allFunctions: FunctionNode[],
  clusters: Cluster[],
  orphans: Set<string>
): void {
  if (orphans.size === 0 || clusters.length === 0) return;

  const barrelExportNames = collectBarrelExportNames(parsedFiles);
  if (barrelExportNames.size === 0) return;

  const orphanNames = buildOrphanNames(allFunctions, orphans);
  const nameToClusterIdx = buildNameToClusterIdx(clusters, allFunctions);

  const toReassign: Array<{ sessionId: string; clusterIdx: number }> = [];

  for (const sessionId of orphans) {
    const fnName = orphanNames.get(sessionId);
    if (!fnName || !barrelExportNames.has(fnName)) continue;

    const fn = allFunctions.find((f) => f.sessionId === sessionId);
    if (!fn) continue;

    const bestCluster = findBestClusterForOrphan(
      fn,
      clusters,
      nameToClusterIdx
    );
    if (bestCluster >= 0) {
      toReassign.push({ sessionId, clusterIdx: bestCluster });
    }
  }

  for (const { sessionId, clusterIdx } of toReassign) {
    orphans.delete(sessionId);
    clusters[clusterIdx].members.add(sessionId);
  }
}
