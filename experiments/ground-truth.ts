/**
 * Ground truth extraction from source maps.
 *
 * Given a bundle and its source map, maps each function in the AST
 * back to its original source file using source map lookups.
 */
import { readFileSync } from "node:fs";
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
