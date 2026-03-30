/**
 * Bun library detector — full-body banner scan.
 *
 * In Bun CJS bundles, each factory IS one module boundary. If a banner
 * comment exists anywhere in a factory body, the entire file is library code.
 * No mixed-file handling needed.
 */

import fs from "node:fs/promises";
import type { BundlerAdapter } from "../../detection/types.js";
import type { WebcrackFile } from "../../plugins/webcrack.js";
import { BANNER_PATTERNS, normalizeLibraryName } from "../banner-patterns.js";
import type {
  LibraryDetection,
  LibraryDetectionResult,
  LibraryDetector
} from "../types.js";

export class BunLibraryDetector implements LibraryDetector {
  name = "bun";

  supports(bundlerAdapter: BundlerAdapter): boolean {
    return bundlerAdapter.name === "bun";
  }

  async detectLibraries(
    files: WebcrackFile[]
  ): Promise<LibraryDetectionResult> {
    const libraryFiles = new Map<string, LibraryDetection>();
    const novelFiles: string[] = [];

    for (const file of files) {
      const code = await fs.readFile(file.path, "utf-8");
      const libraryName = scanForBanner(code);

      if (libraryName) {
        libraryFiles.set(file.path, {
          isLibrary: true,
          libraryName,
          detectedBy: "comment",
          moduleMetadata: file.metadata
        });
      } else {
        novelFiles.push(file.path);
      }
    }

    // Bun factories are single modules — no mixed files
    return { libraryFiles, novelFiles, mixedFiles: new Map() };
  }
}

/**
 * Scan the entire file content for any banner pattern.
 * Returns the first library name found, or undefined.
 */
function scanForBanner(code: string): string | undefined {
  for (const pattern of BANNER_PATTERNS) {
    const regex = new RegExp(pattern.source, pattern.flags);
    const match = regex.exec(code);
    if (match?.[1]) {
      return normalizeLibraryName(match[1]);
    }
  }
  return undefined;
}
