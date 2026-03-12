import { createHash } from "crypto";
import { extractStructuralFeatures } from "./structural-hash.js";
import type {
  CalleeShape,
  FunctionFingerprint,
  FunctionNode,
  StructuralFeatures
} from "./types.js";

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

/**
 * Builds a complete fingerprint for a function including callee information.
 * This computes all resolution levels for multi-resolution matching.
 */
export function buildFullFingerprint(
  fn: FunctionNode,
  _graph: Map<string, FunctionNode>
): FunctionFingerprint {
  // Start with the basic fingerprint (exactHash + features)
  const fingerprint: FunctionFingerprint = {
    exactHash: fn.fingerprint.exactHash,
    features: fn.fingerprint.features
  };

  // Resolution 1: Blurred callee shapes
  const calleeShapes = [...fn.internalCallees]
    .map((callee) => computeCalleeShapeFromNode(callee))
    .sort((a, b) =>
      serializeCalleeShape(a).localeCompare(serializeCalleeShape(b))
    );
  fingerprint.calleeShapes = calleeShapes;

  // Resolution 1: Blurred caller shapes (optional)
  const callerShapes = [...fn.callers]
    .map((caller) => computeCalleeShapeFromNode(caller))
    .sort((a, b) =>
      serializeCalleeShape(a).localeCompare(serializeCalleeShape(b))
    );
  fingerprint.callerShapes = callerShapes;

  // Resolution 2: Exact callee hashes
  const calleeHashes = [...fn.internalCallees]
    .map((callee) => callee.fingerprint.exactHash)
    .sort();
  fingerprint.calleeHashes = calleeHashes;

  // Resolution 2: Two-hop shapes (callees' callees)
  const twoHopShapesSet = new Set<string>();
  for (const callee of fn.internalCallees) {
    for (const calleeOfCallee of callee.internalCallees) {
      const shape = computeCalleeShapeFromNode(calleeOfCallee);
      twoHopShapesSet.add(serializeCalleeShape(shape));
    }
  }
  fingerprint.twoHopShapes = [...twoHopShapesSet].sort();

  return fingerprint;
}

/**
 * Computes a hash of the callee shapes for indexing.
 * This is used for Resolution 1 matching.
 */
export function hashCalleeShapes(shapes: CalleeShape[]): string {
  if (shapes.length === 0) return "empty";
  const serialized = shapes.map(serializeCalleeShape).sort().join("|");
  return createHash("sha256").update(serialized).digest("hex").slice(0, 8);
}

/**
 * Creates a Resolution 1 composite key for indexing.
 * Combines exactHash with blurred callee shapes.
 */
export function makeResolution1Key(fingerprint: FunctionFingerprint): string {
  const shapesHash = hashCalleeShapes(fingerprint.calleeShapes ?? []);
  return `${fingerprint.exactHash}:${shapesHash}`;
}

/**
 * Computes edge n-grams for a function.
 * These represent the call relationships as "caller → callee" pairs.
 *
 * @param fn The function to compute n-grams for
 * @param mode 'exact' uses callee's exactHash, 'blurred' uses serialized CalleeShape
 */
export function computeEdgeNgrams(
  fn: FunctionNode,
  mode: "exact" | "blurred"
): string[] {
  const myHash = fn.fingerprint.exactHash;

  return [...fn.internalCallees].map((callee) => {
    const calleeId =
      mode === "exact"
        ? callee.fingerprint.exactHash
        : serializeCalleeShape(computeCalleeShapeFromNode(callee));

    return `${myHash}→${calleeId}`;
  });
}

/**
 * Computes path n-grams (trigrams, 4-grams, etc.) for a function.
 * These represent call chains through the graph.
 *
 * @param fn The function to start from
 * @param depth Number of hops (2 = trigrams, 3 = 4-grams)
 */
export function computePathNgrams(fn: FunctionNode, depth: number): string[] {
  const paths: string[] = [];
  const myHash = fn.fingerprint.exactHash;

  function walk(
    current: FunctionNode,
    path: string[],
    remaining: number
  ): void {
    if (remaining === 0) {
      paths.push(path.join("→"));
      return;
    }

    for (const callee of current.internalCallees) {
      walk(callee, [...path, callee.fingerprint.exactHash], remaining - 1);
    }

    // Also emit partial paths if we hit a leaf
    if (current.internalCallees.size === 0 && path.length > 1) {
      paths.push(path.join("→"));
    }
  }

  walk(fn, [myHash], depth);
  return [...new Set(paths)]; // Deduplicate
}
