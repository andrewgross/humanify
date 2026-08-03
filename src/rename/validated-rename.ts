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
  /** An ancestor-scope binding of the target name is referenced inside
   * this scope — the rename would capture those references */
  | "target-visible"
  /** A reference to the intended shadow owner sits inside the expression
   * subtree, so the rename would re-capture it (intentional-shadow only) */
  | "capture-in-subtree"
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
 * Names bound by an APPLIED rename, keyed by the scope's BLOCK NODE.
 *
 * Every guard below asks "is this name already bound here?" by reading a
 * `bindings` map off a Scope OBJECT. That is a per-scope-tree cache, and the
 * pipeline runs TWO trees over one AST: the unified graph captures scopes at
 * build time, then `clearBabelCacheAfterPriorMatch` makes the naming pass
 * re-crawl into fresh Scope objects for the same lexical scopes. A rename
 * applied through one tree re-keys only that tree's map (`fastRenameBinding`
 * patches exactly the map on the scope it was given), so a guard walking the
 * other tree is never told — it approves a second rename to the same name and
 * ships `dirPath !== dirPath`. That is exp059; `scope-era.test.ts` reproduces
 * it in ~30ms.
 *
 * Block NODES are the era-independent part: both trees' scopes wrap the same
 * nodes, so a WeakMap keyed by block is one ledger both eras read and write.
 * Keying by node is the same trick the hash tier already relies on across this
 * clear ("slot placeholders key by declaration node, which survives the scope
 * re-crawls this clear induces") — that comment was right about hashing and
 * silent about the guards.
 *
 * Consultation is strictly ADDITIVE: a claim exists only where a rename really
 * did bind that name, so this can turn an approval into a rejection but never
 * the reverse. It deliberately does NOT try to correct the opposite staleness
 * (the other era's map still keyed under a now-dead old name), because that
 * would LOOSEN a guard, and a loose guard is what this bug is.
 *
 * WeakMap keyed by AST node needs no lifecycle: entries die with the tree.
 */
const renameClaims = new WeakMap<t.Node, Map<string, Binding>>();

/** Record that `newName` is now bound in `scope`, whatever era wrote it. */
function recordRenameClaim(
  scope: Scope,
  oldName: string,
  newName: string
): void {
  const binding = scope.bindings[newName];
  if (!binding) return;
  let claims = renameClaims.get(scope.block);
  if (!claims) {
    claims = new Map();
    renameClaims.set(scope.block, claims);
  }
  claims.set(newName, binding);
  // The old name is genuinely free again — leaving it claimed would refuse
  // names no binding holds, and refusing a name only moves the collision
  // (measurement-pitfalls rule 5/6: exp044 cost +3,742 lines that way).
  claims.delete(oldName);
}

/** True when an applied rename bound `name` directly in this scope's block. */
function isClaimedIn(scope: Scope, name: string): boolean {
  return renameClaims.get(scope.block)?.has(name) ?? false;
}

/**
 * The binding `name` resolves to from an ANCESTOR of `scope`, consulting the
 * claim ledger at each level so a rename applied through the other scope tree
 * is visible. Mirrors `scope.parent.getBinding(name)`, which sees only its
 * own tree's maps.
 */
function resolveOuterBinding(scope: Scope, name: string): Binding | undefined {
  // parent is typed non-null but is undefined at runtime on the Program scope
  for (let s: Scope | undefined = scope.parent; s; s = s.parent) {
    const claimed = renameClaims.get(s.block)?.get(name);
    if (claimed) return claimed;
    const own = s.bindings[name];
    if (own) return own;
  }
  return undefined;
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
      if (refScope.bindings[newName] || isClaimedIn(refScope, newName)) {
        return true;
      }
      refScope = refScope.parent;
    }
  }
  return false;
}

/**
 * True when a binding named `newName`, visible from an ancestor of `scope`,
 * has a reference or write INSIDE `scope`'s block subtree. Renaming a
 * binding of `scope` to `newName` re-resolves those paths to the renamed
 * binding — a semantic capture (the 2.1.166 transport bug: an inner env
 * local renamed to the outer variable's name swallowed the outer's
 * assignment). Shadowing an outer name that is never referenced inside the
 * scope changes no resolution and stays allowed — a blanket ancestor-
 * visibility rejection starves transfers and suggestions of safe names.
 */
function wouldCaptureOuterReference(scope: Scope, newName: string): boolean {
  const outer = resolveOuterBinding(scope, newName);
  if (!outer) return false;
  const block = scope.block;
  const { start, end } = block;
  const inside = (p: NodePath) => {
    const node = p.node;
    // Position containment when available (O(1)); renames never move nodes.
    if (
      start != null &&
      end != null &&
      node.start != null &&
      node.end != null
    ) {
      return node.start >= start && node.end <= end;
    }
    return node === block || Boolean(p.findParent((a) => a.node === block));
  };
  return (
    outer.referencePaths.some(inside) || outer.constantViolations.some(inside)
  );
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
  if (scope.bindings[newName] || isClaimedIn(scope, newName)) {
    return "target-in-scope";
  }
  if (wouldCaptureOuterReference(scope, newName)) return "target-visible";
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
export function isExportInvolved(binding: Binding): boolean {
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
  // After the consistency check, so a claim is only ever recorded for a rename
  // that actually landed in this scope's map.
  recordRenameClaim(scope, oldName, newName);
  return { applied: true };
}

/**
 * Rename an inner binding — a class or function EXPRESSION's own id — to the
 * name of the outer binding it is assigned to, deliberately shadowing it.
 *
 * `getRenameRejection` forbids this as `target-visible` (the target name is
 * bound in the parent scope), but for `X = class q {}` the shadow is exactly
 * what the original source said (`X = class X {}`) and is behavior-
 * preserving: inside the expression the inner id already denotes the same
 * object as the outer binding. The one thing that breaks it is a reference
 * to the OUTER binding living inside the expression subtree (e.g.
 * `class q extends X {}`): after the rename that reference re-resolves to
 * the inner id. This variant permits the intentional shadow but keeps every
 * other guard — validity, inner-scope collision, child-scope shadowing — and
 * adds the subtree-capture check.
 */
export function attemptShadowingRename(
  innerBinding: Binding,
  ownerBinding: Binding,
  newName: string
): RenameAttempt {
  const scope = innerBinding.scope;
  const oldName = innerBinding.identifier.name;
  if (!isValidRenameTarget(newName)) {
    return { applied: false, reason: "invalid-target" };
  }
  if (ownerBinding.identifier.name !== newName || !scope.bindings[oldName]) {
    return { applied: false, reason: "no-binding" };
  }
  if (scope.bindings[newName] || isClaimedIn(scope, newName)) {
    return { applied: false, reason: "target-in-scope" };
  }
  if (referencesOwnerInside(ownerBinding, scope.block)) {
    return { applied: false, reason: "capture-in-subtree" };
  }
  if (wouldRenameShadowInChildScope(scope, oldName, newName)) {
    return { applied: false, reason: "shadows-child" };
  }
  // Never fall back to Babel's scope.rename here: an export-involved inner
  // id is not a thing, and scope.rename would re-traverse the whole block.
  if (!fastRenameBinding(scope, oldName, newName)) {
    return { applied: false, reason: "target-in-scope" };
  }
  if (!scope.bindings[newName] || scope.bindings[oldName]) {
    throw new Error(
      `shadow rename ${oldName}→${newName} left scope bindings inconsistent`
    );
  }
  recordRenameClaim(scope, oldName, newName);
  return { applied: true };
}

/** True when any read or write of `owner` sits inside `block`'s subtree. */
function referencesOwnerInside(owner: Binding, block: t.Node): boolean {
  const inside = (p: NodePath) =>
    Boolean(p.findParent((a) => a.node === block));
  return (
    owner.referencePaths.some(inside) || owner.constantViolations.some(inside)
  );
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
