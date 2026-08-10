/**
 * Split adapter for Bun CJS bundles.
 *
 * Groups functions by which Bun CJS factory wrapper they fall within;
 * functions outside any factory go to shared.js. The grouping body
 * lives on PositionalSplitAdapter.
 */
import type { ModuleDetectionResult } from "../module-detect.js";
import { PositionalSplitAdapter } from "./positional-assignment.js";

export class BunCJSAdapter extends PositionalSplitAdapter {
  name = "bun-cjs" as const;

  supports(detection: ModuleDetectionResult): boolean {
    return detection.bundler === "bun-cjs" && detection.modules.length >= 2;
  }
}
