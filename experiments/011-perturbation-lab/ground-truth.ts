import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../../src/babel-utils.js";
import { buildFunctionGraph } from "../../src/analysis/function-graph.js";
import { computeStructuralHash } from "../../src/analysis/structural-hash.js";
import type {
  FingerprintIndex,
  FunctionNode
} from "../../src/analysis/types.js";
import type { MinifiedGroundTruth, SourceGroundTruth } from "./types.js";

/**
 * Compare two source strings and compute expected match counts based on
 * source-level structural hashes.
 *
 * Counts v1 functions whose source-level hash also appears in v2 as
 * "expected to match". The rest are expected to be unmatched (modified or
 * removed). New v2-only functions are counted as "added".
 */
export function computeSourceGroundTruth(
  v1Source: string,
  v2Source: string,
  virtualFilename = "input.js"
): SourceGroundTruth {
  const v1Hashes = hashFunctions(v1Source, virtualFilename);
  const v2Hashes = hashFunctions(v2Source, virtualFilename);

  const v1Multiset = toMultiset(v1Hashes.map((e) => e.hash));
  const v2Multiset = toMultiset(v2Hashes.map((e) => e.hash));

  let expectedMatches = 0;
  let expectedUnmatched = 0;
  const matchableNames: string[] = [];

  for (const entry of v1Hashes) {
    const v2Count = v2Multiset.get(entry.hash) ?? 0;
    if (v2Count > 0) {
      expectedMatches++;
      matchableNames.push(entry.name);
      v2Multiset.set(entry.hash, v2Count - 1);
    } else {
      expectedUnmatched++;
    }
  }

  let expectedAdded = 0;
  for (const entry of v2Hashes) {
    const remaining = v2Multiset.get(entry.hash) ?? 0;
    if (remaining > 0) {
      expectedAdded++;
      v2Multiset.set(entry.hash, remaining - 1);
    }
  }

  return {
    v1FunctionCount: v1Hashes.length,
    v2FunctionCount: v2Hashes.length,
    expectedMatches,
    expectedUnmatched,
    expectedAdded,
    expectedRemoved: 0,
    matchableV1Names: matchableNames
  };
}

function hashFunctions(
  source: string,
  filename: string
): Array<{ name: string; hash: string }> {
  const ast = parseSync(source, { filename, sourceType: "module" });
  if (!ast) throw new Error(`Failed to parse source for ${filename}`);

  const fns = buildFunctionGraph(ast, filename);
  return fns.map((fn) => ({
    name: extractName(fn),
    hash: computeStructuralHash(fn.path)
  }));
}

function extractName(fn: FunctionNode): string {
  const node = fn.path.node;
  if (
    (t.isFunctionDeclaration(node) || t.isFunctionExpression(node)) &&
    node.id
  ) {
    return node.id.name;
  }
  const parent = fn.path.parent;
  if (t.isVariableDeclarator(parent) && t.isIdentifier(parent.id)) {
    return parent.id.name;
  }
  if (t.isObjectProperty(parent) && t.isIdentifier(parent.key)) {
    return parent.key.name;
  }
  return "<anonymous>";
}

function toMultiset<T>(items: T[]): Map<T, number> {
  const m = new Map<T, number>();
  for (const item of items) {
    m.set(item, (m.get(item) ?? 0) + 1);
  }
  return m;
}

/**
 * Build a property-key → sessionId map from minified code.
 *
 * For each ObjectExpression property:
 * - If value is a FunctionExpression/ArrowFunctionExpression → direct map
 * - If value is an Identifier → resolve the binding to find the function
 */
export function buildMinifiedIdentityMap(
  code: string,
  filePath: string,
  index: FingerprintIndex
): Map<string, string> {
  const ast = parseSync(code, { filename: filePath, sourceType: "module" });
  if (!ast) throw new Error(`Failed to parse ${filePath}`);

  // Build a mapping from AST start position → sessionId
  const posToSessionId = new Map<number, string>();
  if (index.functions) {
    for (const [sessionId, fn] of index.functions) {
      const start = fn.path.node.start;
      if (start != null) posToSessionId.set(start, sessionId);
    }
  }

  const keyMap = new Map<string, string>();

  traverse(ast, {
    ObjectProperty(path: NodePath<t.ObjectProperty>) {
      const key = extractPropertyKeyName(path.node);
      if (!key) return;

      const value = path.node.value;

      // Direct function: { getCount: function(){...} }
      if (t.isFunctionExpression(value) || t.isArrowFunctionExpression(value)) {
        const sid =
          value.start != null ? posToSessionId.get(value.start) : undefined;
        if (sid) keyMap.set(key, sid);
        return;
      }

      // Identifier reference: { getCount: o }
      if (t.isIdentifier(value)) {
        const binding = path.scope.getBinding(value.name);
        if (!binding) return;
        const initNode = resolveBindingToFunction(binding);
        if (initNode?.start != null) {
          const sid = posToSessionId.get(initNode.start);
          if (sid) keyMap.set(key, sid);
        }
      }
    }
  });

  return keyMap;
}

function extractPropertyKeyName(node: t.ObjectProperty): string | undefined {
  if (t.isIdentifier(node.key) && !node.computed) return node.key.name;
  if (t.isStringLiteral(node.key)) return node.key.value;
  return undefined;
}

function resolveBindingToFunction(
  binding: ReturnType<typeof Object.create>
): t.Node | null {
  const declPath = binding.path;
  if (declPath.isVariableDeclarator()) {
    const init = declPath.node.init;
    if (t.isFunctionExpression(init) || t.isArrowFunctionExpression(init)) {
      return init;
    }
  }
  if (declPath.isFunctionDeclaration()) {
    return declPath.node;
  }
  return null;
}

/**
 * Build per-function ground truth by comparing identity maps from v1 and v2.
 */
export function buildMinifiedGroundTruth(
  v1Code: string,
  v1Path: string,
  v1Index: FingerprintIndex,
  v2Code: string,
  v2Path: string,
  v2Index: FingerprintIndex
): MinifiedGroundTruth {
  const v1Map = buildMinifiedIdentityMap(v1Code, v1Path, v1Index);
  const v2Map = buildMinifiedIdentityMap(v2Code, v2Path, v2Index);

  const pairs: MinifiedGroundTruth["pairs"] = [];
  const v1Only: string[] = [];
  const v2Only: string[] = [];

  const v2Keys = new Set(v2Map.keys());

  for (const [key, v1Sid] of v1Map) {
    const v2Sid = v2Map.get(key);
    if (v2Sid) {
      pairs.push({ propertyKey: key, v1SessionId: v1Sid, v2SessionId: v2Sid });
      v2Keys.delete(key);
    } else {
      v1Only.push(v1Sid);
    }
  }

  for (const key of v2Keys) {
    const v2Sid = v2Map.get(key);
    if (v2Sid) v2Only.push(v2Sid);
  }

  return { pairs, v1Only, v2Only };
}
