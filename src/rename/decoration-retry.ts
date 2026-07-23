/**
 * Naming-floor workstream 3: decoration retry.
 *
 * When an LLM-chosen name collides with a name already taken in scope, the
 * pipeline decorates it — usually a trailing underscore (`initializeApp_`).
 * The stem is already the RIGHT name; the decoration is an artifact of the
 * collision landscape at naming time. Once the blocking binding has moved
 * on (renamed away, or never re-collided this run), the bare stem is free
 * again. This pass retries the undecorated stem through the validated path;
 * if the collision still stands, `attemptValidatedRename` rejects it and
 * the decorated name is left untouched.
 *
 * Deterministic, no LLM. Only trailing-underscore decorations of a
 * descriptive stem are handled — a stem that is itself a minted token
 * belongs to the derivation / sweep, not here.
 */

import type * as t from "@babel/types";
import {
  type EvalWithTaint,
  isBindingEvalTaintFrozen
} from "../analysis/soundness.js";
import {
  collectMintedBindings,
  isDecoratedDescriptive
} from "./minted-census.js";
import type { IsEligibleFn } from "./rename-eligibility.js";
import { strategyTrail } from "./strategy-trail.js";
import { attemptValidatedRename } from "./validated-rename.js";

export interface DecorationRetryResult {
  undecorated: number;
  skipped: number;
}

export function retryDecoratedNames(
  ast: t.Node,
  isEligible: IsEligibleFn,
  taint: EvalWithTaint
): DecorationRetryResult {
  const result: DecorationRetryResult = { undecorated: 0, skipped: 0 };
  for (const entry of collectMintedBindings(ast, isEligible).entries) {
    if (!isDecoratedDescriptive(entry.name)) continue;
    if (isBindingEvalTaintFrozen(entry.binding, taint)) {
      result.skipped += 1;
      strategyTrail.recordPostPass(entry.binding, entry.name, {
        strategy: "decoration-retry",
        outcome: "abstained",
        reason: "eval-taint-frozen"
      });
      continue;
    }
    const stem = entry.name.replace(/_+$/, "");
    const attempt = attemptValidatedRename(
      entry.binding.scope,
      entry.name,
      stem
    );
    if (attempt.applied) {
      result.undecorated += 1;
      strategyTrail.recordPostPass(entry.binding, entry.name, {
        strategy: "decoration-retry",
        outcome: "applied",
        newName: stem
      });
    } else {
      result.skipped += 1;
      strategyTrail.recordPostPass(entry.binding, entry.name, {
        strategy: "decoration-retry",
        outcome: "abstained",
        reason: attempt.reason ?? "still-blocked",
        newName: stem
      });
    }
  }
  return result;
}
