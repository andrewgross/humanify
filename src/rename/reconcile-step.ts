/**
 * Pipeline step: prior-diff reconciliation over a freshly generated output.
 *
 * Re-parses the generated code (so binding locs are in OUTPUT coordinates,
 * matching the rendered-text diff), snaps rename-noise bindings back to the
 * prior version's names via reconcileDiffNoise, and re-generates.
 *
 * This is an OPTIONAL, best-effort pass: it must never lose an otherwise-
 * valid run. Any internal failure — an unparseable output, a missing `diff`
 * binary, or a reconciliation that trips the pure-rename invariant — is
 * contained here and returns undefined, so the caller ships the validated
 * pre-reconcile output. The pure-rename guarantee is enforced against a
 * LOCAL baseline captured on the re-parsed output; a violation discards the
 * reconciliation (loudly) rather than failing the run.
 */

import { parseSync } from "@babel/core";
import type { GeneratorOptions } from "@babel/generator";
import type * as t from "@babel/types";
import { generate } from "../babel-utils.js";
import { debug } from "../debug.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant
} from "../output-validation.js";
import { computeNormalDiff, reconcileDiffNoise } from "./diff-reconcile.js";
import type { IsEligibleFn } from "./rename-eligibility.js";

export interface AppliedRename {
  fromName: string;
  toName: string;
}

export interface PriorDiffReconcileOutcome {
  stats: { renames: number; skipped: number };
  /** The applied (new → prior) name pairs, for diagnostics reconciliation. */
  renames: AppliedRename[];
  /** Regenerated code — set only when renames applied and the invariant held. */
  code?: string;
  /** AST of `code`, for downstream consumers (e.g. split). */
  ast?: t.File;
}

function reconcileInternal(
  code: string,
  priorVersionCode: string,
  isEligible: IsEligibleFn,
  genOpts: GeneratorOptions
): PriorDiffReconcileOutcome | undefined {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File | null;
  if (!ast) return undefined;

  const baseline = captureSemanticBaseline(ast);
  const diffText = computeNormalDiff(priorVersionCode, code);
  const result = reconcileDiffNoise(ast, diffText, {
    apply: true,
    descriptiveTier: true,
    isEligible,
    priorLineCount: priorVersionCode.split("\n").length
  });
  const applied = result.renames.map((r) => ({
    fromName: r.fromName,
    toName: r.toName
  }));
  const stats = { renames: applied.length, skipped: result.skipped.length };
  if (applied.length === 0) return { stats, renames: applied };

  // The reconciliation mutated `ast` in place; the local invariant proves
  // it changed only binding names relative to the pre-reconcile output. On
  // violation, discard the reconciliation entirely — the pre-reconcile
  // output the caller holds is still valid.
  const failure = checkStructuralInvariant(ast, baseline);
  if (failure) {
    debug.log(
      "reconcile-prior-diff",
      `discarded: reconciliation violated the pure-rename invariant ` +
        `(${failure.message}); shipping the pre-reconcile output`
    );
    return undefined;
  }
  return { stats, renames: applied, code: generate(ast, genOpts).code, ast };
}

/**
 * Best-effort prior-diff reconciliation. Returns undefined when the pass
 * did not run, had nothing to apply that survived validation, or hit any
 * internal error (never throws — an optional pass must not abort a
 * completed run).
 */
export function runPriorDiffReconciliation(
  code: string,
  priorVersionCode: string,
  isEligible: IsEligibleFn,
  genOpts: GeneratorOptions
): PriorDiffReconcileOutcome | undefined {
  try {
    return reconcileInternal(code, priorVersionCode, isEligible, genOpts);
  } catch (err) {
    debug.log(
      "reconcile-prior-diff",
      `skipped: ${err instanceof Error ? err.message : String(err)}`
    );
    return undefined;
  }
}
