/**
 * Split adapter for esbuild CJS bundles.
 *
 * Detects `var x = moduleFactory(...)` wrappers that esbuild uses for
 * CommonJS modules. Groups functions by which factory they fall in;
 * functions outside any factory go to shared.js. The grouping body
 * lives on PositionalSplitAdapter.
 */
import type { ModuleDetectionResult } from "../module-detect.js";
import { PositionalSplitAdapter } from "./positional-assignment.js";

export class EsbuildCJSAdapter extends PositionalSplitAdapter {
  name = "esbuild-cjs" as const;

  supports(detection: ModuleDetectionResult): boolean {
    return detection.bundler === "esbuild-cjs" && detection.modules.length >= 2;
  }
}
