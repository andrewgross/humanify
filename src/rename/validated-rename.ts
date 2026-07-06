import type { Binding, NodePath, Scope } from "@babel/traverse";
import * as t from "@babel/types";
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
  /** The file observes the target name as a free (global) reference */
  | "target-free-name"
  /** A child scope binds the target name around a reference (shadow risk) */
  | "shadows-child";

export interface RenameAttempt {
  applied: boolean;
  reason?: RenameRejectionReason;
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
  scope: Scope,
  oldName: string,
  newName: string
): boolean {
  const binding = scope.bindings[oldName];
  if (!binding) return false;

  const allPaths = [...binding.referencePaths, ...binding.constantViolations];

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
  scope: Scope,
  oldName: string,
  newName: string
): RenameRejectionReason | null {
  if (!isValidRenameTarget(newName)) return "invalid-target";
  if (!scope.bindings[oldName]) return "no-binding";
  if (scope.bindings[newName]) return "target-in-scope";
  // parent is typed non-null but is undefined at runtime on the Program scope
  if (scope.parent?.hasBinding(newName)) return "target-visible";
  // Invariant: a rename may never bind a previously-free name. The file's
  // observed free names live on the Program scope (review C1 — renaming a
  // binding to `document` was applied and silently captured every
  // `document.*` read in scope).
  if (Object.hasOwn(scope.getProgramParent().globals, newName)) {
    return "target-free-name";
  }
  if (wouldRenameShadowInChildScope(scope, oldName, newName)) {
    return "shadows-child";
  }
  return null;
}

/** True when a binding participates in an export declaration/specifier. */
function isExportInvolved(binding: Binding): boolean {
  const exportParent = binding.path.find((p) => p.isExportDeclaration());
  if (exportParent) return true;
  for (const ref of binding.referencePaths) {
    if (ref.parentPath?.isExportSpecifier()) return true;
  }
  return false;
}

/**
 * Renames a binding by rewriting its tracked references directly.
 * Babel's scope.rename() re-traverses scope.block — for module-level
 * scopes that is the ENTIRE bundle, measured at ~1.7s per rename on a
 * 20MB Bun runtime, vs ~0.005ms this way. Safe because Babel's
 * binding.referencePaths already excludes shadowed references.
 *
 * Returns false when the binding needs Babel's renamer instead (export
 * declarations/specifiers, where the external name must be preserved).
 */
export function fastRenameBinding(
  scope: Scope,
  oldName: string,
  newName: string
): boolean {
  // Defense against callers that skip getRenameRejection: binding a
  // reserved word or builtin silently corrupts runtime behavior (a rename
  // to `document` captures every document.* read in scope).
  if (!isValidRenameTarget(newName)) {
    throw new Error(
      `invalid rename target "${newName}" reached fastRenameBinding — callers must validate first`
    );
  }
  const binding = scope.bindings[oldName];
  if (!binding) return false;
  if (isExportInvolved(binding)) return false;

  // Declaration identifier
  binding.identifier.name = newName;

  // Reads (Babel tracks only references resolving to THIS binding)
  for (const refPath of binding.referencePaths) {
    if (refPath.isIdentifier()) {
      refPath.node.name = newName;
    }
  }

  // Writes: simple assignment LHS and destructuring targets live in
  // constantViolations, not referencePaths.
  renameConstantViolationPatterns(binding, oldName, newName);

  scope.bindings[newName] = binding;
  delete scope.bindings[oldName];
  return true;
}

/**
 * Validates and applies a rename on a Babel scope. Returns whether the
 * rename was applied, and the rejection reason when it was not. Callers
 * decide what a rejection means (skip the transfer, fall back to the LLM,
 * pick a conflict-free variant, ...).
 */
export function attemptValidatedRename(
  scope: Scope,
  oldName: string,
  newName: string
): RenameAttempt {
  const reason = getRenameRejection(scope, oldName, newName);
  if (reason) return { applied: false, reason };
  if (!fastRenameBinding(scope, oldName, newName)) {
    scope.rename(oldName, newName);
  }
  // Post-rename spot check: the binding must now live under the new name
  // and nothing under the old. Catches binding-split bugs at the rename
  // site, minutes before the output parse gate would (or would not).
  if (!scope.bindings[newName] || scope.bindings[oldName]) {
    throw new Error(
      `rename ${oldName}→${newName} left scope bindings inconsistent ` +
        `(new present: ${Boolean(scope.bindings[newName])}, old present: ${Boolean(scope.bindings[oldName])})`
    );
  }
  return { applied: true };
}

// ---------------------------------------------------------------------------
// Constant-violation (write/destructuring) renaming
// ---------------------------------------------------------------------------

/** Rename destructuring patterns in constant violations (assignments, for-in/of). */
export function renameConstantViolationPatterns(
  binding: {
    constantViolations: NodePath[];
  },
  oldName: string,
  newName: string
): void {
  for (const vPath of binding.constantViolations) {
    const lhs = getConstantViolationLHS(vPath);
    if (!lhs) continue;
    if (t.isObjectPattern(lhs) || t.isArrayPattern(lhs)) {
      renameInDestructuringPattern(lhs, oldName, newName);
    } else if (t.isIdentifier(lhs) && lhs.name === oldName) {
      lhs.name = newName;
    }
  }
}

/**
 * Extract the write target from a constant violation path. Babel records
 * assignments, for-in/of loop heads, AND duplicate declarations
 * (`var a = 1; ... var a = 2;`, a second `function a() {}`) as constant
 * violations — duplicate declaration ids are not referencePaths, so
 * missing them here leaves the second declaration under the old name and
 * silently splits the binding.
 */
function getConstantViolationLHS(vPath: NodePath): t.Node | null {
  if (vPath.isAssignmentExpression()) return vPath.node.left;
  if (vPath.isForInStatement() || vPath.isForOfStatement())
    return vPath.node.left;
  if (vPath.isVariableDeclarator()) return vPath.node.id;
  if (vPath.isFunctionDeclaration() || vPath.isClassDeclaration())
    return vPath.node.id ?? null;
  return null;
}

/**
 * Renames an identifier inside a destructuring assignment pattern.
 * Handles ObjectPattern (`{ prop: target }`) and ArrayPattern (`[target]`)
 * where the target may be an identifier, a nested pattern, a rest element,
 * or an assignment pattern (default value).
 */
function renameInDestructuringPattern(
  pattern: t.ObjectPattern | t.ArrayPattern,
  oldName: string,
  newName: string
): void {
  if (t.isObjectPattern(pattern)) {
    renameInObjectPattern(pattern, oldName, newName);
  } else {
    renameInArrayPattern(pattern, oldName, newName);
  }
}

function renameInObjectPattern(
  pattern: t.ObjectPattern,
  oldName: string,
  newName: string
): void {
  for (const prop of pattern.properties) {
    if (t.isRestElement(prop)) {
      renamePatternTarget(prop.argument, oldName, newName);
    } else if (t.isObjectProperty(prop)) {
      renamePatternTarget(prop.value as t.PatternLike, oldName, newName);
    }
  }
}

function renameInArrayPattern(
  pattern: t.ArrayPattern,
  oldName: string,
  newName: string
): void {
  for (const element of pattern.elements) {
    if (!element) continue;
    if (t.isRestElement(element)) {
      renamePatternTarget(element.argument, oldName, newName);
    } else {
      renamePatternTarget(element, oldName, newName);
    }
  }
}

function renamePatternTarget(
  node: t.PatternLike | t.LVal,
  oldName: string,
  newName: string
): void {
  if (t.isIdentifier(node) && node.name === oldName) {
    node.name = newName;
  } else if (t.isAssignmentPattern(node)) {
    renamePatternTarget(node.left, oldName, newName);
  } else if (t.isObjectPattern(node) || t.isArrayPattern(node)) {
    renameInDestructuringPattern(node, oldName, newName);
  }
}
