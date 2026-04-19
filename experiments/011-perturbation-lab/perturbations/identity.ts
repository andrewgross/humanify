import type { Perturbation } from "../types.js";

/**
 * Identity perturbation: v2 === v1. With deterministic minifiers this
 * produces byte-identical minified output, so matching trivially hits 100%.
 * Serves as a "does the harness actually run" sanity check.
 */
export const identity: Perturbation = {
  id: "identity",
  description: "v2 = v1 (no change)",
  apply(source) {
    return {
      source,
      directlyModified: [],
      added: [],
      removed: [],
      description: "identity"
    };
  }
};
