import { BunUnpackAdapter } from "../unpack/adapters/bun.js";
import { PassthroughAdapter } from "../unpack/adapters/passthrough.js";
import { WebcrackAdapter } from "../unpack/adapters/webcrack.js";
import type { UnpackAdapter } from "../unpack/types.js";
import type { BundlerType, BundlerDetectionResult } from "./types.js";

const adapters: UnpackAdapter[] = [
  new WebcrackAdapter(),
  new BunUnpackAdapter(),
  new PassthroughAdapter() // must be last (fallback)
];

interface SelectAdapterOptions {
  bundlerOverride?: BundlerType;
}

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
