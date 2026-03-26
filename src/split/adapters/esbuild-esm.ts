/**
 * Split adapter for esbuild ESM bundles.
 *
 * Detects `// path/to/file.ts` comments that esbuild leaves as module
 * boundaries. Groups functions by which comment region they fall in.
 */
import type { FunctionNode } from "../../analysis/types.js";
import {
  assignFunctionsToModules,
  type DetectionResult
} from "../module-detect.js";
import type { ParsedFile } from "../types.js";
import type { SplitAdapter, SplitAdapterOptions } from "./types.js";

export class EsbuildESMAdapter implements SplitAdapter {
  name = "esbuild-esm" as const;

  supports(detection: DetectionResult): boolean {
    return detection.bundler === "esbuild-esm" && detection.modules.length >= 2;
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

    // Unassigned top-level functions go to shared.js
    for (const fn of topLevel) {
      if (!assignment.has(fn.sessionId)) {
        assignment.set(fn.sessionId, "shared.js");
      }
    }

    return assignment;
  }
}
