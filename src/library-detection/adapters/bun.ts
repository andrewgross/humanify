/**
 * Bun library detector.
 *
 * Every file extracted from a Bun CJS bundle's factory wrapper is
 * third-party library code by construction — modern TS/ESM app code never
 * lands inside a `var X = HELPER(...)` factory. The adapter writes a
 * sidecar manifest naming each factory file (via the structural-hash
 * cascade); this detector reads that manifest and marks every listed file
 * as library.
 *
 * Falls back to scanning for bang-block banners if the manifest is absent
 * (older bundles, or callers that ran a custom adapter).
 */

import fs from "node:fs/promises";
import path from "node:path";
import {
  BUN_MODULES_MANIFEST,
  type BunModulesManifest
} from "../../unpack/adapters/bun.js";
import type { PipelineConfig } from "../../pipeline/types.js";
import type { WebcrackFile } from "../../plugins/webcrack.js";
import { BANNER_PATTERNS, normalizeLibraryName } from "../banner-patterns.js";
import type {
  LibraryDetection,
  LibraryDetectionResult,
  LibraryDetector
} from "../types.js";

export class BunLibraryDetector implements LibraryDetector {
  name = "bun";

  supports(config: PipelineConfig): boolean {
    return config.unpackAdapterName === "bun";
  }

  async detectLibraries(
    files: WebcrackFile[]
  ): Promise<LibraryDetectionResult> {
    const manifest = await loadManifest(files);
    if (manifest) {
      return classifyViaManifest(files, manifest);
    }
    return classifyViaBannerScan(files);
  }
}

async function loadManifest(
  files: WebcrackFile[]
): Promise<BunModulesManifest | null> {
  if (files.length === 0) return null;
  const dir = path.dirname(files[0].path);
  const manifestPath = path.join(dir, BUN_MODULES_MANIFEST);
  try {
    const raw = await fs.readFile(manifestPath, "utf-8");
    return JSON.parse(raw) as BunModulesManifest;
  } catch {
    return null;
  }
}

function classifyViaManifest(
  files: WebcrackFile[],
  manifest: BunModulesManifest
): LibraryDetectionResult {
  const libraryFiles = new Map<string, LibraryDetection>();
  const novelFiles: string[] = [];

  const factoryByName = new Map(manifest.factories.map((e) => [e.fileName, e]));
  for (const file of files) {
    const base = path.basename(file.path);
    if (base === manifest.runtimeFile) {
      novelFiles.push(file.path);
      continue;
    }
    const entry = factoryByName.get(base);
    if (!entry) {
      novelFiles.push(file.path);
      continue;
    }
    libraryFiles.set(file.path, {
      isLibrary: true,
      libraryName: entry.name,
      detectedBy: "comment",
      moduleMetadata: file.metadata
    });
  }

  return { libraryFiles, novelFiles, mixedFiles: new Map() };
}

async function classifyViaBannerScan(
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

  return { libraryFiles, novelFiles, mixedFiles: new Map() };
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
