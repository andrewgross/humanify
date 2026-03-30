import { BunUnpackAdapter } from "./adapters/bun.js";
import { PassthroughAdapter } from "./adapters/passthrough.js";
import { WebcrackAdapter } from "./adapters/webcrack.js";
import type { BundlerAdapter, BundlerType, DetectionResult } from "./types.js";

const adapters: BundlerAdapter[] = [
  new WebcrackAdapter(),
  new BunUnpackAdapter(),
  new PassthroughAdapter() // must be last (fallback)
];

interface SelectAdapterOptions {
  bundlerOverride?: BundlerType;
}

export function selectAdapter(
  detection: DetectionResult,
  options?: SelectAdapterOptions
): BundlerAdapter {
  // If user forces a bundler type, build a synthetic detection result
  if (options?.bundlerOverride && options.bundlerOverride !== "unknown") {
    const overridden: DetectionResult = {
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
