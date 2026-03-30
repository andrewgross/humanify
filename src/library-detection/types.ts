import type { BundlerAdapter } from "../detection/types.js";
import type { ModuleMetadata, WebcrackFile } from "../plugins/webcrack.js";
import type { CommentRegion } from "./comment-regions.js";

export interface LibraryDetection {
  /** Whether this file was detected as library code */
  isLibrary: boolean;
  /** Name of the detected library, if identified */
  libraryName?: string;
  /** Which detection method identified this as a library */
  detectedBy?: "path" | "comment" | "comment-region";
  /** The module metadata from webcrack, if available */
  moduleMetadata?: ModuleMetadata;
}

/** Detection info for a mixed file (library + app code interleaved) */
export interface MixedFileDetection {
  /** Comment regions found in the file */
  regions: CommentRegion[];
  /** Library names found in the regions */
  libraryNames: string[];
}

export interface LibraryDetectionResult {
  /** Files classified as library code (skipped during processing) */
  libraryFiles: Map<string, LibraryDetection>;
  /** Files classified as application code (processed normally) */
  novelFiles: string[];
  /** Files with interleaved library/app code (Rollup/esbuild bundles) */
  mixedFiles: Map<string, MixedFileDetection>;
}

export interface LibraryDetector {
  name: string;
  supports(bundlerAdapter: BundlerAdapter): boolean;
  detectLibraries(files: WebcrackFile[]): Promise<LibraryDetectionResult>;
}
