/**
 * Default library detector — 3-layer detection.
 *
 * 1. Module path matching (from webcrack bundle metadata)
 * 2. Comment/banner detection (scan file headers for library identifiers)
 * 3. Intra-file comment regions (full file scan for mixed files)
 *
 * This is the fallback detector — it always matches.
 */

import fs from "node:fs/promises";
import type { PipelineConfig } from "../../pipeline/types.js";
import type { WebcrackFile } from "../../plugins/webcrack.js";
import { BANNER_PATTERNS, normalizeLibraryName } from "../banner-patterns.js";
import { findCommentRegions } from "../comment-regions.js";
import type {
  LibraryDetection,
  LibraryDetectionResult,
  LibraryDetector,
  MixedFileDetection
} from "../types.js";

/**
 * Patterns that identify a module path as library code.
 * Checked against the module path from webcrack's bundle metadata.
 */
const LIBRARY_PATH_PATTERNS: RegExp[] = [
  /node_modules\//,
  /^@babel\/runtime/,
  /^core-js/,
  /^regenerator-runtime/,
  /^tslib/,
  /^webpack\/runtime/
];

/** Maximum bytes to scan for comment banners in header */
const SCAN_LIMIT = 1024;

export class DefaultLibraryDetector implements LibraryDetector {
  name = "default";

  supports(_config: PipelineConfig): boolean {
    return true;
  }

  async detectLibraries(
    files: WebcrackFile[]
  ): Promise<LibraryDetectionResult> {
    const libraryFiles = new Map<string, LibraryDetection>();
    const novelFiles: string[] = [];
    const mixedFiles = new Map<string, MixedFileDetection>();

    for (const file of files) {
      const detection = await detectFile(file);
      if (detection.isLibrary) {
        libraryFiles.set(file.path, detection);
      } else if (detection.mixedFileDetection) {
        mixedFiles.set(file.path, detection.mixedFileDetection);
        novelFiles.push(file.path);
      } else {
        novelFiles.push(file.path);
      }
    }

    return { libraryFiles, novelFiles, mixedFiles };
  }
}

/** Extended result from detectFile that may include mixed file info */
interface FileDetectionResult extends LibraryDetection {
  mixedFileDetection?: MixedFileDetection;
}

async function detectFile(file: WebcrackFile): Promise<FileDetectionResult> {
  // Layer 1: Module path matching
  if (file.metadata?.modulePath) {
    if (isLibraryPath(file.metadata.modulePath)) {
      return {
        isLibrary: true,
        libraryName: extractLibraryNameFromPath(file.metadata.modulePath),
        detectedBy: "path",
        moduleMetadata: file.metadata
      };
    }
  }

  // Layer 2: Comment/banner detection (first ~1KB)
  const code = await fs.readFile(file.path, "utf-8");
  const libraryName = detectLibraryFromHeader(code);
  if (libraryName) {
    return {
      isLibrary: true,
      libraryName,
      detectedBy: "comment",
      moduleMetadata: file.metadata
    };
  }

  // Layer 3: Intra-file comment regions (full file scan)
  const regions = findCommentRegions(code);
  if (regions.length > 0) {
    const libraryNames = [...new Set(regions.map((r) => r.libraryName))];
    return {
      isLibrary: false,
      moduleMetadata: file.metadata,
      mixedFileDetection: { regions, libraryNames }
    };
  }

  // Not detected as library
  return {
    isLibrary: false,
    moduleMetadata: file.metadata
  };
}

/**
 * Extract library names from comment banners in the first ~1KB of code.
 */
function detectLibraryFromHeader(code: string): string | undefined {
  const header = code.slice(0, SCAN_LIMIT);

  for (const pattern of BANNER_PATTERNS) {
    const match = header.match(pattern);
    if (match?.[1]) {
      return normalizeLibraryName(match[1]);
    }
  }

  return undefined;
}

/**
 * Check if a module path matches known library patterns.
 */
export function isLibraryPath(modulePath: string): boolean {
  return LIBRARY_PATH_PATTERNS.some((pattern) => pattern.test(modulePath));
}

/**
 * Extract a human-readable library name from a module path.
 * e.g., "node_modules/react-dom/cjs/react-dom.production.min.js" → "react-dom"
 */
export function extractLibraryNameFromPath(modulePath: string): string {
  const nodeModulesMatch = modulePath.match(
    /node_modules\/(@[^/]+\/[^/]+|[^/]+)/
  );
  if (nodeModulesMatch) {
    return nodeModulesMatch[1];
  }

  // For paths like "@babel/runtime/helpers/..." return the package name
  const scopedMatch = modulePath.match(/^(@[^/]+\/[^/]+)/);
  if (scopedMatch) {
    return scopedMatch[1];
  }

  // Return the first path segment
  return modulePath.split("/")[0];
}
