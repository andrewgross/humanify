import type * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../babel-utils.js";

/**
 * Detection of constructs that make name-based renaming unsound.
 *
 * A `with (obj) { ... }` block resolves bare identifiers against obj's
 * properties at RUNTIME; a direct `eval(...)` executes source that can
 * reference any binding visible at the call site by its ORIGINAL name.
 * Renaming any binding visible at such a site can change behavior while
 * still parsing cleanly — the output gates cannot catch it.
 *
 * The sound response is to freeze everything visible at the site: every
 * enclosing function's own bindings, and (since scope chains end there)
 * the module-level bindings too. Functions off the scope chain keep
 * renaming — eval'd code cannot see their locals.
 */
export interface EvalWithTaint {
  /** Function AST nodes on some taint site's scope chain */
  taintedFunctions: Set<t.Node>;
  /** True when any site exists — module-level bindings are always visible */
  moduleTainted: boolean;
  /** Number of with/direct-eval sites found */
  siteCount: number;
}

/** True for `eval(...)` where eval is NOT a local binding (direct eval). */
function isDirectEval(path: babelTraverse.NodePath<t.CallExpression>): boolean {
  const callee = path.node.callee;
  if (!t.isIdentifier(callee) || callee.name !== "eval") return false;
  // A locally bound eval is an ordinary function — renaming stays sound.
  return !path.scope.getBinding("eval");
}

/** Walk up from a taint site, collecting every enclosing function node. */
function taintScopeChain(
  sitePath: babelTraverse.NodePath,
  taintedFunctions: Set<t.Node>
): void {
  let fnPath = sitePath.getFunctionParent();
  while (fnPath) {
    taintedFunctions.add(fnPath.node);
    fnPath = fnPath.parentPath?.getFunctionParent() ?? null;
  }
}

/**
 * Collects eval/with taint for an AST. Runs once per file at graph-build
 * time; the plugin marks tainted functions (and, when any site exists,
 * module bindings) as done so neither the LLM pass nor prior-version
 * transfer renames them.
 */
export function collectEvalWithTaint(ast: t.File): EvalWithTaint {
  const taintedFunctions = new Set<t.Node>();
  let siteCount = 0;

  traverse(ast, {
    WithStatement(path: babelTraverse.NodePath<t.WithStatement>) {
      siteCount++;
      taintScopeChain(path, taintedFunctions);
    },
    CallExpression(path: babelTraverse.NodePath<t.CallExpression>) {
      if (!isDirectEval(path)) return;
      siteCount++;
      taintScopeChain(path, taintedFunctions);
    }
  });

  return { taintedFunctions, moduleTainted: siteCount > 0, siteCount };
}

/**
 * True when renaming `binding` is unsound because it is visible at a
 * `with`/direct-eval site: any binding on a tainted function's scope chain,
 * and — since scope chains end at the module — every module-level binding
 * whenever any site exists. The one predicate every rename pass consults so
 * the freeze rule cannot drift between them (the graph pass freezes these
 * pre-emptively via markEvalWithTaintPreDone; passes that run afterward,
 * like the diff-reconcile and naming-floor passes, re-check it here).
 */
export function isBindingEvalTaintFrozen(
  binding: babelTraverse.Binding,
  taint: EvalWithTaint
): boolean {
  if (taint.siteCount === 0) return false;
  const fnScope = binding.scope.getFunctionParent();
  if (!fnScope) return taint.moduleTainted;
  return taint.taintedFunctions.has(fnScope.block);
}
