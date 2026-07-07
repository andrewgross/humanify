import { detectBrowserify } from "./signals/browserify.js";
import { detectBunBundler } from "./signals/bun.js";
import { detectEsbuild } from "./signals/esbuild.js";
import { detectMinifier } from "./signals/minifier.js";
import { detectParcel } from "./signals/parcel.js";
import { detectWebpack } from "./signals/webpack.js";
import type {
  BundlerType,
  BundlerDetectionResult,
  DetectionSignal,
  DetectionTier,
  MinifierType
} from "./types.js";

/** Maximum bytes to scan for signals (16KB) */
const SCAN_LIMIT = 16 * 1024;

/** Confidence ordering used for tier-aware signal selection (higher wins). */
const TIER_RANK: Record<DetectionTier, number> = {
  definitive: 2,
  likely: 1,
  unknown: 0
};

type BundlerDetector = (code: string) => DetectionSignal[];

const BUNDLER_DETECTORS: BundlerDetector[] = [
  detectWebpack,
  detectBrowserify,
  detectEsbuild,
  detectParcel,
  detectBunBundler
];

export function detectBundle(code: string): BundlerDetectionResult {
  const slice = code.slice(0, SCAN_LIMIT);

  const allSignals: DetectionSignal[] = [];

  // Run bundler detectors
  for (const detector of BUNDLER_DETECTORS) {
    allSignals.push(...detector(slice));
  }

  // Run minifier detectors
  allSignals.push(...detectMinifier(slice));

  // Pick bundler from definitive signals
  const definiteBundlerSignals = allSignals.filter(
    (s) => s.bundler && s.tier === "definitive"
  );

  let bundler: BundlerDetectionResult["bundler"];
  if (definiteBundlerSignals.length > 0) {
    // Use the first definitive bundler signal (they should agree)
    const type = definiteBundlerSignals[0].bundler as BundlerType;
    bundler = { type, tier: "definitive" };
  } else {
    bundler = { type: "unknown", tier: "unknown" };
  }

  // Pick minifier by highest tier, not array order: a distinctive esbuild/bun/swc
  // signal ("likely") outranks the generic terser fallback ("unknown"). Ties keep
  // the earliest signal in detector order, so selection is deterministic.
  const minifierSignals = allSignals.filter((s) => s.minifier);
  let minifier: BundlerDetectionResult["minifier"];
  if (minifierSignals.length > 0) {
    const best = minifierSignals.reduce((a, b) =>
      TIER_RANK[b.tier] > TIER_RANK[a.tier] ? b : a
    );
    minifier = { type: best.minifier as MinifierType, tier: best.tier };
  } else {
    minifier = { type: "unknown", tier: "unknown" };
  }

  return { bundler, minifier, signals: allSignals };
}
