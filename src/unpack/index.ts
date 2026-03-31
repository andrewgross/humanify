import type {
  BundlerType,
  BundlerDetectionResult
} from "../detection/types.js";
import type { PipelineConfig } from "../pipeline/types.js";
import { BunUnpackAdapter } from "./adapters/bun.js";
import { PassthroughAdapter } from "./adapters/passthrough.js";
import { WebcrackAdapter } from "./adapters/webcrack.js";
import type { UnpackAdapter } from "./types.js";

const adapters: UnpackAdapter[] = [
  new WebcrackAdapter(),
  new BunUnpackAdapter(),
  new PassthroughAdapter() // must be last (fallback)
];

/**
 * Select the appropriate unpack adapter for a pipeline config.
 */
export function selectUnpackAdapter(config: PipelineConfig): UnpackAdapter {
  const match = adapters.find((a) => a.name === config.unpackAdapterName);
  if (!match)
    throw new Error(`No unpack adapter named "${config.unpackAdapterName}"`);
  return match;
}

interface SelectAdapterOptions {
  bundlerOverride?: BundlerType;
}

/**
 * Select the appropriate unpack adapter from a detection result.
 *
 * Used by buildPipelineConfig to determine the adapter name before
 * the PipelineConfig exists.
 */
export function selectAdapter(
  detection: BundlerDetectionResult,
  options?: SelectAdapterOptions
): UnpackAdapter {
  // If user forces a bundler type, build a synthetic detection result
  if (options?.bundlerOverride && options.bundlerOverride !== "unknown") {
    const overridden: BundlerDetectionResult = {
      ...detection,
      bundler: { type: options.bundlerOverride, tier: "definitive" }
    };
    const match = adapters.find((a) => a.supports(overridden));
    if (match) return match;
  }

  const match = adapters.find((a) => a.supports(detection));
  // PassthroughAdapter always matches, so this should never be undefined
  if (!match) throw new Error("No adapter found for detection result");
  return match;
}

export type { UnpackAdapter, UnpackResult } from "./types.js";
