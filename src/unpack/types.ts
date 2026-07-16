import type { ModuleMetadata, WebcrackFile } from "../plugins/webcrack.js";
import type { BundlerDetectionResult } from "../detection/types.js";

export interface UnpackOptions {
  /** Optional LLM namer for hash-named vendored factories (bun adapter). */
  vendorNamer?: import("./vendor-namer.js").VendorNamer;
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
