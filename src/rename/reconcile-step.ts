/**
 * Pipeline step: prior-diff reconciliation over a freshly generated output.
 *
 * Re-parses the generated code (so binding locs are in OUTPUT coordinates,
 * matching the rendered-text diff), snaps rename-noise bindings back to the
 * prior version's names via reconcileDiffNoise, and re-generates. The
 * pure-rename guarantee is enforced against a LOCAL semantic baseline
 * captured on the re-parsed output — a reconciliation that changed anything
 * but binding names returns a failure the caller must treat as fatal.
 *
 * The caller (createRenamePlugin) additionally re-validates the returned
 * code against the run's original semantic baseline.
 */

import { parseSync } from "@babel/core";
import type { GeneratorOptions } from "@babel/generator";
import type * as t from "@babel/types";
import { generate } from "../babel-utils.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant,
  type OutputSemanticFailure
} from "../output-validation.js";
import { computeNormalDiff, reconcileDiffNoise } from "./diff-reconcile.js";
import type { IsEligibleFn } from "./rename-eligibility.js";

export interface PriorDiffReconcileOutcome {
  stats: { renames: number; skipped: number };
  /** Regenerated code — set only when renames applied and the invariant held. */
  code?: string;
  /** AST of `code`, for downstream consumers (e.g. split). */
  ast?: t.File;
  /** Set when the pass violated the pure-rename invariant — fatal. */
  failure?: OutputSemanticFailure;
}

export function runPriorDiffReconciliation(
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
  // A generated output that does not re-parse is already reported by the
  // caller's output validation; nothing for this pass to do.
  if (!ast) return undefined;

  const baseline = captureSemanticBaseline(ast);
  const diffText = computeNormalDiff(priorVersionCode, code);
  const result = reconcileDiffNoise(ast, diffText, {
    apply: true,
    descriptiveTier: true,
    isEligible
  });
  const stats = {
    renames: result.renames.length,
    skipped: result.skipped.length
  };
  if (result.renames.length === 0) return { stats };

  const failure = checkStructuralInvariant(ast, baseline);
  if (failure) return { stats, failure };
  return { stats, code: generate(ast, genOpts).code, ast };
}
