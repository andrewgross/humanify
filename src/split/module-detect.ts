/**
 * Module boundary detection for bundled JavaScript.
 *
 * Detects structural patterns left by bundlers to identify where
 * one original module ends and another begins. This is the first
 * step in the split pipeline — detected boundaries become the
 * primary grouping signal.
 *
 * Detection strategies (tried in order):
 *   1. esbuild file-path comments: `// path/to/file.ts`
 *   2. esbuild CJS moduleFactory wrappers: `var x = moduleFactory(...)`
 *   3. Bun CJS factory wrappers: `var x = HELPER(...)` (structurally identified)
 *   4. None detected → fall back to call-graph clustering
 */

import {
  identifyBunCjsFactory,
  identifyBunLazyInit
} from "../detection/bun-helpers.js";

/** A detected module boundary in the bundle source. */
export interface DetectedModule {
  /** Normalized module path or identifier */
  id: string;
  /** Start line (1-indexed) of this module's code */
  startLine: number;
  /** End line (1-indexed, inclusive) of this module's code */
  endLine: number;
}

export type BundlerType = "esbuild-esm" | "esbuild-cjs" | "bun-cjs" | "unknown";

export interface ModuleDetectionResult {
  bundler: BundlerType;
  modules: DetectedModule[];
  /** Lines NOT covered by any detected module (for fallback processing) */
  uncoveredRanges: Array<{ startLine: number; endLine: number }>;
}

// ── esbuild ESM comment detection ───────────────────────────────────

const FILE_COMMENT_RE = /^\/\/ (.+\.[jt]sx?)$/;

function detectEsbuildComments(lines: string[]): DetectedModule[] | null {
  const commentLines: Array<{ path: string; line: number }> = [];

  for (let i = 0; i < lines.length; i++) {
    const match = lines[i].match(FILE_COMMENT_RE);
    if (match) {
      commentLines.push({ path: match[1], line: i + 1 });
    }
  }

  // Need at least 2 comments to consider this an esbuild ESM bundle
  if (commentLines.length < 2) return null;

  const modules: DetectedModule[] = [];
  for (let i = 0; i < commentLines.length; i++) {
    const endLine =
      i < commentLines.length - 1 ? commentLines[i + 1].line - 1 : lines.length;

    modules.push({
      id: normalizeModulePath(commentLines[i].path),
      startLine: commentLines[i].line, // Include the comment line
      endLine
    });
  }

  return modules;
}

// ── esbuild CJS moduleFactory detection ─────────────────────────────

const MODULE_FACTORY_RE = /^var\s+(\w+)\s*=\s*moduleFactory\s*\(/;

/** Find all moduleFactory(...) declarations in the source lines. */
function findFactoryDeclarations(
  lines: string[]
): Array<{ name: string; line: number }> {
  const factories: Array<{ name: string; line: number }> = [];
  for (let i = 0; i < lines.length; i++) {
    const match = lines[i].match(MODULE_FACTORY_RE);
    if (match) {
      factories.push({ name: match[1], line: i + 1 });
    }
  }
  return factories;
}

/** Find the closing line of a moduleFactory call by tracking paren depth. */
function findFactoryEnd(lines: string[], startLine: number): number {
  let depth = 0;
  let foundOpen = false;

  for (let i = startLine - 1; i < lines.length; i++) {
    for (const ch of lines[i]) {
      if (ch === "(") {
        depth++;
        foundOpen = true;
      } else if (ch === ")") {
        depth--;
        if (foundOpen && depth === 0) return i + 1;
      }
    }
  }
  return startLine;
}

function detectModuleFactory(lines: string[]): DetectedModule[] | null {
  const factories = findFactoryDeclarations(lines);
  if (factories.length < 2) return null;

  return factories.map((factory) => ({
    id: factory.name,
    startLine: factory.line,
    endLine: findFactoryEnd(lines, factory.line)
  }));
}

// ── Public API ──────────────────────────────────────────────────────

/**
 * Detect module boundaries in a bundle.
 *
 * Tries multiple detection strategies in order and returns
 * the first successful result.
 */
export function detectModules(source: string): ModuleDetectionResult {
  const lines = source.split("\n");
  const totalLines = lines.length;

  // Strategy 1: esbuild ESM comments
  const esmModules = detectEsbuildComments(lines);
  if (esmModules && esmModules.length >= 2) {
    return {
      bundler: "esbuild-esm",
      modules: esmModules,
      uncoveredRanges: computeUncovered(esmModules, totalLines)
    };
  }

  // Strategy 2: esbuild CJS moduleFactory
  const cjsModules = detectModuleFactory(lines);
  if (cjsModules && cjsModules.length >= 2) {
    return {
      bundler: "esbuild-cjs",
      modules: cjsModules,
      uncoveredRanges: computeUncovered(cjsModules, totalLines)
    };
  }

  // Strategy 3: Bun CJS factory wrappers
  const bunModules = detectBunFactories(source, lines);
  if (bunModules && bunModules.length >= 2) {
    return {
      bundler: "bun-cjs",
      modules: bunModules,
      uncoveredRanges: computeUncovered(bunModules, totalLines)
    };
  }

  // No patterns detected
  return {
    bundler: "unknown",
    modules: [],
    uncoveredRanges: [{ startLine: 1, endLine: totalLines }]
  };
}

/**
 * Assign functions to detected modules based on line position.
 * Returns a map: sessionId → moduleId for functions within detected modules.
 * Functions not in any module are excluded (need fallback processing).
 */
export function assignFunctionsToModules(
  functions: Array<{ sessionId: string; startLine: number }>,
  modules: DetectedModule[]
): Map<string, string> {
  const assignment = new Map<string, string>();

  for (const fn of functions) {
    // Binary search or linear scan for the enclosing module
    for (let i = modules.length - 1; i >= 0; i--) {
      if (
        fn.startLine >= modules[i].startLine &&
        fn.startLine <= modules[i].endLine
      ) {
        assignment.set(fn.sessionId, modules[i].id);
        break;
      }
    }
  }

  return assignment;
}

// ── Bun CJS factory detection ────────────────────────────────────────

function detectBunFactories(
  source: string,
  lines: string[]
): DetectedModule[] | null {
  const factory = identifyBunCjsFactory(source);
  if (!factory) return null;

  const lazyInitName = identifyBunLazyInit(source);
  const helperName = factory.name;

  // Build a global regex to find all `var NAME = HELPER_NAME(` per line
  // (small bundles may have multiple factories on the same line)
  // Use [$\\w]+ for identifiers since $ is valid in JS but not in \w
  const pattern = new RegExp(
    `(?:var|let|const)\\s+([$\\w]+)\\s*=\\s*${escapeRegExp(helperName)}\\s*\\(`,
    "g"
  );

  const modules: DetectedModule[] = [];

  for (let i = 0; i < lines.length; i++) {
    pattern.lastIndex = 0;
    for (
      let match = pattern.exec(lines[i]);
      match !== null;
      match = pattern.exec(lines[i])
    ) {
      const varName = match[1];

      // Skip lazy init helper — it's runtime, not a module
      if (lazyInitName && varName === lazyInitName) continue;

      const startLine = i + 1;
      const endLine = findFactoryEnd(lines, startLine);
      modules.push({ id: varName, startLine, endLine });
    }
  }

  return modules.length >= 2 ? modules : null;
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

// ── Helpers ─────────────────────────────────────────────────────────

export function normalizeModulePath(filePath: string): string {
  const parts = filePath.split("/");

  // Find "src/" and use everything from there
  const srcIdx = parts.indexOf("src");
  if (srcIdx >= 0) return parts.slice(srcIdx).join("/");

  const libIdx = parts.indexOf("lib");
  if (libIdx >= 0) return parts.slice(libIdx).join("/");

  const nmIdx = parts.indexOf("node_modules");
  if (nmIdx >= 0) return parts.slice(nmIdx).join("/");

  return filePath;
}

function computeUncovered(
  modules: DetectedModule[],
  totalLines: number
): Array<{ startLine: number; endLine: number }> {
  const covered = modules
    .map((m) => ({ start: m.startLine, end: m.endLine }))
    .sort((a, b) => a.start - b.start);

  const uncovered: Array<{ startLine: number; endLine: number }> = [];
  let cursor = 1;

  for (const range of covered) {
    if (cursor < range.start) {
      uncovered.push({ startLine: cursor, endLine: range.start - 1 });
    }
    cursor = Math.max(cursor, range.end + 1);
  }

  if (cursor <= totalLines) {
    uncovered.push({ startLine: cursor, endLine: totalLines });
  }

  return uncovered;
}
