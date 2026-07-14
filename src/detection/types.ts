export type BundlerType =
  | "webpack"
  | "browserify"
  | "rollup"
  | "esbuild"
  | "parcel"
  | "bun"
  | "unknown";
export type MinifierType =
  | "terser"
  | "esbuild"
  | "swc"
  | "bun"
  | "none"
  | "unknown";
export type DetectionTier = "definitive" | "likely" | "unknown";

/**
 * Values a user may force via `--bundler` / `--minifier`. These exclude the
 * `"unknown"` sentinel (it is the no-override signal, so forcing it is a no-op)
 * and drive both the CLI help text and upfront flag-value validation, keeping
 * the two in sync. `satisfies` pins each entry to a real enum member.
 */
export const SELECTABLE_BUNDLERS = [
  "webpack",
  "browserify",
  "rollup",
  "esbuild",
  "parcel",
  "bun"
] as const satisfies readonly BundlerType[];

export const SELECTABLE_MINIFIERS = [
  "terser",
  "esbuild",
  "swc",
  "bun",
  "none"
] as const satisfies readonly MinifierType[];

export interface DetectionSignal {
  source: string;
  pattern: string;
  bundler?: BundlerType;
  minifier?: MinifierType;
  tier: DetectionTier;
}

export interface BundlerDetectionResult {
  bundler?: { type: BundlerType; tier: DetectionTier; version?: string };
  minifier?: { type: MinifierType; tier: DetectionTier };
  signals: DetectionSignal[];
}
