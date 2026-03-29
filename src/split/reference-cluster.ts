/**
 * Co-reference clustering for sparse call graphs.
 *
 * When bundlers like esbuild hoist all functions to the top level, the call
 * graph becomes empty (no internalCallees between top-level functions).
 * However, identifier references survive: functions from the same original
 * file reference the same variables, utilities, and types.
 *
 * This module clusters functions by IDF-weighted Jaccard similarity of their
 * positional reference sets, using an inverted-index approach to avoid O(n^2).
 *
 * Key insight: instead of tracking referenced identifier NAMES (which minification
 * destroys), we track the DECLARATION POSITIONS of referenced top-level bindings.
 * A variable declared at line 50 is still at line 50 after renaming, so the
 * clustering signal survives minification.
 *
 * When the positional-reference similarity graph is too dense (>50% density
 * among connected nodes, characteristic of minified code with short variable
 * names creating spurious overlaps), falls back to gap-based clustering
 * which splits at byte-offset gaps between consecutive functions.
 */
import { createHash } from "node:crypto";
import type * as babelTraverse from "@babel/traverse";
import type * as t from "@babel/types";
import type { FunctionNode } from "../analysis/types.js";
import { nameCluster } from "./naming.js";
import type { Cluster, ParsedFile } from "./types.js";

// We need full traverse with scope analysis (NOT noScope).
import * as _babelTraverse from "@babel/traverse";
const traverse = (
  typeof _babelTraverse.default === "function"
    ? _babelTraverse.default
    : (_babelTraverse.default as unknown as Record<string, unknown>).default
) as (node: t.Node, opts: Record<string, unknown>) => void;

export interface SimilarityEdge {
  target: string;
  weight: number;
}

// ── Positional reference collection ───────────────────────────────────

/** Check if a binding's scope is the program scope (top-level). */
function isTopLevelBinding(binding: babelTraverse.Binding): boolean {
  return binding.scope.path.isProgram();
}

/** Format a binding's declaration position as a stable string key. */
function formatBindingPosition(binding: babelTraverse.Binding): string {
  const loc = binding.identifier.loc;
  if (loc) {
    return `pos:${loc.start.line}:${loc.start.column}`;
  }
  // Fallback: use the start position of the binding path node
  const nodeLoc = binding.path.node.loc;
  if (nodeLoc) {
    return `pos:${nodeLoc.start.line}:${nodeLoc.start.column}`;
  }
  // Last resort: use identifier name (degrades to name-based, but should rarely happen)
  return `name:${binding.identifier.name}`;
}

/** Check if an identifier path is a non-reference site (property key, declaration, etc.). */
function isNonReferenceSite(p: babelTraverse.NodePath<t.Identifier>): boolean {
  // Property access keys (obj.prop)
  if (
    p.parentPath?.isMemberExpression({ property: p.node }) &&
    !p.parentPath.node.computed
  )
    return true;
  if (
    p.parentPath?.isObjectProperty({ key: p.node }) &&
    !p.parentPath.node.computed
  )
    return true;
  // Function name in function declaration
  if (p.parentPath?.isFunctionDeclaration({ id: p.node })) return true;
  return false;
}

/** Process a single identifier reference: resolve to top-level binding and record. */
function processIdentifierRef(
  p: babelTraverse.NodePath<t.Identifier>,
  targetNodes: Set<t.Node>,
  result: Map<t.Node, Set<string>>
): void {
  const ownerFn = findOwnerFunction(p, targetNodes);
  if (!ownerFn) return;

  const binding = p.scope.getBinding(p.node.name);
  if (!binding) return;
  if (!isTopLevelBinding(binding)) return;

  // Skip self-references (function referencing its own declaration)
  if (binding.path.node === ownerFn) return;

  const refSet = result.get(ownerFn);
  if (refSet) {
    refSet.add(formatBindingPosition(binding));
  }
}

/**
 * Collect all positional references for all top-level functions in one AST traversal.
 *
 * Traverses the entire file AST with scope analysis enabled.
 * For each identifier inside a top-level function that resolves to a top-level
 * binding (not a local variable or parameter), records the binding's declaration
 * position as "pos:LINE:COL".
 *
 * Returns a map from the function's AST node to its set of positional references.
 */
function collectAllPositionalReferences(
  ast: t.File,
  targetNodes: Set<t.Node>
): Map<t.Node, Set<string>> {
  const result = new Map<t.Node, Set<string>>();
  for (const node of targetNodes) {
    result.set(node, new Set());
  }

  traverse(ast, {
    Identifier(path: babelTraverse.NodePath<t.Identifier>) {
      if (path.isBindingIdentifier()) return;
      // Cast needed: after isBindingIdentifier() TypeScript narrows to `never`
      const p = path as babelTraverse.NodePath<t.Identifier>;
      if (isNonReferenceSite(p)) return;
      processIdentifierRef(p, targetNodes, result);
    }
  });

  return result;
}

/** Walk up the path ancestry to find the nearest target function node. */
function findOwnerFunction(
  path: babelTraverse.NodePath,
  targetNodes: Set<t.Node>
): t.Node | null {
  let current = path.parentPath;
  while (current) {
    if (targetNodes.has(current.node)) {
      return current.node;
    }
    current = current.parentPath;
  }
  return null;
}

/**
 * Collect positional references for a single function.
 *
 * This is the public API for testing. For bulk collection in referenceCluster,
 * use collectAllPositionalReferences instead (single traversal for all functions).
 */
export function collectPositionalReferences(
  fn: FunctionNode,
  ast: t.File
): Set<string> {
  const targetNodes = new Set<t.Node>([fn.path.node]);
  const resultMap = collectAllPositionalReferences(ast, targetNodes);
  return resultMap.get(fn.path.node) ?? new Set();
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
 * Compute density of the similarity graph among connected nodes.
 *
 * Excludes isolated (zero-edge) nodes from the calculation since they
 * dilute the density metric. A high density among connected nodes means
 * the reference signal is noisy (typical of minified code where short
 * variable names like e/t/r create false similarities).
 */
export function computeGraphDensity(
  graph: Map<string, SimilarityEdge[]>
): number {
  if (graph.size <= 1) return 0;

  let connectedNodes = 0;
  let totalEdges = 0;
  for (const edges of graph.values()) {
    if (edges.length > 0) {
      connectedNodes++;
      totalEdges += edges.length;
    }
  }

  if (connectedNodes <= 1) return 0;
  // Density among connected nodes only
  return totalEdges / connectedNodes / (connectedNodes - 1);
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

/** Group functions by the parsed file they belong to (by file path in sessionId). */
function groupFunctionsByFile(
  topLevel: FunctionNode[],
  parsedFiles: ParsedFile[]
): Map<t.File, FunctionNode[]> {
  const filePathToAst = new Map<string, t.File>();
  for (const pf of parsedFiles) {
    filePathToAst.set(pf.filePath, pf.ast);
  }

  const fnsByAst = new Map<t.File, FunctionNode[]>();
  for (const pf of parsedFiles) {
    fnsByAst.set(pf.ast, []);
  }

  for (const fn of topLevel) {
    const fnFile = fn.sessionId.split(":")[0];
    const ast = filePathToAst.get(fnFile);
    if (ast) {
      fnsByAst.get(ast)?.push(fn);
    }
  }
  return fnsByAst;
}

/**
 * Build reference sets for all top-level functions using positional references.
 *
 * Traverses each AST once with scope analysis, collecting position-based
 * references ("pos:LINE:COL") to top-level bindings. This approach is
 * immune to identifier renaming (minification) because declaration positions
 * are stable.
 */
function buildReferenceSets(
  topLevel: FunctionNode[],
  parsedFiles: ParsedFile[]
): Map<string, Set<string>> {
  const refSets = new Map<string, Set<string>>();
  const fnsByAst = groupFunctionsByFile(topLevel, parsedFiles);

  for (const [ast, fns] of fnsByAst) {
    if (fns.length === 0) continue;

    const targetNodes = new Set<t.Node>(fns.map((fn) => fn.path.node));
    const posRefs = collectAllPositionalReferences(ast, targetNodes);

    for (const fn of fns) {
      const refs = posRefs.get(fn.path.node) ?? new Set();
      refSets.set(fn.sessionId, refs);
    }
  }

  return refSets;
}

/**
 * Estimate the number of output files from function count and total lines.
 *
 * For minified bundles (few lines, many functions), uses function count
 * alone since line count is meaningless. Typical source files have 5-15
 * functions, so we use a smaller divisor than for normal bundles.
 *
 * For normal bundles, uses the geometric mean of function-based and
 * line-based estimates.
 */
export function estimateFileCount(
  totalFunctions: number,
  totalLines: number
): number {
  if (totalFunctions <= 3) return 1;

  // Detect minified bundles: many functions on very few lines
  const isMinified = totalLines < totalFunctions / 5;
  if (isMinified) {
    // Use sqrt(N) as a balanced estimate: produces reasonable file counts
    // across a wide range of function counts.
    // 100 functions -> 10 files, 400 -> 20, 1000 -> 32
    const estimate = Math.round(Math.sqrt(totalFunctions));
    return Math.max(2, Math.min(estimate, Math.floor(totalFunctions / 2)));
  }

  const byFunctions = Math.round(totalFunctions / 40);
  const byLines = Math.round(totalLines / 300);
  const estimate = Math.round(Math.sqrt(byFunctions * byLines));
  return Math.max(2, Math.min(estimate, Math.floor(totalFunctions / 2)));
}

// ── Gap-based clustering ──────────────────────────────────────────────

/**
 * Get source position for a function as a byte offset.
 * Uses the `start` byte offset when available, falling back to
 * line/column approximation. Byte offsets handle minified single-line
 * code where line numbers are useless.
 */
function getStartOffset(fn: FunctionNode): number {
  const node = fn.path.node;
  if (typeof node.start === "number") return node.start;
  const loc = node.loc?.start;
  if (loc) return loc.line * 10000 + loc.column;
  return 0;
}

/** Get the end byte offset for a function. */
function getEndOffset(fn: FunctionNode): number {
  const node = fn.path.node;
  if (typeof node.end === "number") return node.end;
  const loc = node.loc?.end;
  if (loc) return loc.line * 10000 + loc.column;
  return 0;
}

/** Get the body size of a function (end - start bytes). */
function getBodySize(fn: FunctionNode): number {
  return Math.max(0, getEndOffset(fn) - getStartOffset(fn));
}

/** Sort functions by source position (byte offset). */
function sortByPosition(topLevel: FunctionNode[]): FunctionNode[] {
  return [...topLevel].sort((a, b) => getStartOffset(a) - getStartOffset(b));
}

/** Compute byte-offset gaps between consecutive function ends and starts. */
function computePositionGaps(sorted: FunctionNode[]): number[] {
  const gaps: number[] = [];
  for (let i = 0; i < sorted.length - 1; i++) {
    const endI = getEndOffset(sorted[i]);
    const startNext = getStartOffset(sorted[i + 1]);
    gaps.push(Math.max(0, startNext - endI));
  }
  return gaps;
}

/** Compute body-size discontinuities between consecutive functions. */
function computeSizeDiscontinuities(sorted: FunctionNode[]): number[] {
  const discs: number[] = [];
  for (let i = 0; i < sorted.length - 1; i++) {
    const sizeI = getBodySize(sorted[i]);
    const sizeNext = getBodySize(sorted[i + 1]);
    discs.push(Math.abs(sizeI - sizeNext));
  }
  return discs;
}

/**
 * Detect potential bundle file boundaries between consecutive top-level
 * functions by computing a gap score for each adjacent pair.
 *
 * Gap score components:
 * - Position gap: byte distance between end of fn[i] and start of fn[i+1].
 *   Large gaps suggest file boundaries.
 * - Size discontinuity: absolute difference in function body sizes between
 *   adjacent functions, normalized by the max body size.
 *
 * Returns an array of gap scores (length = topLevel.length - 1).
 */
export function detectBundleGaps(topLevel: FunctionNode[]): number[] {
  if (topLevel.length < 2) return [];

  const sorted = sortByPosition(topLevel);

  const posGaps = computePositionGaps(sorted);
  const sizeDisc = computeSizeDiscontinuities(sorted);

  // Normalize both components to [0, 1] and combine
  const maxPosGap = Math.max(1, ...posGaps);
  const maxSizeDisc = Math.max(1, ...sizeDisc);

  const scores: number[] = [];
  for (let i = 0; i < sorted.length - 1; i++) {
    const normPos = posGaps[i] / maxPosGap;
    const normSize = sizeDisc[i] / maxSizeDisc;
    // Weight position gap more heavily -- it's the primary signal
    scores.push(normPos * 0.7 + normSize * 0.3);
  }

  return scores;
}

/**
 * Find the indices of the top-k largest gaps.
 * Returns a Set of indices where a new cluster starts (i.e., the index
 * of the first function AFTER each gap).
 */
function findTopGapIndices(gaps: number[], k: number): Set<number> {
  const actualK = Math.min(k, gaps.length);

  // Create (index, score) pairs and sort descending by score
  const indexed = gaps.map((score, i) => ({ i, score }));
  indexed.sort((a, b) => b.score - a.score || a.i - b.i);

  const splitIndices = new Set<number>();
  for (let j = 0; j < actualK; j++) {
    // The gap at index i is between sorted[i] and sorted[i+1],
    // so the new cluster starts at sorted[i+1] which has index i+1
    splitIndices.add(indexed[j].i + 1);
  }
  return splitIndices;
}

/**
 * Cluster functions by splitting at the largest detected gaps.
 *
 * Computes gap scores, finds the top (targetCount - 1) largest gaps as
 * file boundaries, and splits the sorted function list at those gaps.
 *
 * Returns a community assignment map (sessionId -> community index).
 */
export function gapBasedClustering(
  topLevel: FunctionNode[],
  targetCount: number
): Map<string, number> {
  const result = new Map<string, number>();
  if (topLevel.length === 0) return result;

  const sorted = sortByPosition(topLevel);

  if (targetCount <= 1 || sorted.length <= 1) {
    for (const fn of sorted) result.set(fn.sessionId, 0);
    return result;
  }

  const gaps = detectBundleGaps(sorted);
  const splitIndices = findTopGapIndices(gaps, targetCount - 1);

  // Assign community indices based on split points
  let community = 0;
  for (let i = 0; i < sorted.length; i++) {
    if (splitIndices.has(i)) community++;
    result.set(sorted[i].sessionId, community);
  }

  return result;
}

/** Build a map of community ID → position centroid accumulator. */
function buildCommunityCentroids(
  communityMap: Map<string, number>,
  fnBySessionId: Map<string, FunctionNode>
): Map<number, { sum: number; count: number }> {
  const positions = new Map<number, { sum: number; count: number }>();
  for (const [sessionId, comId] of communityMap) {
    const fn = fnBySessionId.get(sessionId);
    if (!fn) continue;
    const pos = getStartOffset(fn);
    const entry = positions.get(comId);
    if (entry) {
      entry.sum += pos;
      entry.count++;
    } else {
      positions.set(comId, { sum: pos, count: 1 });
    }
  }
  return positions;
}

/** Find the closest pair of centroids by position distance. Returns [mergeInto, mergeFrom]. */
function findClosestCentroidPair(
  positions: Map<number, { sum: number; count: number }>
): [number, number] {
  const centroids: { id: number; centroid: number }[] = [];
  for (const [id, entry] of positions) {
    centroids.push({ id, centroid: entry.sum / entry.count });
  }
  centroids.sort((a, b) => a.centroid - b.centroid);

  let bestDist = Infinity;
  let bestI = 0;
  for (let i = 0; i < centroids.length - 1; i++) {
    const dist = centroids[i + 1].centroid - centroids[i].centroid;
    if (dist < bestDist) {
      bestDist = dist;
      bestI = i;
    }
  }
  return [centroids[bestI].id, centroids[bestI + 1].id];
}

/**
 * Merge communities by source position proximity until the target count is reached.
 *
 * When the similarity graph has disconnected components, agglomerative
 * clustering cannot merge across them. This function fills the gap by
 * computing the centroid position of each community and repeatedly merging
 * the two nearest communities.
 */
function mergeCommunitiesByPosition(
  communityMap: Map<string, number>,
  fnBySessionId: Map<string, FunctionNode>,
  target: number
): void {
  const positions = buildCommunityCentroids(communityMap, fnBySessionId);

  while (positions.size > target) {
    const [mergeInto, mergeFrom] = findClosestCentroidPair(positions);

    for (const [sessionId, comId] of communityMap) {
      if (comId === mergeFrom) communityMap.set(sessionId, mergeInto);
    }

    const into = positions.get(mergeInto);
    const from = positions.get(mergeFrom);
    if (!into || !from) break;
    into.sum += from.sum;
    into.count += from.count;
    positions.delete(mergeFrom);
  }
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

/** Find the nearest cluster to an orphan by source position proximity. */
function findNearestClusterByPosition(
  orphanFn: FunctionNode,
  clusters: Cluster[],
  fnBySessionId: Map<string, FunctionNode>
): number {
  const orphanPos = getStartOffset(orphanFn);
  let bestIdx = 0;
  let bestDist = Infinity;

  for (let i = 0; i < clusters.length; i++) {
    let sum = 0;
    let count = 0;
    for (const id of clusters[i].members) {
      const fn = fnBySessionId.get(id);
      if (fn) {
        sum += getStartOffset(fn);
        count++;
      }
    }
    if (count === 0) continue;
    const centroid = sum / count;
    const dist = Math.abs(orphanPos - centroid);
    if (dist < bestDist) {
      bestDist = dist;
      bestIdx = i;
    }
  }
  return bestIdx;
}

/** Build the union of reference sets for each cluster. */
function buildClusterRefSets(
  clusters: Cluster[],
  refSets: Map<string, Set<string>>
): Set<string>[] {
  return clusters.map((cluster) => {
    const union = new Set<string>();
    for (const memberId of cluster.members) {
      const refs = refSets.get(memberId);
      if (refs) {
        for (const r of refs) union.add(r);
      }
    }
    return union;
  });
}

/** Find the largest cluster index. */
function findLargestCluster(clusters: Cluster[]): number {
  let largest = 0;
  for (let i = 1; i < clusters.length; i++) {
    if (clusters[i].members.size > clusters[largest].members.size) {
      largest = i;
    }
  }
  return largest;
}

/** Assign orphan to nearest cluster by position, with largest-cluster fallback. */
function assignOrphanByPosition(
  orphanId: string,
  clusters: Cluster[],
  fnBySessionId: Map<string, FunctionNode>
): void {
  const fn = fnBySessionId.get(orphanId);
  if (fn) {
    const best = findNearestClusterByPosition(fn, clusters, fnBySessionId);
    clusters[best].members.add(orphanId);
  } else {
    clusters[findLargestCluster(clusters)].members.add(orphanId);
  }
}

/** Compute the IDF-weighted overlap score between an orphan and a cluster. */
function computeOverlapScore(
  orphanRefs: Set<string>,
  clusterRefSet: Set<string>,
  idf: Map<string, number>
): number {
  let score = 0;
  for (const name of orphanRefs) {
    if (clusterRefSet.has(name)) {
      score += idf.get(name) ?? 0;
    }
  }
  return score;
}

/**
 * Assign orphan singletons to nearest cluster.
 * Uses IDF-weighted reference overlap when available;
 * falls back to source position proximity when overlap is zero.
 */
function assignOrphans(
  orphans: Set<string>,
  clusters: Cluster[],
  refSets: Map<string, Set<string>>,
  idf: Map<string, number>,
  fnBySessionId: Map<string, FunctionNode>
): void {
  if (clusters.length === 0 || orphans.size === 0) return;

  const clusterRefs = buildClusterRefSets(clusters, refSets);

  for (const orphanId of orphans) {
    const orphanRefs = refSets.get(orphanId);
    if (!orphanRefs || orphanRefs.size === 0) {
      assignOrphanByPosition(orphanId, clusters, fnBySessionId);
      continue;
    }

    const best = findBestClusterForOrphan(
      orphanRefs,
      clusters,
      clusterRefs,
      idf
    );
    const bestScore = computeOverlapScore(orphanRefs, clusterRefs[best], idf);

    if (bestScore > 0) {
      clusters[best].members.add(orphanId);
    } else {
      assignOrphanByPosition(orphanId, clusters, fnBySessionId);
    }
  }
}

/** Merge orphan functions into the nearest cluster by source position. */
function mergeOrphansByPosition(
  orphans: Set<string>,
  clusters: Cluster[],
  fnBySessionId: Map<string, FunctionNode>
): void {
  for (const orphanId of orphans) {
    const fn = fnBySessionId.get(orphanId);
    if (!fn) continue;
    const best = findNearestClusterByPosition(fn, clusters, fnBySessionId);
    clusters[best].members.add(orphanId);
  }
}

// ── Output helpers ────────────────────────────────────────────────────

/** Build function name map for cluster naming. */
function buildFunctionNames(topLevel: FunctionNode[]): Map<string, string> {
  const functionNames = new Map<string, string>();
  for (const fn of topLevel) {
    const node = fn.path.node;
    if ("id" in node && node.id && node.id.name) {
      functionNames.set(fn.sessionId, node.id.name);
    }
  }
  return functionNames;
}

/** Build output map from clusters: sessionId -> filename. */
function buildOutputMap(
  clusters: Cluster[],
  topLevel: FunctionNode[],
  functionNames: Map<string, string>
): Map<string, string> {
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

// ── Main entry point ──────────────────────────────────────────────────

/** Density threshold: above this, fall back to gap-based clustering. */
const DENSITY_THRESHOLD = 0.5;

/** Empty-ref threshold: if more than this fraction of functions have no positional refs,
 *  the reference signal is too weak for agglomerative clustering. */
const EMPTY_REF_THRESHOLD = 0.5;

/** Compute the fraction of functions with empty reference sets. */
function computeEmptyRefRatio(refSets: Map<string, Set<string>>): number {
  if (refSets.size === 0) return 1;
  let empty = 0;
  for (const refs of refSets.values()) {
    if (refs.size === 0) empty++;
  }
  return empty / refSets.size;
}

/**
 * Cluster functions by co-reference similarity.
 *
 * Uses positional references (declaration line:column of top-level bindings)
 * which are immune to identifier renaming/minification.
 *
 * Falls back to gap-based position clustering when:
 * - The similarity graph is too dense (>50% density), OR
 * - More than half the functions have no positional references (signal too weak)
 *
 * Returns Map<sessionId, outputFileName> -- same interface as other adapters.
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

  // Estimate target file count
  const totalLines = parsedFiles.reduce(
    (sum, pf) => sum + (pf.source?.split("\n").length ?? 0),
    0
  );
  const target =
    targetFileCount ?? estimateFileCount(topLevel.length, totalLines);

  // 1. Build positional reference vectors (immune to renaming)
  const refSets = buildReferenceSets(topLevel, parsedFiles);

  // 2. Compute IDF weights
  const idf = computeIdfWeights(refSets);

  // 3. Build similarity graph
  const graph = buildSimilarityGraph(refSets, idf);

  // 4. Check whether reference signal is reliable enough for agglomerative clustering
  const density = computeGraphDensity(graph);
  const emptyRefRatio = computeEmptyRefRatio(refSets);

  if (density > DENSITY_THRESHOLD || emptyRefRatio > EMPTY_REF_THRESHOLD) {
    // Reference signal is unreliable: either too dense (spurious overlap)
    // or too sparse (most functions have no positional refs). Use gap-based.
    return referenceClusterByGaps(topLevel, fnBySessionId, target);
  }

  // 5. Normal path: apply bundler boosts and use agglomerative clustering
  const source = parsedFiles.map((pf) => pf.source).join("\n");
  if (source.length > 0) {
    const boosts = extractBundlerSignals(source, topLevel);
    applyBundlerBoosts(graph, boosts);
  }

  const communityMap = detectCommunities(graph, target);
  const numCommunities = new Set(communityMap.values()).size;

  // When the similarity graph has disconnected components, agglomerative
  // clustering can't merge across them, leaving more communities than the
  // target. Use gap-based position merging to combine the excess.
  if (numCommunities > target) {
    mergeCommunitiesByPosition(communityMap, fnBySessionId, target);
  }

  const { clusters, orphans } = buildClustersFromCommunities(
    communityMap,
    fnBySessionId
  );

  assignOrphans(orphans, clusters, refSets, idf, fnBySessionId);

  const functionNames = buildFunctionNames(topLevel);
  return buildOutputMap(clusters, topLevel, functionNames);
}

/**
 * Cluster functions using gap-based position clustering.
 *
 * Used when the similarity graph is too dense (minified code where
 * short variable names create spurious reference overlap). In this case,
 * byte-offset gaps between functions are the strongest file boundary signal.
 */
function referenceClusterByGaps(
  topLevel: FunctionNode[],
  fnBySessionId: Map<string, FunctionNode>,
  target: number
): Map<string, string> {
  const communityMap = gapBasedClustering(topLevel, target);
  const { clusters, orphans } = buildClustersFromCommunities(
    communityMap,
    fnBySessionId
  );

  // For gap-based clustering, merge orphans into nearest cluster by position
  if (orphans.size > 0 && clusters.length > 0) {
    mergeOrphansByPosition(orphans, clusters, fnBySessionId);
  }

  const functionNames = buildFunctionNames(topLevel);
  return buildOutputMap(clusters, topLevel, functionNames);
}
