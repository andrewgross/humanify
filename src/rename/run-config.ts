/**
 * Per-run configuration, resolved ONCE at a pipeline entry point
 * (createRenamePlugin, processUnified) and passed down as a required
 * value. No layer below an entry point may re-default these — an
 * argument-less createIsEligible() ignores bundler/minifier types and
 * silently diverges from the entry-resolved rules.
 */
import type { BundlerType, MinifierType } from "../detection/types.js";
import type { Profiler } from "../profiling/profiler.js";
import { NULL_PROFILER } from "../profiling/profiler.js";
import type { IsEligibleFn } from "./rename-eligibility.js";
import { createIsEligible } from "./rename-eligibility.js";

export interface RunConfig {
  /** Rename-eligibility rules resolved for the detected bundler/minifier. */
  isEligible: IsEligibleFn;
  profiler: Profiler;
  bundlerType?: BundlerType;
  minifierType?: MinifierType;
}

/** The single place run defaults are applied. An explicit isEligible wins. */
export function resolveRunConfig(
  options: {
    isEligible?: IsEligibleFn;
    profiler?: Profiler;
    bundlerType?: BundlerType;
    minifierType?: MinifierType;
  } = {}
): RunConfig {
  return {
    isEligible:
      options.isEligible ??
      createIsEligible(options.bundlerType, options.minifierType),
    profiler: options.profiler ?? NULL_PROFILER,
    bundlerType: options.bundlerType,
    minifierType: options.minifierType
  };
}
