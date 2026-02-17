import type { ModuleMetadata } from "../plugins/webcrack.js";

export interface LibraryDetection {
  /** Whether this file was detected as library code */
  isLibrary: boolean;
  /** Name of the detected library, if identified */
  libraryName?: string;
  /** Which detection method identified this as a library */
  detectedBy?: "path" | "comment";
  /** The module metadata from webcrack, if available */
  moduleMetadata?: ModuleMetadata;
}

export interface DetectionResult {
  /** Files classified as library code (skipped during processing) */
  libraryFiles: Map<string, LibraryDetection>;
  /** Files classified as application code (processed normally) */
  novelFiles: string[];
}
