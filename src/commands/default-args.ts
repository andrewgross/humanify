import type { BundlerType } from "../detection/types.js";

export const DEFAULT_CONCURRENCY = 50;

/**
 * LLM request timeout. One number, because it was three.
 *
 * The CLI's `--timeout` default said `300000`, `OpenAICompatibleProvider`
 * fell back to `30000`, and `experiments/prepare-humanified.ts` hard-coded
 * `300000` again. The 10x-smaller one was unreachable from the CLI (commander
 * always supplies its default), so this was a latent divergence rather than a
 * live bug — but the two experiment scripts that build a provider without
 * passing a timeout did get 30s against a local vLLM that routinely takes
 * longer, and any new direct caller would have too.
 */
export const DEFAULT_LLM_TIMEOUT_MS = 300_000;

/**
 * Module-lane concurrency, per bundler. esbuild bundles hold many more
 * independent module bindings, so their lane is wider.
 *
 * Single-sourced because the value appeared three times: twice in
 * `rename/processor.ts` (the real schedulers) and once in `commands/unified.ts`
 * sizing the LLM rate limiter's ceiling, where it was written bundler-UNAWARE
 * as a flat 40.
 *
 * That last one is not a bug and must not be "fixed" by making it
 * bundler-aware: `buildProvider` runs BEFORE `detectBundle`, so no bundler is
 * known there, and the rate limiter's `maxConcurrent` is an OUTER bound over
 * both of the processor's limiters (`concurrency` + this). 40 is the maximum
 * this function can return, so the bound is always >= what the processor
 * actually schedules and therefore never binds. It only becomes a bug if
 * someone raises a lane above 40 here and does not look at the ceiling — which
 * is precisely why the number now lives in one place.
 */
export function defaultModuleConcurrency(bundlerType?: BundlerType): number {
  return bundlerType === "esbuild" ? 40 : 20;
}

/** The widest lane `defaultModuleConcurrency` can return — the outer bound the
 * LLM rate limiter is sized against when the bundler is not yet known. */
export const MAX_DEFAULT_MODULE_CONCURRENCY = 40;
