import { readFileSync } from "fs";
import type { SourceFunction } from "./ground-truth.js";

/**
 * Extract source code for functions from their source files.
 */
export function extractFunctionCode(
  functions: SourceFunction[],
  sourceFiles: string[]
): Map<string, string> {
  const result = new Map<string, string>();

  // Load all source files
  const fileContents = new Map<string, string[]>();
  for (const filePath of sourceFiles) {
    try {
      const content = readFileSync(filePath, "utf-8");
      fileContents.set(filePath, content.split("\n"));
    } catch {
      // File not found - skip
    }
  }

  for (const fn of functions) {
    // Find the source file
    const filePath = sourceFiles.find((f) => f.endsWith(fn.file));
    if (!filePath) continue;

    const lines = fileContents.get(filePath);
    if (!lines) continue;

    // Extract the function code
    const startLine = fn.location.startLine - 1; // 0-indexed
    const endLine = fn.location.endLine; // exclusive

    if (startLine >= 0 && endLine <= lines.length) {
      const extracted = lines.slice(startLine, endLine).join("\n");
      result.set(fn.id, extracted);
    }
  }

  return result;
}
