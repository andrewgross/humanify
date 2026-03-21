/**
 * Rename eligibility — opt-out skip-list replacing the old looksMinified heuristic.
 *
 * Everything in scope.bindings is eligible for rename UNLESS it matches
 * a skip-set entry or a pattern-based skip rule. This inverted approach
 * ensures short names like `get`, `set`, `map` are properly renamed.
 */

import type { BundlerType, MinifierType } from "../detection/types.js";
import { createSkipSet } from "./skip-list.js";

/** Function signature for rename-eligibility detection */
export type IsEligibleFn = (name: string) => boolean;

/** Double-underscore prefix → bundler/runtime helper */
const DOUBLE_UNDERSCORE_RE = /^__[a-z]/;

/** SWC helper pattern: _word_word (at least two underscore-separated segments) */
const SWC_HELPER_RE = /^_[a-z]+(_[a-z]+)+$/;

/**
 * Creates a rename-eligibility function.
 *
 * Returns true if the identifier is eligible for renaming,
 * false if it should be preserved (bundler runtime, module system, etc.).
 */
export function createIsEligible(
  bundlerType?: BundlerType,
  minifierType?: MinifierType
): IsEligibleFn {
  const skipSet = createSkipSet(bundlerType, minifierType);

  return (name: string): boolean => {
    if (name.length === 0) return false;

    // Layer 1: hard skip-set
    if (skipSet.has(name)) return false;

    // Layer 2: pattern-based skip rules
    if (DOUBLE_UNDERSCORE_RE.test(name)) return false;
    if (SWC_HELPER_RE.test(name)) return false;

    // Default: eligible for rename
    return true;
  };
}
