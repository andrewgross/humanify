#!/usr/bin/env tsx
/**
 * Analyze "not renamed" functions to understand why they weren't renamed.
 *
 * Usage:
 *   tsx scripts/analyze-not-renamed.ts <input-file> [--samples N]
 *
 * Categorizes all functions into:
 *   - Has LLM report (renamed by LLM)
 *   - Has library-prefix report
 *   - Zero own bindings (nothing to rename)
 *   - All bindings descriptive (already good names)
 *   - Library, no minified bindings
 *   - Unaccounted (potential bugs)
 *
 * Prints samples from each category so you can inspect whether
 * the classification is correct.
 */

import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/core";
import * as t from "@babel/types";
import { readFileSync } from "node:fs";
import { traverse } from "../src/babel-utils.js";
import { looksMinified } from "../src/rename/minified-heuristic.js";
import {
  findCommentRegions,
  classifyFunctionsByRegion
} from "../src/library-detection/comment-regions.js";
import type { FunctionNode } from "../src/analysis/types.js";

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

const args = process.argv.slice(2);
const inputFile = args.find((a) => !a.startsWith("--"));
const samplesArg = args.indexOf("--samples");
const sampleCount =
  samplesArg !== -1 ? Number.parseInt(args[samplesArg + 1], 10) : 5;

if (!inputFile) {
  console.error(
    "Usage: tsx scripts/analyze-not-renamed.ts <input-file> [--samples N]"
  );
  process.exit(1);
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface FunctionInfo {
  /** Location string like "line:col" */
  location: string;
  /** Total own bindings */
  totalBindings: number;
  /** Minified bindings (looksMinified=true) */
  minifiedBindings: number;
  /** Descriptive bindings (looksMinified=false) */
  descriptiveBindings: number;
  /** Binding names */
  bindingNames: string[];
  /** Library name if in a library region */
  libraryName?: string;
  /** Function type (arrow, function expression, function declaration) */
  fnType: string;
  /** First 120 chars of generated code */
  preview: string;
}

type Category =
  | "zero-bindings"
  | "all-descriptive"
  | "library-no-minified"
  | "has-minified"; // These SHOULD have been renamed — potential bugs

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

const code = readFileSync(inputFile, "utf-8");
console.log(
  `Parsing ${inputFile} (${(code.length / 1024 / 1024).toFixed(1)} MB)...`
);

const ast = parseSync(code, { sourceType: "unambiguous" });
if (!ast) {
  console.error("Failed to parse input file");
  process.exit(1);
}

// Detect library regions
const commentRegions = findCommentRegions(code);
console.log(`Found ${commentRegions.length} library comment regions`);

// Build a map of (line,col) -> libraryName using a simple offset approach
// We need function start offsets, so we'll check during traversal
const regionLookup = (startOffset: number): string | undefined => {
  for (let i = commentRegions.length - 1; i >= 0; i--) {
    const r = commentRegions[i];
    if (startOffset >= r.startOffset) {
      if (r.endOffset === null || startOffset < r.endOffset) {
        return r.libraryName;
      }
      return undefined;
    }
  }
  return undefined;
};

// ---------------------------------------------------------------------------
// Collect all functions
// ---------------------------------------------------------------------------

interface BindingEntry {
  name: string;
  isMinified: boolean;
}

function getOwnBindingNames(fnPath: NodePath<t.Function>): BindingEntry[] {
  const scope = fnPath.scope;
  const entries: BindingEntry[] = [];
  const seen = new Set<string>();

  // Own scope bindings
  for (const [name, binding] of Object.entries(scope.bindings)) {
    if (seen.has(name)) continue;
    // Check if binding is from this function (not inherited)
    const bindingScope = binding.scope;
    if (bindingScope === scope) {
      seen.add(name);
      entries.push({ name, isMinified: looksMinified(name) });
    }
  }

  // Block-scoped bindings in immediate body
  fnPath.traverse({
    // biome-ignore lint/style/useNamingConvention: Babel visitor
    BlockStatement(blockPath: NodePath<t.BlockStatement>) {
      // Only direct children blocks (if/for/etc), not nested functions
      if (blockPath.getFunctionParent() !== fnPath) return;
      for (const [name, binding] of Object.entries(blockPath.scope.bindings)) {
        if (seen.has(name)) continue;
        if (binding.scope === blockPath.scope) {
          // Check it's a let/const (block-scoped)
          if (binding.kind === "let" || binding.kind === "const") {
            seen.add(name);
            entries.push({ name, isMinified: looksMinified(name) });
          }
        }
      }
    }
  });

  return entries;
}

function getFnType(node: t.Function): string {
  if (t.isArrowFunctionExpression(node)) return "arrow";
  if (t.isFunctionDeclaration(node)) return "declaration";
  if (t.isFunctionExpression(node)) return "expression";
  return "unknown";
}

function getPreview(node: t.Function, source: string): string {
  const start = node.start;
  const end = node.end;
  if (start == null || end == null) return "<no source info>";
  const slice = source.slice(start, Math.min(start + 120, end));
  return slice.replace(/\n/g, "\\n");
}

const functions: FunctionInfo[] = [];

traverse(ast as t.File, {
  // biome-ignore lint/style/useNamingConvention: Babel visitor
  Function(path: NodePath<t.Function>) {
    const node = path.node;
    const loc = node.loc?.start;
    const location = loc ? `${loc.line}:${loc.column}` : "?:?";
    const bindings = getOwnBindingNames(path);
    const libraryName =
      node.start != null ? regionLookup(node.start) : undefined;

    functions.push({
      location,
      totalBindings: bindings.length,
      minifiedBindings: bindings.filter((b) => b.isMinified).length,
      descriptiveBindings: bindings.filter((b) => !b.isMinified).length,
      bindingNames: bindings.map((b) => b.name),
      libraryName,
      fnType: getFnType(node),
      preview: getPreview(node, code)
    });
  }
});

// ---------------------------------------------------------------------------
// Categorize
// ---------------------------------------------------------------------------

function categorize(fn: FunctionInfo): Category {
  if (fn.totalBindings === 0) return "zero-bindings";
  if (fn.minifiedBindings === 0) return "all-descriptive";
  if (fn.libraryName && fn.minifiedBindings === 0) return "library-no-minified";
  return "has-minified";
}

const categories = new Map<Category, FunctionInfo[]>();
for (const cat of [
  "zero-bindings",
  "all-descriptive",
  "library-no-minified",
  "has-minified"
] as Category[]) {
  categories.set(cat, []);
}

// Also split "has-minified" by library vs non-library
let libraryWithMinified = 0;
let appWithMinified = 0;

for (const fn of functions) {
  const cat = categorize(fn);
  categories.get(cat)!.push(fn);
  if (cat === "has-minified") {
    if (fn.libraryName) libraryWithMinified++;
    else appWithMinified++;
  }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

const total = functions.length;
console.log(`\n${"=".repeat(70)}`);
console.log(`  Function Classification (${total.toLocaleString()} total)`);
console.log(`${"=".repeat(70)}`);

const categoryLabels: Record<Category, string> = {
  "zero-bindings": "Zero own bindings (nothing to rename)",
  "all-descriptive": "All bindings descriptive (already good names)",
  "library-no-minified": "Library, no minified bindings",
  "has-minified": "Has minified bindings (SHOULD be renamed)"
};

for (const [cat, fns] of categories) {
  const pct = total > 0 ? ((fns.length / total) * 100).toFixed(1) : "0.0";
  console.log(
    `  ${categoryLabels[cat].padEnd(50)} ${fns.length.toLocaleString().padStart(8)}  (${pct}%)`
  );
}

if (categories.get("has-minified")!.length > 0) {
  console.log(
    `    - In library region:     ${libraryWithMinified.toLocaleString().padStart(8)}`
  );
  console.log(
    `    - In app code:           ${appWithMinified.toLocaleString().padStart(8)}`
  );
}

// ---------------------------------------------------------------------------
// Samples
// ---------------------------------------------------------------------------

function printSamples(cat: Category, label: string) {
  const fns = categories.get(cat)!;
  if (fns.length === 0) return;

  console.log(
    `\n--- Samples: ${label} (${fns.length.toLocaleString()} total, showing ${Math.min(sampleCount, fns.length)}) ---`
  );

  // Pick evenly spaced samples
  const step = Math.max(1, Math.floor(fns.length / sampleCount));
  for (let i = 0; i < Math.min(sampleCount, fns.length); i++) {
    const fn = fns[i * step];
    console.log(
      `  [${fn.location}] ${fn.fnType} | bindings: ${fn.totalBindings} (${fn.minifiedBindings} minified)`
    );
    if (fn.bindingNames.length > 0) {
      const names = fn.bindingNames.slice(0, 20).join(", ");
      const suffix =
        fn.bindingNames.length > 20
          ? ` ... +${fn.bindingNames.length - 20} more`
          : "";
      console.log(`    names: ${names}${suffix}`);
    }
    if (fn.libraryName) {
      console.log(`    library: ${fn.libraryName}`);
    }
    console.log(`    preview: ${fn.preview}`);
  }
}

printSamples("zero-bindings", "Zero own bindings");
printSamples("all-descriptive", "All bindings descriptive");
printSamples("library-no-minified", "Library, no minified");
printSamples("has-minified", "Has minified bindings (SHOULD be renamed)");

// ---------------------------------------------------------------------------
// Binding length distribution for "all-descriptive"
// ---------------------------------------------------------------------------

const allDescriptive = categories.get("all-descriptive")!;
if (allDescriptive.length > 0) {
  console.log(
    `\n--- Binding name length distribution for "all-descriptive" functions ---`
  );
  const lengths = new Map<number, number>();
  const nameSamples = new Map<number, string[]>();
  for (const fn of allDescriptive) {
    for (const name of fn.bindingNames) {
      const len = name.length;
      lengths.set(len, (lengths.get(len) ?? 0) + 1);
      if (!nameSamples.has(len)) nameSamples.set(len, []);
      const samples = nameSamples.get(len)!;
      if (samples.length < 10 && !samples.includes(name)) samples.push(name);
    }
  }
  const sorted = [...lengths.entries()].sort((a, b) => a[0] - b[0]);
  for (const [len, count] of sorted) {
    const samples = nameSamples.get(len)!.join(", ");
    console.log(
      `  len=${len}: ${count.toLocaleString().padStart(8)} bindings  (e.g. ${samples})`
    );
  }
}

// ---------------------------------------------------------------------------
// "has-minified" sub-analysis: how many minified bindings per function?
// ---------------------------------------------------------------------------

const hasMinified = categories.get("has-minified")!;
if (hasMinified.length > 0) {
  console.log(
    `\n--- Minified binding count distribution for "has-minified" functions ---`
  );
  const dist = new Map<number, number>();
  for (const fn of hasMinified) {
    const n = fn.minifiedBindings;
    dist.set(n, (dist.get(n) ?? 0) + 1);
  }
  const sorted = [...dist.entries()].sort((a, b) => a[0] - b[0]);
  for (const [n, count] of sorted.slice(0, 20)) {
    console.log(
      `  ${n} minified binding(s): ${count.toLocaleString().padStart(8)} functions`
    );
  }
  if (sorted.length > 20) {
    console.log(`  ... +${sorted.length - 20} more buckets`);
  }
}
