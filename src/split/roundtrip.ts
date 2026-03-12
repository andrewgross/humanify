import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import * as esbuild from "esbuild";

/**
 * Result of round-trip validation.
 */
export interface RoundtripResult {
  /** Whether the bundle succeeded */
  bundleSuccess: boolean;
  /** Error message if bundling failed */
  bundleError?: string;
  /** Export names found in the bundled output */
  bundledExports: string[];
  /** Export names found in the original file */
  originalExports: string[];
  /** Exports present in original but missing from bundle */
  missingExports: string[];
  /** Exports present in bundle but not in original */
  extraExports: string[];
  /** Whether all original exports are preserved */
  exportsMatch: boolean;
}

/**
 * Bundle a split output directory back together using esbuild.
 * Expects an index.js in the outputDir as the entry point.
 */
export async function bundleSplitOutput(outputDir: string): Promise<string> {
  const entryPoint = path.join(outputDir, "index.js");
  if (!fs.existsSync(entryPoint)) {
    throw new Error(`No index.js found in ${outputDir}`);
  }

  const result = await esbuild.build({
    entryPoints: [entryPoint],
    bundle: true,
    write: false,
    format: "esm",
    outdir: "out"
  });

  if (result.errors.length > 0) {
    throw new Error(
      `esbuild errors: ${result.errors.map((e) => e.text).join(", ")}`
    );
  }

  return result.outputFiles[0].text;
}

/**
 * Extract export names from JavaScript source code using Babel.
 */
export function extractExportNames(code: string): string[] {
  const exports: string[] = [];

  let ast: t.File | null = null;
  for (const sourceType of ["module", "script"] as const) {
    try {
      const result = parseSync(code, { sourceType });
      if (result && result.type === "File") {
        ast = result;
        break;
      }
    } catch {
      // Try next
    }
  }

  if (!ast) return exports;

  for (const node of ast.program.body) {
    if (t.isExportNamedDeclaration(node)) {
      // export { a, b } or export { a } from './foo'
      for (const spec of node.specifiers) {
        if (t.isExportSpecifier(spec)) {
          const name = t.isIdentifier(spec.exported)
            ? spec.exported.name
            : spec.exported.value;
          exports.push(name);
        }
      }
      // export function foo() {} or export const bar = ...
      if (node.declaration) {
        if (t.isFunctionDeclaration(node.declaration) && node.declaration.id) {
          exports.push(node.declaration.id.name);
        } else if (
          t.isClassDeclaration(node.declaration) &&
          node.declaration.id
        ) {
          exports.push(node.declaration.id.name);
        } else if (t.isVariableDeclaration(node.declaration)) {
          for (const decl of node.declaration.declarations) {
            if (t.isIdentifier(decl.id)) {
              exports.push(decl.id.name);
            }
          }
        }
      }
    } else if (t.isExportDefaultDeclaration(node)) {
      exports.push("default");
    }
  }

  return [...new Set(exports)].sort();
}

/**
 * Validate that split output can be re-bundled and preserves the original API.
 */
export async function validateRoundtrip(
  originalPath: string,
  outputDir: string
): Promise<RoundtripResult> {
  // Extract exports from original
  const originalCode = fs.readFileSync(originalPath, "utf-8");
  const originalExports = extractExportNames(originalCode);

  // Bundle the split output
  let bundledCode: string;
  try {
    bundledCode = await bundleSplitOutput(outputDir);
  } catch (err) {
    return {
      bundleSuccess: false,
      bundleError: String(err),
      bundledExports: [],
      originalExports,
      missingExports: originalExports,
      extraExports: [],
      exportsMatch: false
    };
  }

  const bundledExports = extractExportNames(bundledCode);

  const originalSet = new Set(originalExports);
  const bundledSet = new Set(bundledExports);

  const missingExports = originalExports.filter((e) => !bundledSet.has(e));
  const extraExports = bundledExports.filter((e) => !originalSet.has(e));

  return {
    bundleSuccess: true,
    bundledExports,
    originalExports,
    missingExports,
    extraExports,
    exportsMatch: missingExports.length === 0
  };
}
