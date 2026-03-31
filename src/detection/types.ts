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
