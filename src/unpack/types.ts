import type { ModuleMetadata, WebcrackFile } from "../plugins/webcrack.js";
import type { BundlerDetectionResult } from "../detection/types.js";

export interface UnpackOptions {
  /** Optional LLM namer for hash-named vendored factories (bun adapter). */
  vendorNamer?: import("./vendor-namer.js").VendorNamer;
  /**
   * Prior release's vendor names (structuralHash → name) for cross-version
   * carry-over. Applied in the naming cascade AHEAD of the LLM, so an
   * unchanged library keeps the name the lineage already used instead of
   * whatever the model answers this run.
   */
  priorVendorNames?: Map<string, string>;
}

export interface UnpackAdapter {
  name: string;
  supports(detection: BundlerDetectionResult): boolean;
  unpack(
    code: string,
    outputDir: string,
    options?: UnpackOptions
  ): Promise<UnpackResult>;
}

export interface UnpackResult {
  files: WebcrackFile[];
  moduleMetadata?: Map<string, ModuleMetadata>;
}

export type { WebcrackFile, ModuleMetadata };
