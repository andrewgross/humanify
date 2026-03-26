/**
 * Split adapter registry and selection.
 *
 * Mirrors the pattern from src/detection/adapters.ts:
 *   detectModules() -> selectSplitAdapter() -> adapter.groupFunctions()
 */
import type { DetectionResult } from "../module-detect.js";
import { CallGraphAdapter } from "./call-graph.js";
import { EsbuildCJSAdapter } from "./esbuild-cjs.js";
import { EsbuildESMAdapter } from "./esbuild-esm.js";
import type { SplitAdapter, SplitStrategyType } from "./types.js";

const adapters: SplitAdapter[] = [
  new EsbuildESMAdapter(),
  new EsbuildCJSAdapter(),
  new CallGraphAdapter() // must be last (fallback)
];

/**
 * Select the appropriate split adapter for a detection result.
 *
 * When strategyOverride is set, that adapter is used regardless of
 * detection. Otherwise, the first adapter whose supports() returns
 * true is selected (CallGraphAdapter always matches as fallback).
 */
export function selectSplitAdapter(
  detection: DetectionResult,
  strategyOverride?: SplitStrategyType
): SplitAdapter {
  if (strategyOverride) {
    const forced = adapters.find((a) => a.name === strategyOverride);
    if (forced) return forced;
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
