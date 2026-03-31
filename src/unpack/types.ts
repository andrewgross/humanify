import type { ModuleMetadata, WebcrackFile } from "../plugins/webcrack.js";
import type { BundlerDetectionResult } from "../detection/types.js";

export interface UnpackAdapter {
  name: string;
  supports(detection: BundlerDetectionResult): boolean;
  unpack(code: string, outputDir: string): Promise<UnpackResult>;
}

export interface UnpackResult {
  files: WebcrackFile[];
  moduleMetadata?: Map<string, ModuleMetadata>;
}

export type { WebcrackFile, ModuleMetadata };
