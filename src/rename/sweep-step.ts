/**
 * Pipeline step: prior-aware (deferred) LLM coverage sweep.
 *
 * In a prior-aware run the naming floor's LLM sweep must NOT run before the
 * prior-diff reconciliation: the reconcile pass's asymmetric tier transfers
 * the prior version's name onto every minted sweep target with a clean
 * positional counterpart (deterministic, cross-version stable), and a fresh
 * LLM name applied first would turn that easy minified→descriptive case
 * into a descriptive↔descriptive conflict the reconcile pass rightly
 * refuses (exp022). So the sweep runs HERE, after reconciliation, over the
 * shipping output — whatever is still minted at this point truly has no
 * usable prior counterpart and is the LLM's to name.
 *
 * Like the reconcile step, this is best-effort and self-contained: it
 * parses the shipping code privately, applies the sweep through the
 * validated path, proves the result is a pure rename against a LOCAL
 * baseline, and re-generates. Any internal failure or invariant violation
 * discards the sweep (loudly) and the caller ships its existing output.
 */

import { parseSync } from "@babel/core";
import type { GeneratorOptions } from "@babel/generator";
import type * as t from "@babel/types";
import { collectEvalWithTaint } from "../analysis/soundness.js";
import { generate } from "../babel-utils.js";
import { debug } from "../debug.js";
import type { LLMProvider } from "../llm/types.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant
} from "../output-validation.js";
import { sweepMintedNames } from "./coverage-sweep.js";
import type { IsEligibleFn } from "./rename-eligibility.js";

export interface DeferredSweepOutcome {
  /** Bindings force-named and applied through the validated path. */
  named: number;
  /** Targets skipped (declined, collided, or discarded by the invariant). */
  skipped: number;
  /** Regenerated code — set only when renames applied and the invariant held. */
  code?: string;
  /** AST of `code`, for downstream consumers (e.g. split). */
  ast?: t.File;
}

async function sweepInternal(
  code: string,
  provider: LLMProvider,
  isEligible: IsEligibleFn,
  opts: { concurrency: number; genOpts: GeneratorOptions }
): Promise<DeferredSweepOutcome | undefined> {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File | null;
  if (!ast) return undefined;

  const taint = collectEvalWithTaint(ast);
  const baseline = captureSemanticBaseline(ast);
  const sweep = await sweepMintedNames(ast, provider, isEligible, taint, {
    concurrency: opts.concurrency
  });
  if (sweep.named === 0) {
    return { named: 0, skipped: sweep.skipped };
  }

  // The sweep mutated the private AST; the local invariant proves it
  // changed only binding names relative to the shipping output. On
  // violation, discard the sweep entirely — the caller's output is valid.
  const failure = checkStructuralInvariant(ast, baseline);
  if (failure) {
    debug.log(
      "naming-floor",
      `deferred sweep discarded: pure-rename invariant violated ` +
        `(${failure.message}); shipping the pre-sweep output`
    );
    return { named: 0, skipped: sweep.named + sweep.skipped };
  }
  return {
    named: sweep.named,
    skipped: sweep.skipped,
    code: generate(ast, opts.genOpts).code,
    ast
  };
}

/**
 * Best-effort deferred sweep. Returns undefined when the pass could not
 * run at all (unparseable input or an internal error — never throws: an
 * optional pass must not abort a completed run); returns stats without
 * code when nothing was applied.
 */
export async function runDeferredSweep(
  code: string,
  provider: LLMProvider,
  isEligible: IsEligibleFn,
  opts: { concurrency: number; genOpts: GeneratorOptions }
): Promise<DeferredSweepOutcome | undefined> {
  try {
    return await sweepInternal(code, provider, isEligible, opts);
  } catch (err) {
    debug.log(
      "naming-floor",
      `deferred sweep skipped: ${err instanceof Error ? err.message : String(err)}`
    );
    return undefined;
  }
}
