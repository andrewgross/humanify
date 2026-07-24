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

/**
 * WORD-LIKE double-underscore prefix → bundler/runtime helper or library
 * API (`__esm`, `__commonJS`, `__createBinding`, `__exportStar`). The
 * `__` + word shape is what real helpers use; enumerated helpers are
 * also caught by the hard skip-set above.
 *
 * SHORT dunder bindings (`__c`, `__t`, `__ab`) are NOT reserved: those
 * are minifier-minted app bindings (measured on 216: 22 such bindings,
 * 0 real helpers of this shape — Bun minifies its own helpers to single
 * letters like `Q`/`b`, never `__`-prefixed). Reserving them was a
 * shape heuristic that turned every one into a guaranteed minted
 * leftover and blocked corroborated prior-name transfers (exp036 idea
 * 6). Provenance, not shape: real helpers are word-like (>=3 chars after
 * `__`); the boot gate is the backstop if the oracle ever misses one.
 */
const DOUBLE_UNDERSCORE_RE = /^__[a-z][A-Za-z0-9$]{2,}/;

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
