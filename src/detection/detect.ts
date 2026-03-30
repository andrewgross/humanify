import { detectBrowserify } from "./signals/browserify.js";
import { detectBunBundler } from "./signals/bun.js";
import { detectEsbuild } from "./signals/esbuild.js";
import { detectMinifier } from "./signals/minifier.js";
import { detectParcel } from "./signals/parcel.js";
import { detectWebpack } from "./signals/webpack.js";
import type {
  BundlerType,
  DetectionResult,
  DetectionSignal,
  MinifierType
} from "./types.js";

/** Maximum bytes to scan for signals (16KB) */
const SCAN_LIMIT = 16 * 1024;

type BundlerDetector = (code: string) => DetectionSignal[];

const BUNDLER_DETECTORS: BundlerDetector[] = [
  detectWebpack,
  detectBrowserify,
  detectEsbuild,
  detectParcel,
  detectBunBundler
];

export function detectBundle(code: string): DetectionResult {
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

  let bundler: DetectionResult["bundler"];
  if (definiteBundlerSignals.length > 0) {
    // Use the first definitive bundler signal (they should agree)
    const type = definiteBundlerSignals[0].bundler as BundlerType;
    bundler = { type, tier: "definitive" };
  } else {
    bundler = { type: "unknown", tier: "unknown" };
  }

  // Pick minifier from likely signals
  const minifierSignals = allSignals.filter((s) => s.minifier);
  let minifier: DetectionResult["minifier"];
  if (minifierSignals.length > 0) {
    const type = minifierSignals[0].minifier as MinifierType;
    minifier = { type, tier: minifierSignals[0].tier };
  } else {
    minifier = { type: "unknown", tier: "unknown" };
  }

  return { bundler, minifier, signals: allSignals };
}
