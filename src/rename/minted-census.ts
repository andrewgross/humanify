/**
 * Minted-token census: count surviving minifier-minted identifier bindings
 * in a (re-)parsed output, classified by family. This is exp021's meter —
 * the pipeline reports it end-of-run so "not renamed" becomes truthful, and
 * the floor passes consume the same binding list to target their work.
 *
 * The token shape (`isBunToken`) is deliberately LOOSE and biased toward
 * over-counting: it is the DETECTION heuristic (what might be a leftover),
 * kept separate from the reconcile pass's metric heuristic
 * (`isMinifiedName` in diff-reconcile.ts) and from the stricter predicate a
 * force-naming sweep must use before it rewrites anything.
 */

import type { Binding, NodePath, Scope } from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import type { IsEligibleFn } from "./rename-eligibility.js";

export type MintedFamily =
  | "classExprId"
  | "fnExprId"
  | "param"
  | "fnDecl"
  | "varOther";

export interface MintedBinding {
  name: string;
  line: number | undefined;
  family: MintedFamily;
  /** For class/function-expression ids: the name derivation would use. */
  derivedFrom: string | null;
  refCount: number;
  /** The Babel binding, for floor passes that act on it. */
  binding: Binding;
}

/**
 * Short dictionary words the length-≤2 rule must not flag (it catches
 * minted survivors like `qA`, `q7`).
 */
const SHORT_WORDS = new Set(
  (
    "fs os id url env obj err ctx arg key val map set get idx row col end tag " +
    "raw fn cb ok ip add has del min max sum abs pos len dir ext sep cwd pid " +
    "uid gid now run log out res req msg str num x y i j k n a b e t"
  ).split(" ")
);

/**
 * Bun-token shape: `$` anywhere, a trailing underscore, a 1–2 letter head
 * followed by a digit/underscore (`uq6`, `M2_`, `FH3`), or a very short
 * non-word. Looser than a rename-safe predicate on purpose — this only
 * FLAGS candidates; every consumer applies its own precision gate.
 */
export function isBunToken(name: string): boolean {
  if (name.includes("$")) return true;
  if (/_$/.test(name)) return true;
  if (/^[A-Za-z]{1,2}[0-9_]/.test(name)) return true;
  if (name.length <= 2 && !SHORT_WORDS.has(name.toLowerCase())) return true;
  return false;
}

/** Descriptive name wearing a trailing-underscore collision decoration. */
export function isDecoratedDescriptive(name: string): boolean {
  if (!/_$/.test(name)) return false;
  const stem = name.replace(/_+$/, "");
  return stem.length > 0 && !isBunToken(stem);
}

/** Minified stem + descriptive CamelCase tail, e.g. `RP_ConstructorKey`. */
export function isHalfNamedSuffix(name: string): boolean {
  return /^[A-Za-z]{1,2}[0-9]*_[A-Z][a-z]/.test(name);
}

function classify(binding: Binding): MintedFamily {
  const path = binding.path;
  if (path.isClassExpression()) return "classExprId";
  if (path.isFunctionExpression()) return "fnExprId";
  if (binding.kind === "param") return "param";
  if (path.isFunctionDeclaration() || path.isClassDeclaration())
    return "fnDecl";
  return "varOther";
}

function nameOfAssignmentTarget(left: t.Node): string | null {
  if (t.isIdentifier(left)) return left.name;
  if (
    t.isMemberExpression(left) &&
    !left.computed &&
    t.isIdentifier(left.property)
  ) {
    return left.property.name;
  }
  return null;
}

/**
 * The name a deterministic derivation would give a class/function
 * expression's inner id, in priority order: assignment target, variable
 * declarator id, object property key. Null when there is no source or the
 * source is itself minted.
 */
export function derivationSource(exprPath: NodePath): string | null {
  const parent = exprPath.parentPath;
  if (!parent) return null;
  let candidate: string | null = null;
  if (parent.isAssignmentExpression() && parent.node.right === exprPath.node) {
    candidate = nameOfAssignmentTarget(parent.node.left);
  } else if (
    parent.isVariableDeclarator() &&
    parent.node.init === exprPath.node &&
    t.isIdentifier(parent.node.id)
  ) {
    candidate = parent.node.id.name;
  } else if (
    parent.isObjectProperty() &&
    parent.node.value === exprPath.node &&
    !parent.node.computed &&
    t.isIdentifier(parent.node.key)
  ) {
    candidate = parent.node.key.name;
  }
  return candidate && !isBunToken(candidate) ? candidate : null;
}

/** Walk every scope once, collecting eligible minted bindings. */
export function collectMintedBindings(
  ast: t.Node,
  isEligible: IsEligibleFn
): MintedBinding[] {
  const seenScopes = new Set<Scope>();
  const seenBindings = new Set<Binding>();
  const entries: MintedBinding[] = [];

  traverse(ast, {
    Scopable(path: NodePath) {
      const scope = path.scope;
      if (seenScopes.has(scope)) return;
      seenScopes.add(scope);
      for (const [name, binding] of Object.entries(scope.bindings)) {
        if (seenBindings.has(binding)) continue;
        seenBindings.add(binding);
        if (!isEligible(name) || !isBunToken(name)) continue;
        const family = classify(binding);
        const isExprId = family === "classExprId" || family === "fnExprId";
        entries.push({
          name,
          line: binding.identifier.loc?.start.line,
          family,
          derivedFrom: isExprId ? derivationSource(binding.path) : null,
          refCount: binding.referencePaths.length,
          binding
        });
      }
    }
  });
  return entries;
}

export interface MintedCensus {
  total: number;
  byFamily: Record<MintedFamily, number>;
  /** Expression inner ids that have a derivable non-minted source name. */
  derivableExprIds: number;
  /** Expression inner ids with zero references (safest to rename). */
  zeroRefExprIds: number;
}

export function summarizeCensus(bindings: MintedBinding[]): MintedCensus {
  const byFamily: Record<MintedFamily, number> = {
    classExprId: 0,
    fnExprId: 0,
    param: 0,
    fnDecl: 0,
    varOther: 0
  };
  let derivableExprIds = 0;
  let zeroRefExprIds = 0;
  for (const entry of bindings) {
    byFamily[entry.family] += 1;
    if (entry.family === "classExprId" || entry.family === "fnExprId") {
      if (entry.derivedFrom !== null) derivableExprIds += 1;
      if (entry.refCount === 0) zeroRefExprIds += 1;
    }
  }
  return { total: bindings.length, byFamily, derivableExprIds, zeroRefExprIds };
}
