/**
 * Split adapter registry and selection.
 *
 * Mirrors the pattern from src/detection/adapters.ts:
 *   detectModules() -> selectSplitAdapter() -> adapter.groupFunctions()
 */
import type { ModuleDetectionResult } from "../module-detect.js";
import { CallGraphAdapter } from "./call-graph.js";
import { PositionalSplitAdapter } from "./positional-assignment.js";
import type { SplitAdapter, SplitStrategyType } from "./types.js";

const adapters: SplitAdapter[] = [
  new PositionalSplitAdapter("esbuild-esm"),
  new PositionalSplitAdapter("esbuild-cjs"),
  new PositionalSplitAdapter("bun-cjs"),
  new CallGraphAdapter() // must be last (fallback)
];

/**
 * Every strategy name that has an adapter — derived from the registry, never
 * restated. Two hand-written copies drifted from it: `VALID_STRATEGIES` in
 * `commands/split.ts` omitted `bun-cjs` (which the help text advertised), and
 * `SplitStrategyType` listed `webpack`, for which no adapter exists.
 */
export const SPLIT_STRATEGY_NAMES = adapters.map(
  (a) => a.name
) as readonly SplitStrategyType[];

/**
 * Select the appropriate split adapter for a detection result.
 *
 * When strategyOverride is set, that adapter is used regardless of
 * detection. Otherwise, the first adapter whose supports() returns
 * true is selected (CallGraphAdapter always matches as fallback).
 */
export function selectSplitAdapter(
  detection: ModuleDetectionResult,
  strategyOverride?: SplitStrategyType
): SplitAdapter {
  if (strategyOverride) {
    const forced = adapters.find((a) => a.name === strategyOverride);
    // Loudly, not silently: with no else-branch an unrecognised override fell
    // through to detection, so a typo (or a strategy that was declared but
    // never implemented, as `webpack` was) looked accepted and changed nothing.
    if (!forced) {
      throw new Error(
        `unknown split strategy "${strategyOverride}" — known: ${SPLIT_STRATEGY_NAMES.join(", ")}`
      );
    }
    return forced;
  }

  const match = adapters.find((a) => a.supports(detection));
  // CallGraphAdapter always matches, so this should never be undefined
  if (!match) throw new Error("No split adapter found for detection result");
  return match;
}

export type {
  SplitAdapter,
  SplitAdapterOptions,
  SplitStrategyType
} from "./types.js";
