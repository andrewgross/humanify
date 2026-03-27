/**
 * Co-reference clustering for sparse call graphs.
 *
 * When bundlers like esbuild hoist all functions to the top level, the call
 * graph becomes empty (no internalCallees between top-level functions).
 * However, identifier references survive: functions from the same original
 * file reference the same variables, utilities, and types.
 *
 * This module clusters functions by IDF-weighted Jaccard similarity of their
 * referenced name sets, using an inverted-index approach to avoid O(n^2).
 */
import { createHash } from "node:crypto";
import * as t from "@babel/types";
import type { FunctionNode } from "../analysis/types.js";
import { collectReferencedNames } from "./emitter.js";
import { nameCluster } from "./naming.js";
import type { Cluster, ParsedFile } from "./types.js";

export interface SimilarityEdge {
  target: string;
  weight: number;
}

/**
 * Compute IDF weights for referenced names.
 * idf(name) = log(N / count_of_functions_referencing_name)
 * Rare names (high IDF) are stronger clustering signals.
 */
export function computeIdfWeights(
  refSets: Map<string, Set<string>>
): Map<string, number> {
  const N = refSets.size;
  if (N === 0) return new Map();

  const docFreq = new Map<string, number>();
  for (const refs of refSets.values()) {
    for (const name of refs) {
      docFreq.set(name, (docFreq.get(name) ?? 0) + 1);
    }
  }

  const idf = new Map<string, number>();
  for (const [name, count] of docFreq) {
    idf.set(name, Math.log(N / count));
  }
  return idf;
}

/**
 * Compute what fraction of top-level functions have zero top-level call edges.
 * Returns 0.0 (fully connected) to 1.0 (no edges).
 */
export function computeSparsity(topLevel: FunctionNode[]): number {
  if (topLevel.length === 0) return 1.0;
  const topLevelIds = new Set(topLevel.map((fn) => fn.sessionId));

  let zeroEdge = 0;
  for (const fn of topLevel) {
    const hasTopLevelCallee = Array.from(fn.internalCallees).some((c) =>
      topLevelIds.has(c.sessionId)
    );
    if (!hasTopLevelCallee) zeroEdge++;
  }

  return zeroEdge / topLevel.length;
}

// ── Similarity graph building ─────────────────────────────────────────

/** Build inverted index: name → [function IDs], skipping zero-IDF names. */
function buildInvertedIndex(
  refSets: Map<string, Set<string>>,
  idf: Map<string, number>
): Map<string, string[]> {
  const invertedIndex = new Map<string, string[]>();
  for (const [fnId, refs] of refSets) {
    for (const name of refs) {
      const w = idf.get(name) ?? 0;
      if (w <= 0) continue;
      let list = invertedIndex.get(name);
      if (!list) {
        list = [];
        invertedIndex.set(name, list);
      }
      list.push(fnId);
    }
  }
  return invertedIndex;
}

/** Collect pairwise shared IDF weight from inverted index. */
function collectPairWeights(
  invertedIndex: Map<string, string[]>,
  idf: Map<string, number>
): Map<string, number> {
  const pairWeights = new Map<string, number>();
  for (const [name, fnIds] of invertedIndex) {
    if (fnIds.length < 2) continue;
    const w = idf.get(name) ?? 0;
    for (let i = 0; i < fnIds.length; i++) {
      for (let j = i + 1; j < fnIds.length; j++) {
        const key =
          fnIds[i] < fnIds[j]
            ? `${fnIds[i]}|${fnIds[j]}`
            : `${fnIds[j]}|${fnIds[i]}`;
        pairWeights.set(key, (pairWeights.get(key) ?? 0) + w);
      }
    }
  }
  return pairWeights;
}

/** Add similarity edges for a single candidate pair. */
function addSimilarityEdge(
  graph: Map<string, SimilarityEdge[]>,
  idA: string,
  idB: string,
  sharedWeight: number,
  refSets: Map<string, Set<string>>,
  idf: Map<string, number>
): void {
  const refsA = refSets.get(idA);
  const refsB = refSets.get(idB);
  if (!refsA || !refsB) return;

  let unionWeight = 0;
  const allNames = new Set([...refsA, ...refsB]);
  for (const name of allNames) {
    unionWeight += idf.get(name) ?? 0;
  }

  const similarity = unionWeight > 0 ? sharedWeight / unionWeight : 0;
  if (similarity > 0) {
    graph.get(idA)?.push({ target: idB, weight: similarity });
    graph.get(idB)?.push({ target: idA, weight: similarity });
  }
}

/**
 * Build a similarity graph using an inverted index.
 * This is O(n * avg_shared_names) instead of O(n^2).
 */
export function buildSimilarityGraph(
  refSets: Map<string, Set<string>>,
  idf: Map<string, number>
): Map<string, SimilarityEdge[]> {
  const graph = new Map<string, SimilarityEdge[]>();
  for (const id of refSets.keys()) {
    graph.set(id, []);
  }

  if (refSets.size <= 1) return graph;

  const invertedIndex = buildInvertedIndex(refSets, idf);
  const pairWeights = collectPairWeights(invertedIndex, idf);

  for (const [key, sharedWeight] of pairWeights) {
    const [idA, idB] = key.split("|");
    addSimilarityEdge(graph, idA, idB, sharedWeight, refSets, idf);
  }

  return graph;
}

// ── Community detection ───────────────────────────────────────────────

/** Re-number communities to be 0, 1, 2, ... with no gaps. */
function compactCommunityIds(
  community: Map<string, number>
): Map<string, number> {
  const oldToNew = new Map<number, number>();
  const result = new Map<string, number>();

  const entries = Array.from(community.entries()).sort((a, b) =>
    a[0].localeCompare(b[0])
  );

  for (const [node, oldId] of entries) {
    if (!oldToNew.has(oldId)) {
      oldToNew.set(oldId, oldToNew.size);
    }
    result.set(node, oldToNew.get(oldId) ?? 0);
  }

  return result;
}

/** Accumulate cross-community edge weights into a pair-weight map. */
function accumulatePairWeights(
  graph: Map<string, SimilarityEdge[]>,
  community: Map<string, number>
): Map<string, { comA: number; comB: number; weight: number }> {
  const pairWeights = new Map<
    string,
    { comA: number; comB: number; weight: number }
  >();

  for (const [node, edges] of graph) {
    const comA = community.get(node) ?? 0;
    for (const edge of edges) {
      const comB = community.get(edge.target) ?? 0;
      if (comA === comB) continue;
      const lo = Math.min(comA, comB);
      const hi = Math.max(comA, comB);
      const key = `${lo}|${hi}`;
      const existing = pairWeights.get(key);
      if (existing) {
        existing.weight += edge.weight;
      } else {
        pairWeights.set(key, { comA: lo, comB: hi, weight: edge.weight });
      }
    }
  }

  return pairWeights;
}

/**
 * Find the two communities with the strongest inter-community edge weight.
 * Returns [comA, comB] to merge, or null if no communities share edges.
 */
function findMostConnectedPair(
  graph: Map<string, SimilarityEdge[]>,
  community: Map<string, number>
): [number, number] | null {
  const pairWeights = accumulatePairWeights(graph, community);

  let best: { comA: number; comB: number; weight: number } | null = null;
  for (const pair of pairWeights.values()) {
    if (!best || pair.weight > best.weight) best = pair;
  }

  return best ? [best.comA, best.comB] : null;
}

/**
 * Community detection via agglomerative clustering.
 * Starts with each node in its own community, then iteratively merges
 * the two communities with the strongest inter-community edge weight
 * until reaching the target count or no connected pairs remain.
 */
function detectCommunities(
  graph: Map<string, SimilarityEdge[]>,
  targetCount: number
): Map<string, number> {
  const nodes = Array.from(graph.keys()).sort();
  if (nodes.length === 0) return new Map();

  const community = new Map<string, number>();
  for (let i = 0; i < nodes.length; i++) {
    community.set(nodes[i], i);
  }

  let numCommunities = nodes.length;

  while (numCommunities > targetCount) {
    const pair = findMostConnectedPair(graph, community);
    if (!pair) break;

    const [mergeTo, mergeFrom] = pair;
    for (const [node, com] of community) {
      if (com === mergeFrom) community.set(node, mergeTo);
    }
    numCommunities--;
  }

  return compactCommunityIds(community);
}

// ── Bundler-specific signal extraction ────────────────────────────────

interface BundlerAffinityBoost {
  pairBoosts: Map<string, number>;
}

/** Create a sorted pair key for two session IDs. */
function pairKey(idA: string, idB: string): string {
  return idA < idB ? `${idA}|${idB}` : `${idB}|${idA}`;
}

/** Add pairwise boosts for all names in a group. */
function addGroupBoosts(
  sessionIds: string[],
  weight: number,
  pairBoosts: Map<string, number>
): void {
  for (let i = 0; i < sessionIds.length; i++) {
    for (let j = i + 1; j < sessionIds.length; j++) {
      const key = pairKey(sessionIds[i], sessionIds[j]);
      pairBoosts.set(key, (pairBoosts.get(key) ?? 0) + weight);
    }
  }
}

/** Extract names from an __export block body string. */
function extractExportNames(
  body: string,
  nameToSessionId: Map<string, string>
): string[] {
  const names: string[] = [];
  const nameExtractor = /(\w+)\s*:/g;
  for (const m of body.matchAll(nameExtractor)) {
    if (nameToSessionId.has(m[1])) {
      names.push(m[1]);
    }
  }
  return names;
}

/** Extract __export() block signals from source. */
function extractExportSignals(
  source: string,
  nameToSessionId: Map<string, string>,
  pairBoosts: Map<string, number>
): void {
  const exportPattern = /__export\(\s*\w+\s*,\s*\{([^}]+)\}\s*\)/g;
  for (const match of source.matchAll(exportPattern)) {
    const names = extractExportNames(match[1], nameToSessionId);
    const ids = names
      .map((n) => nameToSessionId.get(n))
      .filter((id): id is string => !!id);
    addGroupBoosts(ids, 2.0, pairBoosts);
  }
}

/** Extract __name() call signals and group by name prefix. */
function extractNamePrefixSignals(
  source: string,
  nameToSessionId: Map<string, string>,
  pairBoosts: Map<string, number>
): void {
  const namePattern = /__name\(\s*(\w+)\s*,\s*"([^"]+)"\s*\)/g;
  const prefixGroups = new Map<string, string[]>();

  for (const match of source.matchAll(namePattern)) {
    const fnName = match[1];
    const originalName = match[2];
    const sessionId = nameToSessionId.get(fnName);
    if (!sessionId) continue;

    const prefix = extractNamePrefix(originalName);
    if (prefix.length >= 3) {
      let group = prefixGroups.get(prefix);
      if (!group) {
        group = [];
        prefixGroups.set(prefix, group);
      }
      group.push(sessionId);
    }
  }

  for (const group of prefixGroups.values()) {
    if (group.length >= 2 && group.length <= 50) {
      addGroupBoosts(group, 0.5, pairBoosts);
    }
  }
}

/** Extract meaningful prefix from a camelCase/PascalCase name. */
function extractNamePrefix(name: string): string {
  const parts = name.replace(/([a-z])([A-Z])/g, "$1|$2").split("|");
  if (parts.length >= 2) return parts[0];
  return name.length <= 8 ? name : name.slice(0, 6);
}

/**
 * Extract esbuild-specific signals that provide strong clustering hints.
 */
export function extractBundlerSignals(
  source: string,
  topLevel: FunctionNode[]
): BundlerAffinityBoost {
  const pairBoosts = new Map<string, number>();

  const nameToSessionId = new Map<string, string>();
  for (const fn of topLevel) {
    const node = fn.path.node;
    if ("id" in node && node.id && node.id.name) {
      nameToSessionId.set(node.id.name, fn.sessionId);
    }
  }

  extractExportSignals(source, nameToSessionId, pairBoosts);
  extractNamePrefixSignals(source, nameToSessionId, pairBoosts);

  return { pairBoosts };
}

/** Apply bundler affinity boosts to a similarity graph. */
function applyBundlerBoosts(
  graph: Map<string, SimilarityEdge[]>,
  boosts: BundlerAffinityBoost
): void {
  for (const [key, boost] of boosts.pairBoosts) {
    const [idA, idB] = key.split("|");
    const edgesA = graph.get(idA);
    const edgesB = graph.get(idB);
    if (!edgesA || !edgesB) continue;

    const existing = edgesA.find((e) => e.target === idB);
    if (existing) {
      existing.weight += boost;
      const reverse = edgesB.find((e) => e.target === idA);
      if (reverse) reverse.weight += boost;
    } else {
      edgesA.push({ target: idB, weight: boost });
      edgesB.push({ target: idA, weight: boost });
    }
  }
}

// ── Reference sets and helpers ────────────────────────────────────────

/** Convert function node to a statement for collectReferencedNames. */
function fnToStatement(node: t.Function): t.Statement {
  if (t.isFunctionDeclaration(node)) return node;
  // ClassMethod/ObjectMethod/ClassPrivateMethod — use their block body
  if ("body" in node && t.isBlockStatement(node.body)) return node.body;
  return t.expressionStatement(node as unknown as t.Expression);
}

/** Build reference sets for all top-level functions. */
function buildReferenceSets(
  topLevel: FunctionNode[]
): Map<string, Set<string>> {
  const refSets = new Map<string, Set<string>>();
  for (const fn of topLevel) {
    const stmt = fnToStatement(fn.path.node);
    const refs = collectReferencedNames(stmt);
    refSets.set(fn.sessionId, refs);
  }
  return refSets;
}

/**
 * Estimate the number of output files from function count and total lines.
 */
export function estimateFileCount(
  totalFunctions: number,
  totalLines: number
): number {
  if (totalFunctions <= 3) return 1;

  const byFunctions = Math.round(totalFunctions / 40);
  const byLines = Math.round(totalLines / 300);
  const estimate = Math.round(Math.sqrt(byFunctions * byLines));
  return Math.max(2, Math.min(estimate, Math.floor(totalFunctions / 2)));
}

// ── Cluster building ──────────────────────────────────────────────────

/** Build Cluster objects from community assignments. */
function buildClustersFromCommunities(
  communityMap: Map<string, number>,
  fnBySessionId: Map<string, FunctionNode>
): { clusters: Cluster[]; orphans: Set<string> } {
  const groups = new Map<number, Set<string>>();
  for (const [sessionId, comId] of communityMap) {
    let group = groups.get(comId);
    if (!group) {
      group = new Set();
      groups.set(comId, group);
    }
    group.add(sessionId);
  }

  const clusters: Cluster[] = [];
  const orphans = new Set<string>();

  for (const members of groups.values()) {
    if (members.size === 1) {
      const first = members.values().next().value;
      if (first) orphans.add(first);
      continue;
    }

    const memberHashes = Array.from(members)
      .map((id) => fnBySessionId.get(id)?.fingerprint.exactHash ?? "")
      .sort();

    const fingerprint = createHash("sha256")
      .update(memberHashes.join(","))
      .digest("hex")
      .slice(0, 16);

    const sortedMembers = Array.from(members).sort();
    clusters.push({
      id: fingerprint,
      rootFunctions: [sortedMembers[0]],
      members,
      memberHashes
    });
  }

  clusters.sort((a, b) => a.id.localeCompare(b.id));
  return { clusters, orphans };
}

/** Find the best cluster for an orphan by IDF-weighted reference overlap. */
function findBestClusterForOrphan(
  orphanRefs: Set<string>,
  clusters: Cluster[],
  clusterRefs: Set<string>[],
  idf: Map<string, number>
): number {
  let bestCluster = 0;
  let bestScore = -1;
  for (let i = 0; i < clusters.length; i++) {
    let score = 0;
    for (const name of orphanRefs) {
      if (clusterRefs[i].has(name)) {
        score += idf.get(name) ?? 0;
      }
    }
    if (
      score > bestScore ||
      (score === bestScore && clusters[i].id < clusters[bestCluster].id)
    ) {
      bestScore = score;
      bestCluster = i;
    }
  }
  return bestCluster;
}

/** Assign orphan singletons to nearest cluster by reference overlap. */
function assignOrphans(
  orphans: Set<string>,
  clusters: Cluster[],
  refSets: Map<string, Set<string>>,
  idf: Map<string, number>
): void {
  if (clusters.length === 0 || orphans.size === 0) return;

  const clusterRefs: Set<string>[] = clusters.map((cluster) => {
    const union = new Set<string>();
    for (const memberId of cluster.members) {
      const refs = refSets.get(memberId);
      if (refs) {
        for (const r of refs) union.add(r);
      }
    }
    return union;
  });

  for (const orphanId of orphans) {
    const orphanRefs = refSets.get(orphanId);
    if (!orphanRefs || orphanRefs.size === 0) {
      // No references → assign to largest cluster
      let largest = 0;
      for (let i = 1; i < clusters.length; i++) {
        if (clusters[i].members.size > clusters[largest].members.size) {
          largest = i;
        }
      }
      clusters[largest].members.add(orphanId);
      continue;
    }

    const best = findBestClusterForOrphan(
      orphanRefs,
      clusters,
      clusterRefs,
      idf
    );
    clusters[best].members.add(orphanId);
  }
}

// ── Main entry point ──────────────────────────────────────────────────

/**
 * Cluster functions by co-reference similarity.
 *
 * Returns Map<sessionId, outputFileName> — same interface as other adapters.
 */
export function referenceCluster(
  functions: FunctionNode[],
  parsedFiles: ParsedFile[],
  targetFileCount?: number
): Map<string, string> {
  const topLevel = functions.filter((fn) => !fn.scopeParent);
  if (topLevel.length === 0) return new Map();

  const fnBySessionId = new Map<string, FunctionNode>();
  for (const fn of topLevel) {
    fnBySessionId.set(fn.sessionId, fn);
  }

  // 1. Build reference vectors
  const refSets = buildReferenceSets(topLevel);

  // 2. Compute IDF weights
  const idf = computeIdfWeights(refSets);

  // 3. Build similarity graph
  const graph = buildSimilarityGraph(refSets, idf);

  // 3b. Apply bundler-specific signal boosts
  const source = parsedFiles.map((pf) => pf.source).join("\n");
  if (source.length > 0) {
    const boosts = extractBundlerSignals(source, topLevel);
    applyBundlerBoosts(graph, boosts);
  }

  // 4. Estimate target file count if not provided
  const totalLines = parsedFiles.reduce(
    (sum, pf) => sum + (pf.source?.split("\n").length ?? 0),
    0
  );
  const target =
    targetFileCount ?? estimateFileCount(topLevel.length, totalLines);

  // 5. Community detection
  const communityMap = detectCommunities(graph, target);

  // 6. Build clusters and handle orphans
  const { clusters, orphans } = buildClustersFromCommunities(
    communityMap,
    fnBySessionId
  );

  assignOrphans(orphans, clusters, refSets, idf);

  // 7. Build output map: sessionId → filename
  const functionNames = new Map<string, string>();
  for (const fn of topLevel) {
    const node = fn.path.node;
    if ("id" in node && node.id && node.id.name) {
      functionNames.set(fn.sessionId, node.id.name);
    }
  }

  const result = new Map<string, string>();
  for (const cluster of clusters) {
    const fileName = nameCluster(cluster, functionNames);
    for (const memberId of cluster.members) {
      result.set(memberId, fileName);
    }
  }

  // Catch any unassigned functions
  for (const fn of topLevel) {
    if (!result.has(fn.sessionId)) {
      result.set(fn.sessionId, "orphans.js");
    }
  }

  return result;
}
