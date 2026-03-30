/**
 * Split adapter interface.
 *
 * Each adapter implements a strategy for grouping functions into output
 * modules. Adapters are tried in order; the first that supports the
 * detected module pattern is used. CallGraphAdapter is the fallback.
 */
import type { FunctionNode } from "../../analysis/types.js";
import type { ClusterOptions } from "../cluster.js";
import type { ModuleDetectionResult } from "../module-detect.js";
import type { ParsedFile } from "../types.js";

export type SplitStrategyType =
  | "esbuild-esm"
  | "esbuild-cjs"
  | "bun-cjs"
  | "webpack"
  | "call-graph";

export interface SplitAdapterOptions extends ClusterOptions {}

export interface SplitAdapter {
  /** Adapter identifier, matches --split-strategy CLI values. */
  name: SplitStrategyType;

  /** Does this adapter handle the given detection result? */
  supports(detection: ModuleDetectionResult): boolean;

  /**
   * Group functions into output modules.
   * Returns Map<sessionId, outputFileName>.
   */
  groupFunctions(
    functions: FunctionNode[],
    parsedFiles: ParsedFile[],
    detection: ModuleDetectionResult,
    options?: SplitAdapterOptions
  ): Map<string, string>;
}
