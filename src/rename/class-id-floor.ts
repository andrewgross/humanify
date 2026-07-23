/**
 * Naming-floor workstream 1: deterministic class/function-EXPRESSION inner-id
 * derivation. No LLM.
 *
 * `X = class q {}` names the outer binding `X` through the normal pipeline
 * but leaves the class expression's own id `q` minted — it binds in the
 * expression's own scope, which no naming collector visits. This pass
 * derives the inner id's name from what the expression is assigned to
 * (declarator id, plain-identifier assignment target, member/property key),
 * producing `X = class X {}` — almost certainly what the original source
 * said. Stable across versions by construction, so it also starves the
 * reroll bucket at the source.
 *
 * When the derivation source is a real binding whose name we copy, the
 * rename is an intentional shadow and goes through `attemptShadowingRename`
 * (the standard validated path would reject it as target-visible); when the
 * source is a bare property (`obj.Foo = class q {}`), no binding is
 * shadowed and the normal `attemptValidatedRename` applies. Every gate
 * defaults to skip; eval/with-frozen scopes are never touched.
 */

import type { Binding, NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import {
  type EvalWithTaint,
  isBindingEvalTaintFrozen
} from "../analysis/soundness.js";
import { collectMintedBindings } from "./minted-census.js";
import type { IsEligibleFn } from "./rename-eligibility.js";
import {
  attemptShadowingRename,
  attemptValidatedRename
} from "./validated-rename.js";

export interface ClassIdFloorSkip {
  name: string;
  toName: string | null;
  reason: string;
}

export interface ClassIdFloorResult {
  derived: number;
  skipped: ClassIdFloorSkip[];
}

/**
 * The outer binding whose name the derivation copies, when it is a real
 * binding (declarator id or plain-identifier assignment target). Null for
 * property/member targets — those shadow nothing and use the normal path.
 */
function ownerBindingFor(exprPath: NodePath, name: string): Binding | null {
  const parent = exprPath.parentPath;
  if (!parent) return null;
  const isIdentTarget =
    (parent.isVariableDeclarator() && t.isIdentifier(parent.node.id)) ||
    (parent.isAssignmentExpression() && t.isIdentifier(parent.node.left));
  if (!isIdentTarget) return null;
  return parent.scope.getBinding(name) ?? null;
}

export function deriveExpressionInnerNames(
  ast: t.Node,
  isEligible: IsEligibleFn,
  taint: EvalWithTaint
): ClassIdFloorResult {
  const candidates = collectMintedBindings(ast, isEligible).entries.filter(
    (b) => b.family === "classExprId" || b.family === "fnExprId"
  );
  const result: ClassIdFloorResult = { derived: 0, skipped: [] };

  for (const cand of candidates) {
    const toName = cand.derivedFrom;
    const skip = (reason: string) =>
      result.skipped.push({ name: cand.name, toName, reason });

    if (toName === null) {
      skip("no-derivation-source");
      continue;
    }
    if (isBindingEvalTaintFrozen(cand.binding, taint)) {
      skip("eval-taint-frozen");
      continue;
    }
    const owner = ownerBindingFor(cand.binding.path, toName);
    const attempt = owner
      ? attemptShadowingRename(cand.binding, owner, toName)
      : attemptValidatedRename(cand.binding.scope, cand.name, toName);
    if (attempt.applied) {
      result.derived += 1;
    } else {
      skip(mapReason(attempt.reason));
    }
  }
  return result;
}

function mapReason(reason: string | undefined): string {
  return reason === "capture-in-subtree"
    ? "capture-in-subtree"
    : `rename-rejected:${reason ?? "unknown"}`;
}
