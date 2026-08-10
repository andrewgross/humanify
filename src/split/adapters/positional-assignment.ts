/**
 * Shared grouping body for the positional split adapters (bun-cjs,
 * esbuild-cjs, esbuild-esm). Each adapter differs only in what it
 * `supports()`; the assignment itself is identical: map top-level
 * functions to detected module regions by source position, and send
 * anything outside every region to shared.js.
 */
import type { FunctionNode } from "../../analysis/types.js";
import {
  assignFunctionsToModules,
  type ModuleDetectionResult
} from "../module-detect.js";
import type { ParsedFile } from "../types.js";
import type {
  SplitAdapter,
  SplitAdapterOptions,
  SplitStrategyType
} from "./types.js";

/**
 * Base class owning the ONE `groupFunctions` body the three positional
 * adapters share (they were byte-identical copies until 2026-08-10).
 * Subclasses provide only `name` and `supports()`.
 */
export abstract class PositionalSplitAdapter implements SplitAdapter {
  abstract name: SplitStrategyType;

  abstract supports(detection: ModuleDetectionResult): boolean;

  groupFunctions(
    functions: FunctionNode[],
    _parsedFiles: ParsedFile[],
    detection: ModuleDetectionResult,
    _options?: SplitAdapterOptions
  ): Map<string, string> {
    return groupByModulePosition(functions, detection);
  }
}

/** Assign top-level functions to modules by position; unassigned → shared.js. */
export function groupByModulePosition(
  functions: FunctionNode[],
  detection: ModuleDetectionResult
): Map<string, string> {
  const topLevel = functions.filter((fn) => !fn.scopeParent);
  const fnPositions = topLevel.map((fn) => ({
    sessionId: fn.sessionId,
    startLine: fn.path.node.loc?.start.line ?? 0
  }));

  const assignment = assignFunctionsToModules(fnPositions, detection.modules);

  // Functions outside any detected module region go to shared.js
  for (const fn of topLevel) {
    if (!assignment.has(fn.sessionId)) {
      assignment.set(fn.sessionId, "shared.js");
    }
  }

  return assignment;
}
