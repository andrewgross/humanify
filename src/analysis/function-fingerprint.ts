import * as t from "@babel/types";
import { extractStructuralFeatures } from "./structural-hash.js";
import type {
  CalleeShape,
  FunctionFingerprint,
  FunctionNode,
  ModuleBindingNode,
  StructuralFeatures
} from "./types.js";

/** Discriminates FunctionNode from ModuleBindingNode in union sets. */
export function isFunctionNode(
  node: FunctionNode | ModuleBindingNode
): node is FunctionNode {
  return "path" in node;
}

/**
 * Computes a CalleeShape from structural features.
 * This is a "blurred" representation that describes the callee's structure
 * without identifying it exactly, preventing cascade effects.
 */
export function computeCalleeShape(features: StructuralFeatures): CalleeShape {
  return {
    arity: features.arity,
    complexity: features.complexity,
    cfgType: classifyCfgType(features),
    hasExternalCalls: features.externalCalls.length > 0
  };
}

/**
 * Computes a CalleeShape directly from a FunctionNode.
 */
function computeCalleeShapeFromNode(fn: FunctionNode): CalleeShape {
  const features =
    fn.fingerprint.features ?? extractStructuralFeatures(fn.path.node);
  return computeCalleeShape(features);
}

/**
 * Classifies the control flow type based on structural features.
 */
function classifyCfgType(
  features: StructuralFeatures
): "linear" | "branching" | "looping" | "complex" {
  if (features.loopCount > 0 && features.branchCount > 0) {
    return "complex";
  }
  if (features.loopCount > 0) {
    return "looping";
  }
  if (features.branchCount > 0) {
    return "branching";
  }
  return "linear";
}

/**
 * Serializes a CalleeShape to a deterministic string for hashing/comparison.
 */
export function serializeCalleeShape(shape: CalleeShape): string {
  return `(${shape.arity},${shape.complexity},${shape.cfgType},${shape.hasExternalCalls})`;
}

/**
 * Compares two arrays of CalleeShapes for equality.
 */
export function calleeShapesEqual(a: CalleeShape[], b: CalleeShape[]): boolean {
  if (a.length !== b.length) return false;
  const aStrs = a.map(serializeCalleeShape).sort();
  const bStrs = b.map(serializeCalleeShape).sort();
  return aStrs.every((str, i) => str === bStrs[i]);
}

/** Extract key name from an Identifier or StringLiteral node. */
function keyName(
  key: t.Node,
  computed: boolean | undefined
): string | undefined {
  if (t.isIdentifier(key) && !computed) return key.name;
  if (t.isStringLiteral(key)) return key.value;
  return undefined;
}

/**
 * Extracts the property key that a function is assigned to.
 * Covers ObjectProperty value, ObjectMethod/ClassMethod/ClassProperty,
 * and AssignmentExpression with MemberExpression LHS.
 */
export function extractMemberKey(fn: FunctionNode): string | undefined {
  const parent = fn.path.parent;
  const node = fn.path.node;

  // ObjectProperty: { getCount: function(){...} }
  if (t.isObjectProperty(parent) && parent.value === node) {
    return keyName(parent.key, parent.computed);
  }

  // ObjectMethod / ClassMethod / ClassProperty: the function IS the method
  if (
    t.isObjectMethod(node) ||
    t.isClassMethod(node) ||
    t.isClassProperty(node)
  ) {
    return keyName(node.key, node.computed);
  }

  // AssignmentExpression: obj.foo = function(){...}
  if (t.isAssignmentExpression(parent) && parent.right === node) {
    const lhs = parent.left;
    if (
      t.isMemberExpression(lhs) &&
      !lhs.computed &&
      t.isIdentifier(lhs.property)
    ) {
      return lhs.property.name;
    }
  }

  return undefined;
}

export interface FingerprintOptions {
  /** SessionIds to exclude from shape computation (e.g., Bun CJS wrapper) */
  excludeFromShapes?: Set<string>;
}

/**
 * Builds a complete fingerprint for a function including callee information.
 * This computes all resolution levels for multi-resolution matching.
 */
export function buildFullFingerprint(
  fn: FunctionNode,
  _graph: Map<string, FunctionNode>,
  options?: FingerprintOptions
): FunctionFingerprint {
  // Start with the basic fingerprint (structuralHash + features)
  const fingerprint: FunctionFingerprint = {
    structuralHash: fn.fingerprint.structuralHash,
    features: fn.fingerprint.features,
    memberKey: extractMemberKey(fn)
  };

  const exclude = options?.excludeFromShapes;

  // calleeShapes: Blurred callee structural shapes
  const callees = exclude
    ? [...fn.internalCallees].filter((c) => !exclude.has(c.sessionId))
    : [...fn.internalCallees];
  const calleeShapes = callees
    .map((callee) => computeCalleeShapeFromNode(callee))
    .sort((a, b) =>
      serializeCalleeShape(a).localeCompare(serializeCalleeShape(b))
    );
  fingerprint.calleeShapes = calleeShapes;

  // callerShapes: Blurred caller structural shapes (optional)
  const callers = exclude
    ? [...fn.callers].filter((c) => !exclude.has(c.sessionId))
    : [...fn.callers];
  const callerShapes = callers
    .map((caller) => computeCalleeShapeFromNode(caller))
    .sort((a, b) =>
      serializeCalleeShape(a).localeCompare(serializeCalleeShape(b))
    );
  fingerprint.callerShapes = callerShapes;

  // calleeHashes: Exact callee hash values
  const calleeHashes = callees
    .map((callee) => callee.fingerprint.structuralHash)
    .sort();
  fingerprint.calleeHashes = calleeHashes;

  // twoHopShapes: Blurred shapes of callees' callees
  const twoHopShapesSet = new Set<string>();
  for (const callee of callees) {
    for (const calleeOfCallee of callee.internalCallees) {
      if (exclude?.has(calleeOfCallee.sessionId)) continue;
      const shape = computeCalleeShapeFromNode(calleeOfCallee);
      twoHopShapesSet.add(serializeCalleeShape(shape));
    }
  }
  fingerprint.twoHopShapes = [...twoHopShapesSet].sort();

  return fingerprint;
}

/**
 * Builds a match-time fingerprint for a module binding, mirroring
 * buildFullFingerprint for functions. The binding's cheap fingerprint
 * (structuralHash) is computed at graph-build time, before callee/caller
 * edges exist; this fills in the relational fields once wiring is done.
 *
 * Referenced functions contribute shapes and hashes exactly like function
 * callees do. Referenced bindings contribute their init structural hash to
 * calleeHashes (shape concepts like arity do not apply to them).
 */
export function buildBindingFullFingerprint(
  binding: ModuleBindingNode
): FunctionFingerprint {
  const baseFingerprint = binding.fingerprint;
  if (!baseFingerprint) {
    throw new Error(
      `buildBindingFullFingerprint requires a hashable binding (${binding.sessionId})`
    );
  }
  const callees = [...binding.internalCallees];
  const fnCallees = callees.filter(isFunctionNode);

  const calleeShapes = fnCallees
    .map((callee) => computeCalleeShapeFromNode(callee))
    .sort((a, b) =>
      serializeCalleeShape(a).localeCompare(serializeCalleeShape(b))
    );

  const callerShapes = [...binding.callers]
    .map((caller) => computeCalleeShapeFromNode(caller))
    .sort((a, b) =>
      serializeCalleeShape(a).localeCompare(serializeCalleeShape(b))
    );

  // Null-fingerprint binding callees are excluded: the old name-derived
  // fallback hash made the parent's fingerprint rename-VARIANT.
  const calleeHashes = callees
    .map((callee) => callee.fingerprint?.structuralHash)
    .filter((h): h is string => h !== undefined)
    .sort();

  const twoHopShapesSet = new Set<string>();
  for (const callee of fnCallees) {
    for (const calleeOfCallee of callee.internalCallees) {
      const shape = computeCalleeShapeFromNode(calleeOfCallee);
      twoHopShapesSet.add(serializeCalleeShape(shape));
    }
  }

  return {
    structuralHash: baseFingerprint.structuralHash,
    calleeShapes,
    callerShapes,
    calleeHashes,
    twoHopShapes: [...twoHopShapesSet].sort()
  };
}

/**
 * Computes edge n-grams for a function.
 * These represent the call relationships as "caller → callee" pairs.
 *
 * @param fn The function to compute n-grams for
 * @param mode 'exact' uses callee's structuralHash, 'blurred' uses serialized CalleeShape
 */
export function computeEdgeNgrams(
  fn: FunctionNode,
  mode: "exact" | "blurred"
): string[] {
  const myHash = fn.fingerprint.structuralHash;

  return [...fn.internalCallees].map((callee) => {
    const calleeId =
      mode === "exact"
        ? callee.fingerprint.structuralHash
        : serializeCalleeShape(computeCalleeShapeFromNode(callee));

    return `${myHash}→${calleeId}`;
  });
}

/**
 * Memoized shingle sets. Match-time inputs (internalCallees, features) are
 * stable, and the shingle tiebreaker recomputes the same candidates for
 * every ambiguous function sharing a hash bucket — O(bucket²) without this.
 */
const shingleSetCache = new WeakMap<FunctionNode, Set<string>>();

/**
 * Computes a shingle set for a function, combining edge n-grams with
 * structural feature tokens. Used for Jaccard similarity tiebreaking.
 */
export function computeShingleSet(fn: FunctionNode): Set<string> {
  const cached = shingleSetCache.get(fn);
  if (cached) return cached;

  const shingles = new Set<string>();

  // Blurred edge n-grams (call relationship pairs)
  for (const ngram of computeEdgeNgrams(fn, "blurred")) {
    shingles.add(ngram);
  }

  // Feature tokens from structural analysis
  const f = fn.fingerprint.features;
  if (f) {
    for (const ext of f.externalCalls) shingles.add(`ext:${ext}`);
    for (const prop of f.propertyAccesses) shingles.add(`prop:${prop}`);
    for (const str of f.stringLiterals) shingles.add(`str:${str}`);
  }

  shingleSetCache.set(fn, shingles);
  return shingles;
}

/**
 * Computes Jaccard similarity between two sets: |A ∩ B| / |A ∪ B|.
 */
export function jaccardSimilarity(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 && b.size === 0) return 1;

  let intersection = 0;
  for (const item of a) {
    if (b.has(item)) intersection++;
  }

  const union = a.size + b.size - intersection;
  return union > 0 ? intersection / union : 0;
}
