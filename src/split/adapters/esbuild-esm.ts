/**
 * Split adapter for esbuild ESM bundles.
 *
 * Detects `// path/to/file.ts` comments that esbuild leaves as module
 * boundaries. Groups functions by which comment region they fall in;
 * anything outside every region goes to shared.js. The grouping body
 * lives on PositionalSplitAdapter.
 */
import type { ModuleDetectionResult } from "../module-detect.js";
import { PositionalSplitAdapter } from "./positional-assignment.js";

export class EsbuildESMAdapter extends PositionalSplitAdapter {
  name = "esbuild-esm" as const;

  supports(detection: ModuleDetectionResult): boolean {
    return detection.bundler === "esbuild-esm" && detection.modules.length >= 2;
  }
}
