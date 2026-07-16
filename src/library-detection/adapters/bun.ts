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
  bunManifestPath,
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
    const found = await loadManifest(files);
    if (found) {
      return classifyViaManifest(files, found.manifest, found.outputRoot);
    }
    return classifyViaBannerScan(files);
  }
}

/** Deepest a factory file sits below the output root:
 * vendor/@scope/pkg/file.js — three directory levels. */
const MAX_VENDOR_DEPTH = 3;

/**
 * Resolve the manifest AND the output root it is relative to. The manifest
 * lives at <root>/vendor/_bun-modules.json, but `files` may start with a
 * factory nested in a package folder, so walk up until it resolves rather
 * than assuming the first file is flat in vendor/.
 */
async function loadManifest(
  files: WebcrackFile[]
): Promise<{ manifest: BunModulesManifest; outputRoot: string } | null> {
  if (files.length === 0) return null;
  let dir = path.dirname(files[0].path);
  for (let up = 0; up <= MAX_VENDOR_DEPTH; up++) {
    try {
      const raw = await fs.readFile(bunManifestPath(dir), "utf-8");
      return {
        manifest: JSON.parse(raw) as BunModulesManifest,
        outputRoot: dir
      };
    } catch {
      const parent = path.dirname(dir);
      if (parent === dir) break;
      dir = parent;
    }
  }
  return null;
}

function classifyViaManifest(
  files: WebcrackFile[],
  manifest: BunModulesManifest,
  outputRoot: string
): LibraryDetectionResult {
  const libraryFiles = new Map<string, LibraryDetection>();
  const novelFiles: string[] = [];

  // Manifest paths (runtimeFile, factories[].fileName) are output-root-
  // relative, so compare on that — NOT the basename. Vendor names come from
  // the LLM, so a factory named "runtime" yields vendor/runtime.js, whose
  // basename equals the app's runtime.js; and package folders let two
  // factories share a basename. Either way a basename key is ambiguous, and
  // mistaking a factory for the app feeds library code to the rename pipeline.
  const factoryByPath = new Map(manifest.factories.map((e) => [e.fileName, e]));
  for (const file of files) {
    const rel = toManifestPath(outputRoot, file.path);
    if (rel === manifest.runtimeFile) {
      novelFiles.push(file.path);
      continue;
    }
    const entry = factoryByPath.get(rel);
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

/** An absolute file path as the manifest spells it: output-root-relative and
 * forward-slashed (the adapter writes POSIX separators on every platform). */
function toManifestPath(outputRoot: string, filePath: string): string {
  return path.relative(outputRoot, filePath).split(path.sep).join("/");
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
