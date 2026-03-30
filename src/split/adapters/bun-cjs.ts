/**
 * Split adapter for Bun CJS bundles.
 *
 * Groups functions by which Bun CJS factory wrapper they fall within;
 * functions outside any factory go to shared.js.
 */
import type { FunctionNode } from "../../analysis/types.js";
import {
  assignFunctionsToModules,
  type DetectionResult
} from "../module-detect.js";
import type { ParsedFile } from "../types.js";
import type { SplitAdapter, SplitAdapterOptions } from "./types.js";

export class BunCJSAdapter implements SplitAdapter {
  name = "bun-cjs" as const;

  supports(detection: DetectionResult): boolean {
    return detection.bundler === "bun-cjs" && detection.modules.length >= 2;
  }

  groupFunctions(
    functions: FunctionNode[],
    _parsedFiles: ParsedFile[],
    detection: DetectionResult,
    _options?: SplitAdapterOptions
  ): Map<string, string> {
    const topLevel = functions.filter((fn) => !fn.scopeParent);
    const fnPositions = topLevel.map((fn) => ({
      sessionId: fn.sessionId,
      startLine: fn.path.node.loc?.start.line ?? 0
    }));

    const assignment = assignFunctionsToModules(fnPositions, detection.modules);

    // Functions outside any factory wrapper go to shared.js
    for (const fn of topLevel) {
      if (!assignment.has(fn.sessionId)) {
        assignment.set(fn.sessionId, "shared.js");
      }
    }

    return assignment;
  }
}
