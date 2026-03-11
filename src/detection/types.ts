import type { WebcrackFile, ModuleMetadata } from "../plugins/webcrack.js";

export type BundlerType = "webpack" | "browserify" | "rollup" | "esbuild" | "parcel" | "bun" | "unknown";
export type MinifierType = "terser" | "esbuild" | "swc" | "bun" | "none" | "unknown";
export type DetectionTier = "definitive" | "likely" | "unknown";

export interface DetectionSignal {
  source: string;
  pattern: string;
  bundler?: BundlerType;
  minifier?: MinifierType;
  tier: DetectionTier;
}

export interface DetectionResult {
  bundler?: { type: BundlerType; tier: DetectionTier; version?: string };
  minifier?: { type: MinifierType; tier: DetectionTier };
  signals: DetectionSignal[];
}

export interface BundlerAdapter {
  name: string;
  supports(detection: DetectionResult): boolean;
  unpack(code: string, outputDir: string): Promise<UnpackResult>;
}

export interface UnpackResult {
  files: WebcrackFile[];
  moduleMetadata?: Map<string, ModuleMetadata>;
}

export type { WebcrackFile, ModuleMetadata };
