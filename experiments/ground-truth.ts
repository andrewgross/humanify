/**
 * Ground truth extraction from source maps or cached JSON.
 *
 * Given a bundle and its source map, maps each function in the AST
 * back to its original source file using source map lookups.
 *
 * For humanified fixtures (where line numbers shifted after rename),
 * supports loading from a pre-computed ground-truth.json that maps
 * by function ordinal instead of source map position.
 */
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { SourceMapConsumer } from "source-map";
import type { FunctionNode } from "../src/analysis/types.js";
import { normalizeModulePath } from "../src/split/module-detect.js";
import type { GroundTruthMapping, SplitAssignment } from "./types.js";

/**
 * Build ground truth mapping from functions to their original source files.
 *
 * For each function in the graph, looks up its start position in the source map
 * to determine which original file it came from.
 */
export async function extractGroundTruth(
  functions: FunctionNode[],
  sourceMapPath: string
): Promise<GroundTruthMapping> {
  const rawMap = JSON.parse(readFileSync(sourceMapPath, "utf-8"));

  const functionToFile = new Map<string, string>();
  const fileToFunctions = new Map<string, string[]>();

  await SourceMapConsumer.with(rawMap, null, (consumer) => {
    for (const fn of functions) {
      // Only consider top-level functions (same as splitter)
      if (fn.scopeParent) continue;

      const loc = fn.path.node.loc;
      if (!loc) continue;

      const original = consumer.originalPositionFor({
        line: loc.start.line,
        column: loc.start.column
      });

      if (!original.source) continue;

      const normalizedSource = normalizeModulePath(original.source);
      functionToFile.set(fn.sessionId, normalizedSource);

      const existing = fileToFunctions.get(normalizedSource);
      if (existing) {
        existing.push(fn.sessionId);
      } else {
        fileToFunctions.set(normalizedSource, [fn.sessionId]);
      }
    }
  });

  const sourceFiles = Array.from(fileToFunctions.keys()).sort();

  return { functionToFile, fileToFunctions, sourceFiles };
}

/**
 * Load ground truth from a pre-computed JSON cache.
 *
 * Used for humanified fixtures where the source map no longer matches
 * (line numbers shifted after LLM rename + prettier). The cache maps
 * by function ordinal — function ordering is preserved through rename.
 */
export function extractGroundTruthFromCache(
  functions: FunctionNode[],
  cachePath: string
): GroundTruthMapping {
  const cached = JSON.parse(readFileSync(cachePath, "utf-8"));
  const entries: Array<{ ordinal: number; originalFile: string }> =
    cached.functionsByOrdinal;

  const topLevel = functions
    .filter((fn) => !fn.scopeParent)
    .sort((a, b) => {
      const aLine = a.path.node.loc?.start.line ?? 0;
      const bLine = b.path.node.loc?.start.line ?? 0;
      if (aLine !== bLine) return aLine - bLine;
      const aCol = a.path.node.loc?.start.column ?? 0;
      const bCol = b.path.node.loc?.start.column ?? 0;
      return aCol - bCol;
    });

  const functionToFile = new Map<string, string>();
  const fileToFunctions = new Map<string, string[]>();

  // Match by ordinal: the i-th function in the humanified bundle
  // corresponds to the i-th function in the original bundle
  const limit = Math.min(topLevel.length, entries.length);
  for (let i = 0; i < limit; i++) {
    const fn = topLevel[i];
    const entry = entries[i];
    functionToFile.set(fn.sessionId, entry.originalFile);

    const existing = fileToFunctions.get(entry.originalFile);
    if (existing) {
      existing.push(fn.sessionId);
    } else {
      fileToFunctions.set(entry.originalFile, [fn.sessionId]);
    }
  }

  const sourceFiles = Array.from(fileToFunctions.keys()).sort();
  return { functionToFile, fileToFunctions, sourceFiles };
}

/**
 * Extract ground truth, automatically choosing between source map and cache.
 *
 * Prefers source map lookup. Falls back to cached ground-truth.json if
 * the source map is missing (e.g., humanified fixtures).
 */
export async function extractGroundTruthAuto(
  functions: FunctionNode[],
  sourceMapPath: string
): Promise<GroundTruthMapping> {
  const dir = dirname(sourceMapPath);
  const cachePath = join(dir, "ground-truth.json");

  if (existsSync(sourceMapPath)) {
    const result = await extractGroundTruth(functions, sourceMapPath);
    // If source map produced results, use them; otherwise try cache
    if (result.sourceFiles.length > 0) return result;
  }

  if (existsSync(cachePath)) {
    return extractGroundTruthFromCache(functions, cachePath);
  }

  throw new Error(
    `No source map or ground-truth.json found for ${sourceMapPath}. ` +
      `Run: tsx experiments/prepare-ground-truth.ts <fixture>`
  );
}

/**
 * Build split assignment from the splitter's cluster plan.
 *
 * Maps each function to its assigned output file based on cluster membership.
 */
export function extractSplitAssignment(
  functions: FunctionNode[],
  clusterFileMap: Map<string, string>
): SplitAssignment {
  const functionToFile = new Map<string, string>();
  const fileToFunctions = new Map<string, string[]>();

  for (const fn of functions) {
    if (fn.scopeParent) continue;

    const outputFile = clusterFileMap.get(fn.sessionId);
    if (!outputFile) continue;

    functionToFile.set(fn.sessionId, outputFile);

    const existing = fileToFunctions.get(outputFile);
    if (existing) {
      existing.push(fn.sessionId);
    } else {
      fileToFunctions.set(outputFile, [fn.sessionId]);
    }
  }

  const outputFiles = Array.from(fileToFunctions.keys()).sort();

  return { functionToFile, fileToFunctions, outputFiles };
}
