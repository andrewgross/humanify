import type { NodePath } from "@babel/traverse";
import {
  GLOBAL_BUILTINS,
  isValidIdentifier,
  RESERVED_WORDS
} from "../llm/validation.js";

/**
 * Shared name-application safety checks.
 *
 * Every path that renames a binding — LLM suggestions, prior-version
 * transfers, vote propagation, closure captures — must go through the same
 * validation. Run B of exp013 produced unparseable output because the
 * transfer paths applied names without these checks (duplicate `let NH`,
 * reserved word `delete` as a parameter name).
 */

/** Why a rename was rejected. */
export type RenameRejectionReason =
  /** Not a legal identifier, a reserved word, or a global builtin */
  | "invalid-target"
  /** The old name is not bound in the given scope */
  | "no-binding"
  /** The target name is already bound in the given scope */
  | "target-in-scope"
  /** The target name is visible from an ancestor scope (capture risk) */
  | "target-visible"
  /** A child scope binds the target name around a reference (shadow risk) */
  | "shadows-child";

export interface RenameAttempt {
  applied: boolean;
  reason?: RenameRejectionReason;
}

/** Minimal structural scope shape accepted by the checks below. */
interface ScopeLike {
  bindings: Record<
    string,
    { referencePaths: NodePath[]; constantViolations?: NodePath[] }
  >;
  parent?: ScopeLike | null;
  hasBinding?: (name: string) => boolean;
  rename: (oldName: string, newName: string) => void;
}

/**
 * A name is a valid rename target when it is a syntactically legal
 * identifier, not a reserved word, and not a global builtin.
 */
export function isValidRenameTarget(name: string): boolean {
  return (
    isValidIdentifier(name) &&
    !RESERVED_WORDS.has(name) &&
    !GLOBAL_BUILTINS.has(name)
  );
}

/**
 * Returns true when renaming `oldName` to `newName` in `scope` would cause a
 * reference to resolve to a different (child-scope) binding. Checks both
 * reads (referencePaths) and writes (constantViolations) — Babel tracks
 * `x |= val` as a constantViolation, not a referencePath.
 */
export function wouldRenameShadowInChildScope(
  scope: {
    bindings: Record<
      string,
      { referencePaths: NodePath[]; constantViolations?: NodePath[] }
    >;
  },
  oldName: string,
  newName: string
): boolean {
  const binding = scope.bindings[oldName];
  if (!binding) return false;

  const allPaths = binding.constantViolations
    ? [...binding.referencePaths, ...binding.constantViolations]
    : binding.referencePaths;

  for (const refPath of allPaths) {
    let refScope = refPath.scope;
    while (refScope && refScope !== scope) {
      if (refScope.bindings[newName]) return true;
      refScope = refScope.parent;
    }
  }
  return false;
}

/**
 * Returns the reason a rename must not be applied, or null when it is safe.
 */
export function getRenameRejection(
  scope: ScopeLike,
  oldName: string,
  newName: string
): RenameRejectionReason | null {
  if (!isValidRenameTarget(newName)) return "invalid-target";
  if (!scope.bindings[oldName]) return "no-binding";
  if (scope.bindings[newName]) return "target-in-scope";
  if (scope.parent?.hasBinding?.(newName)) return "target-visible";
  if (wouldRenameShadowInChildScope(scope, oldName, newName)) {
    return "shadows-child";
  }
  return null;
}

/**
 * Validates and applies a rename on a Babel scope. Returns whether the
 * rename was applied, and the rejection reason when it was not. Callers
 * decide what a rejection means (skip the transfer, fall back to the LLM,
 * pick a conflict-free variant, ...).
 */
export function attemptValidatedRename(
  scope: ScopeLike,
  oldName: string,
  newName: string
): RenameAttempt {
  const reason = getRenameRejection(scope, oldName, newName);
  if (reason) return { applied: false, reason };
  scope.rename(oldName, newName);
  return { applied: true };
}
