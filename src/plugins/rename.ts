/**
 * Unified rename plugin that works with any LLM provider.
 *
 * This replaces the legacy per-provider plugins (openaiRename, geminiRename, localRename)
 * with a single implementation that uses the RenameProcessor for parallel,
 * dependency-ordered function processing.
 */

import { parseSync } from "@babel/core";
import type { GeneratorOptions, GeneratorResult } from "@babel/generator";
import type * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import type { FunctionNode, FunctionRenameReport } from "../analysis/types.js";
import { generate, traverse } from "../babel-utils.js";
import { debug } from "../debug.js";
import type { MinifierType } from "../detection/types.js";
import type { CommentRegion } from "../library-detection/comment-regions.js";
import { classifyFunctionsByRegion } from "../library-detection/comment-regions.js";
import type { ProcessingMetrics } from "../llm/metrics.js";
import { MetricsTracker } from "../llm/metrics.js";
import type { LLMProvider } from "../llm/types.js";
import type { Profiler } from "../profiling/profiler.js";
import { NULL_PROFILER } from "../profiling/profiler.js";
import {
  buildCoverageSummary,
  type CoverageSummary,
  formatCoverageSummary
} from "../rename/coverage.js";
import type { LooksMinifiedFn } from "../rename/minified-heuristic.js";
import {
  createLooksMinified,
  looksMinified as defaultLooksMinified
} from "../rename/minified-heuristic.js";
import { RenameProcessor } from "../rename/processor.js";

interface RenamePluginOptions {
  /** The LLM provider to use for name suggestions */
  provider: LLMProvider;

  /** Maximum number of concurrent function processing (default: 50) */
  concurrency?: number;

  /** Callback for progress updates (receives raw metrics) */
  onProgress?: (metrics: ProcessingMetrics) => void;

  /** Generate a source map alongside the output code */
  sourceMap?: boolean;

  /**
   * Comment regions for mixed-file detection (Rollup/esbuild bundles).
   * When set, functions inside these regions are classified as library code
   * and skipped during processing. Read fresh each invocation so callers
   * can update it per-file.
   */
  commentRegions?: CommentRegion[];

  /** Maximum identifiers per LLM batch (default: 10) */
  batchSize?: number;

  /** Per-identifier retry limit (default: 3) */
  maxRetriesPerIdentifier?: number;

  /** Cross-lane collision retry limit (default: 100) */
  maxFreeRetries?: number;

  /** Minimum bindings to enable parallel lanes (default: 25) */
  laneThreshold?: number;

  /** Profiler instance for performance instrumentation */
  profiler?: Profiler;

  /** Detected minifier type — used to select a minifier-specific looksMinified heuristic */
  minifierType?: MinifierType;
}

/**
 * Result from the rename plugin, including output code and diagnostic reports.
 */
export interface RenamePluginResult {
  code: string;
  reports: ReadonlyArray<FunctionRenameReport>;
  sourceMap: GeneratorResult["map"];
  coverageSummary?: string;
  coverageData?: CoverageSummary;
}

// ---------------------------------------------------------------------------
// Internal helpers for createRenamePlugin phases
// ---------------------------------------------------------------------------

/** Mark the wrapper IIFE node as pre-done and return it if found. */
function markWrapperPreDone(
  graph: ReturnType<typeof buildUnifiedGraph>,
  preDone: FunctionNode[]
): void {
  if (!graph.wrapperPath) return;
  const wrapperNode = graph.wrapperPath.node;
  for (const [, renameNode] of graph.nodes) {
    if (
      renameNode.type === "function" &&
      renameNode.node.path.node === wrapperNode
    ) {
      renameNode.node.status = "done";
      renameNode.node.renameMapping = { names: {} };
      preDone.push(renameNode.node);
      debug.log(
        "wrapper",
        `Marked wrapper function ${renameNode.node.sessionId} as pre-done`
      );
      break;
    }
  }
}

/** Collect all FunctionNode entries from the graph. */
function collectAllFunctions(
  graph: ReturnType<typeof buildUnifiedGraph>
): FunctionNode[] {
  const result: FunctionNode[] = [];
  for (const [, renameNode] of graph.nodes) {
    if (renameNode.type === "function") {
      result.push(renameNode.node);
    }
  }
  return result;
}

/** Mark library functions as pre-done based on comment regions and return them. */
function markLibraryFunctionsPreDone(
  allFunctions: FunctionNode[],
  commentRegions: CommentRegion[] | undefined,
  preDone: FunctionNode[]
): FunctionNode[] {
  const libraryFunctions: FunctionNode[] = [];
  if (!commentRegions || commentRegions.length === 0) return libraryFunctions;

  const libraryIds = classifyFunctionsByRegion(allFunctions, commentRegions);
  if (libraryIds.size === 0) return libraryFunctions;

  for (const fn of allFunctions) {
    if (libraryIds.has(fn.sessionId)) {
      fn.status = "done";
      fn.renameMapping = { names: {} };
      preDone.push(fn);
      libraryFunctions.push(fn);
    }
  }
  debug.log("mixed-file", `Skipping ${libraryIds.size} library functions`);
  return libraryFunctions;
}

/** Run the main rename pass on the unified graph. */
async function runRenamePass(
  ast: ReturnType<typeof parseSync>,
  graph: ReturnType<typeof buildUnifiedGraph>,
  provider: LLMProvider,
  options: RenamePluginOptions,
  metrics: MetricsTracker,
  preDone: FunctionNode[],
  profiler: Profiler,
  looksMinified: LooksMinifiedFn
): Promise<{ processor: RenameProcessor; allReports: FunctionRenameReport[] }> {
  const { concurrency = 50 } = options;
  const processor = new RenameProcessor(ast as t.File);
  let allReports: FunctionRenameReport[] = [];

  if (graph.nodes.size > 0) {
    await processor.processUnified(graph, provider, {
      concurrency,
      metrics,
      preDone: preDone.length > 0 ? preDone : undefined,
      batchSize: options.batchSize,
      maxRetriesPerIdentifier: options.maxRetriesPerIdentifier,
      maxFreeRetries: options.maxFreeRetries,
      laneThreshold: options.laneThreshold,
      profiler,
      looksMinified
    });
    allReports = [...processor.reports];
  }

  return { processor, allReports };
}

/** Run the library-params rename pass and append any new reports. */
async function runLibraryParamPass(
  ast: ReturnType<typeof parseSync>,
  libraryFunctions: FunctionNode[],
  provider: LLMProvider,
  concurrency: number,
  metrics: MetricsTracker,
  looksMinified: LooksMinifiedFn,
  existingReports: FunctionRenameReport[]
): Promise<FunctionRenameReport[]> {
  if (libraryFunctions.length === 0 || !provider.suggestAllNames) {
    return existingReports;
  }

  const libraryWithMinifiedParams = libraryFunctions.filter((fn) =>
    hasMinifiedParam(fn, looksMinified)
  );

  if (libraryWithMinifiedParams.length === 0) return existingReports;

  debug.log(
    "library-params",
    `Step 3: processing params for ${libraryWithMinifiedParams.length} library functions`
  );

  for (const fn of libraryWithMinifiedParams) {
    fn.status = "pending";
  }

  const paramProcessor = new RenameProcessor(ast as t.File);
  await paramProcessor.processAll(libraryWithMinifiedParams, provider, {
    concurrency,
    metrics,
    paramOnly: true,
    looksMinified
  });

  return [...existingReports, ...paramProcessor.reports];
}

/**
 * Creates a rename plugin that processes all functions in dependency order
 * using the provided LLM provider.
 *
 * @param options Configuration options for the rename plugin
 * @returns An async function that transforms code and returns reports
 */
export function createRenamePlugin(options: RenamePluginOptions) {
  const { provider, concurrency = 50, onProgress } = options;
  const profiler = options.profiler ?? NULL_PROFILER;
  const looksMinified: LooksMinifiedFn = createLooksMinified(
    options.minifierType
  );

  return async (code: string): Promise<RenamePluginResult> => {
    const originalCode = code;

    const parseSpan = profiler.startSpan("parse", "pipeline");
    const ast = parseSync(code, {
      sourceType: "unambiguous"
    });
    parseSpan.end({ codeLength: code.length });

    if (!ast) {
      throw new Error("Failed to parse code");
    }

    const metrics = new MetricsTracker({
      onMetrics: (m) => onProgress?.(m)
    });

    const genOpts: GeneratorOptions = options.sourceMap
      ? { sourceMaps: true, sourceFileName: "input.js" }
      : {};
    const genSource = options.sourceMap ? originalCode : undefined;

    // Step 1: Build unified graph (functions + module-level bindings)
    metrics.setStage("building-graph");
    const graphSpan = profiler.startSpan("graph-build", "pipeline");
    const graph = buildUnifiedGraph(ast, "input.js", profiler, looksMinified);
    graphSpan.end({ nodeCount: graph.nodes.size });

    if (graph.nodes.size === 0) {
      const output = generate(ast, genOpts, genSource);
      return { code: output.code, reports: [], sourceMap: output.map };
    }

    // Collect pre-done nodes (library + wrapper IIFE)
    const preDone: FunctionNode[] = [];

    // Mark wrapper IIFE as pre-done so its children can process without deadlock
    markWrapperPreDone(graph, preDone);

    // Collect all function nodes for library detection
    const allFunctions = collectAllFunctions(graph);

    // Filter out library functions from mixed files (Layer 3)
    // ONLY use comment regions when there is NO wrapper
    const commentRegions = graph.wrapperPath
      ? undefined
      : options.commentRegions;
    const libraryFunctions = markLibraryFunctionsPreDone(
      allFunctions,
      commentRegions,
      preDone
    );

    // Remove pre-done function nodes from the graph's active set
    // (they'll be in preDone for dependency tracking but won't be processed)
    for (const fn of preDone) {
      graph.nodes.delete(fn.sessionId);
    }

    // Step 2: Process unified graph in a single parallel pass
    metrics.setStage("renaming");
    const renameSpan = profiler.startSpan("rename:functions", "pipeline");
    const { processor, allReports: renameReports } = await runRenamePass(
      ast,
      graph,
      provider,
      options,
      metrics,
      preDone,
      profiler,
      looksMinified
    );
    renameSpan.end({ processedCount: renameReports.length });

    // Step 3: Rename library function parameters (lightweight param-only mode)
    metrics.setStage("library-params");
    const libParamSpan = profiler.startSpan(
      "rename:library-params",
      "pipeline"
    );
    const allReports = await runLibraryParamPass(
      ast,
      libraryFunctions,
      provider,
      concurrency,
      metrics,
      looksMinified,
      renameReports
    );
    libParamSpan.end();

    // Count module bindings for coverage
    const mbReportCount = allReports.filter((r) =>
      r.functionId.startsWith("module-binding-batch:")
    ).length;
    const totalSkippedByHeuristic = processor.skippedByHeuristic;
    const coverage = buildCoverageSummary(
      allReports,
      allFunctions.length,
      mbReportCount,
      metrics.getMetrics(),
      totalSkippedByHeuristic,
      libraryFunctions.length
    );
    const coverageSummary = formatCoverageSummary(coverage);

    metrics.setStage("generating");
    const generateSpan = profiler.startSpan("generate", "pipeline");
    const output = generate(ast, genOpts, genSource);
    generateSpan.end({ codeLength: output.code.length });
    metrics.setStage("done");
    return {
      code: output.code,
      reports: allReports,
      sourceMap: output.map,
      coverageSummary,
      coverageData: coverage
    };
  };
}

/** Minimum number of bindings for an IIFE to be considered a wrapper */
const WRAPPER_IIFE_BINDING_THRESHOLD = 50;

/** Maximum identifiers per batch for module-level renaming */
const _MODULE_BATCH_SIZE = 5;

interface ModuleBinding {
  name: string;
  identifier: t.Identifier;
  declaration: string;
}

/**
 * Result of wrapper function detection.
 */
interface WrapperFunctionResult {
  /** The scope of the wrapper function (replaces programScope for bindings) */
  scope: any;
  /** The path to the wrapper function (for marking as pre-done) */
  functionPath: babelTraverse.NodePath<t.Function>;
}

/**
 * Extract the callee function from a CallExpression node, or return null.
 * Handles: direct IIFE, .call/.apply IIFE.
 */
function extractCalleeFromCall(expr: t.CallExpression): t.Expression | null {
  const fn = expr.callee;

  // (function(){...})() or (() => {...})()
  if (t.isFunctionExpression(fn) || t.isArrowFunctionExpression(fn)) {
    return fn;
  }

  // (function(){}).call(this, ...) or .apply(...)
  if (
    t.isMemberExpression(fn) &&
    t.isIdentifier(fn.property) &&
    (fn.property.name === "call" || fn.property.name === "apply") &&
    (t.isFunctionExpression(fn.object) ||
      t.isArrowFunctionExpression(fn.object))
  ) {
    return fn.object;
  }

  return null;
}

/**
 * Detects a giant wrapper function pattern where the entire program body
 * is a single expression statement containing a function.
 *
 * Handles:
 * - (function(exports, require, module) { ... })()           — IIFE
 * - !function() { ... }()                                     — negated IIFE
 * - (function(){}).call(this, ...)                             — .call/.apply
 * - (() => { ... })()                                         — arrow IIFE
 * - (function(exports, require, module) { ... });             — Bun CJS bytecode (bare, not called)
 *
 * Only triggers if the wrapper has more bindings than WRAPPER_IIFE_BINDING_THRESHOLD,
 * to avoid interfering with small per-module IIFEs (Webpack style).
 */
function findWrapperFunction(ast: t.File): WrapperFunctionResult | null {
  const body = ast.program.body;

  // Must be a single expression statement
  if (body.length !== 1 || !t.isExpressionStatement(body[0])) return null;

  const expr = body[0].expression;
  let callee: t.Expression | null = null;

  if (t.isCallExpression(expr)) {
    callee = extractCalleeFromCall(expr);
  }

  // !function(){...}()
  if (
    !callee &&
    t.isUnaryExpression(expr) &&
    t.isCallExpression(expr.argument)
  ) {
    const fn = expr.argument.callee;
    if (t.isFunctionExpression(fn) || t.isArrowFunctionExpression(fn)) {
      callee = fn;
    }
  }

  // Bun CJS bytecode: (function(exports, require, module) { ... });
  // A bare function expression (not called) wrapping the entire bundle
  if (!callee && t.isFunctionExpression(expr)) {
    callee = expr;
  }

  if (!callee) return null;

  // Now traverse to find the actual path and check binding count
  let result: WrapperFunctionResult | null = null;

  traverse(ast, {
    Function(path: babelTraverse.NodePath<t.Function>) {
      if (path.node === callee) {
        const bindingCount = Object.keys(path.scope.bindings).length;
        if (bindingCount >= WRAPPER_IIFE_BINDING_THRESHOLD) {
          result = { scope: path.scope, functionPath: path };
          debug.log(
            "wrapper",
            `Detected wrapper function with ${bindingCount} bindings`
          );
        }
        path.stop();
      }
    }
  });

  return result;
}

/**
 * Result of collecting module-level bindings.
 */
interface ModuleLevelBindingsResult {
  bindings: ModuleBinding[];
  /** The scope used for renaming (program scope or wrapper IIFE scope) */
  targetScope: any;
  /** If a wrapper IIFE was detected, the path to it */
  wrapperPath?: babelTraverse.NodePath<t.Function>;
}

/**
 * Returns the declaration text for a function/class declaration binding path.
 */
function getFunctionOrClassDeclarationText(
  name: string,
  bindingPath: babelTraverse.NodePath
): string {
  try {
    const fullCode = generate(bindingPath.node).code;
    const lines = fullCode.split("\n");
    if (lines.length > 10) {
      return `${lines.slice(0, 10).join("\n")}\n  // ...`;
    }
    return fullCode;
  } catch {
    const params =
      (bindingPath.node as any).params
        ?.map((p: any) => generate(p).code)
        .join(", ") ?? "";
    return `function ${name}(${params}) { ... }`;
  }
}

/**
 * Returns the declaration text for a variable declarator binding path.
 */
function getVariableDeclaratorText(
  bindingPath: babelTraverse.NodePath
): string {
  const declPath = bindingPath.parentPath;
  if (declPath) {
    return generate(declPath.node).code;
  }
  return "";
}

/**
 * Returns the declaration text for an import specifier binding path.
 */
function getImportSpecifierText(bindingPath: babelTraverse.NodePath): string {
  const importPath = bindingPath.parentPath;
  if (importPath) {
    return generate(importPath.node).code;
  }
  return "";
}

/**
 * Derives a human-readable declaration string for a binding path.
 */
function getDeclarationText(
  name: string,
  bindingPath: babelTraverse.NodePath
): string {
  if (bindingPath.isFunctionDeclaration() || bindingPath.isClassDeclaration()) {
    return getFunctionOrClassDeclarationText(name, bindingPath);
  }
  if (bindingPath.isVariableDeclarator()) {
    return getVariableDeclaratorText(bindingPath);
  }
  if (
    bindingPath.isImportSpecifier() ||
    bindingPath.isImportDefaultSpecifier() ||
    bindingPath.isImportNamespaceSpecifier()
  ) {
    return getImportSpecifierText(bindingPath);
  }
  return generate(bindingPath.node).code;
}

/**
 * Returns true if a binding should be skipped (function/class declarations
 * when NOT in wrapper mode, or named function/class expressions stored in variables).
 */
function shouldSkipBinding(
  bindingPath: babelTraverse.NodePath,
  wrapper: WrapperFunctionResult | null
): boolean {
  // Skip function/class declarations when NOT in wrapper mode
  if (!wrapper) {
    if (
      bindingPath.isFunctionDeclaration() ||
      bindingPath.isClassDeclaration()
    ) {
      return true;
    }
  }

  // For variable declarators, skip if init is a NAMED function/class expression
  if (bindingPath.isVariableDeclarator()) {
    const init = (bindingPath.node as t.VariableDeclarator).init;
    if (
      (t.isFunctionExpression(init) && init.id) ||
      (t.isClassExpression(init) && init.id)
    ) {
      return true;
    }
  }

  return false;
}

/**
 * Collects module-level bindings that look minified and aren't functions/classes.
 * When a giant wrapper IIFE is detected, uses the wrapper's scope instead of programScope.
 */
export function getModuleLevelBindings(
  ast: t.File,
  looksMinifiedOverride?: LooksMinifiedFn
): ModuleLevelBindingsResult | null {
  let programScope: any = null;
  const bindings: ModuleBinding[] = [];

  traverse(ast, {
    Program(path: babelTraverse.NodePath<t.Program>) {
      programScope = path.scope;
      path.stop();
    }
  });

  if (!programScope) return null;

  // Check for wrapper function — use its scope instead of programScope when detected
  const wrapper = findWrapperFunction(ast);
  const targetScope = wrapper ? wrapper.scope : programScope;

  const isMinified = looksMinifiedOverride ?? defaultLooksMinified;
  for (const [name, binding] of Object.entries(targetScope.bindings) as [
    string,
    any
  ][]) {
    if (!isMinified(name)) continue;

    const bindingPath = binding.path;

    if (shouldSkipBinding(bindingPath, wrapper)) continue;

    const declaration = getDeclarationText(name, bindingPath);

    bindings.push({
      name,
      identifier: binding.identifier,
      declaration
    });
  }

  if (bindings.length === 0) return null;

  return {
    bindings,
    targetScope,
    wrapperPath: wrapper?.functionPath
  };
}

/** Maximum number of usage/assignment context snippets per identifier */
const MAX_CONTEXT_SNIPPETS = 10;
/** Maximum character length for a single context snippet */
const MAX_SNIPPET_CHARS = 800;
/** Maximum lines to take from a single statement */
const MAX_SNIPPET_LINES = 10;

/** Well-known names that should always appear in usedNames regardless of proximity */
const WELL_KNOWN_NAMES = new Set([
  "exports",
  "require",
  "module",
  "__filename",
  "__dirname",
  "console",
  "process",
  "Buffer",
  "Promise",
  "Object",
  "Array",
  "Map",
  "Set",
  "Error",
  "JSON",
  "Math",
  "undefined",
  "null",
  "NaN",
  "Infinity",
  "setTimeout",
  "setInterval",
  "clearTimeout",
  "clearInterval",
  "parseInt",
  "parseFloat",
  "isNaN",
  "isFinite",
  "encodeURI",
  "decodeURI",
  "encodeURIComponent",
  "decodeURIComponent"
]);

/** Minimum scope bindings before activating proximity windowing */
const WINDOWING_THRESHOLD = 100;

/** Line proximity radius for usedNames windowing */
const PROXIMITY_RADIUS = 100;

/**
 * Returns true if a name is within the proximity window given the binding info.
 */
function isNameInProximityWindow(
  _name: string,
  binding: any,
  minLine: number,
  maxLine: number,
  alreadyIncluded: boolean
): boolean {
  if (alreadyIncluded) return false;
  if (!binding) return true; // include if binding not found, to be safe

  const declLine = binding.identifier?.loc?.start?.line;
  if (declLine !== undefined && declLine >= minLine && declLine <= maxLine) {
    return true;
  }

  if (binding.referencePaths) {
    for (const refPath of binding.referencePaths) {
      const refLine = refPath.node?.loc?.start?.line;
      if (refLine !== undefined && refLine >= minLine && refLine <= maxLine) {
        return true;
      }
    }
  }

  return false;
}

/**
 * Computes a proximity-windowed subset of usedNames for module-level prompts.
 *
 * When the scope has >= WINDOWING_THRESHOLD bindings, only returns names whose
 * own declarations/references fall within +-PROXIMITY_RADIUS lines of the batch's
 * relevant lines. Always includes well-known names and excludes minified-looking names.
 *
 * When the scope has fewer bindings, returns all non-minified names.
 */
export function getProximateUsedNames(
  allUsedNames: Set<string>,
  batchLines: number[],
  scopeBindings: Record<string, any>,
  totalBindings: number,
  looksMinifiedOverride?: LooksMinifiedFn
): Set<string> {
  const result = new Set<string>();

  // Always include well-known names that are in scope
  for (const name of allUsedNames) {
    if (WELL_KNOWN_NAMES.has(name)) {
      result.add(name);
    }
  }

  // Filter out minified-looking names in all cases
  const isMinified = looksMinifiedOverride ?? defaultLooksMinified;
  const nonMinified = [...allUsedNames].filter((n) => !isMinified(n));

  // If below threshold, return all non-minified names
  if (totalBindings < WINDOWING_THRESHOLD) {
    for (const name of nonMinified) {
      result.add(name);
    }
    return result;
  }

  // Compute the proximity window
  const minLine = Math.min(...batchLines) - PROXIMITY_RADIUS;
  const maxLine = Math.max(...batchLines) + PROXIMITY_RADIUS;

  for (const name of nonMinified) {
    if (
      isNameInProximityWindow(
        name,
        scopeBindings[name],
        minLine,
        maxLine,
        result.has(name)
      )
    ) {
      result.add(name);
    }
  }

  return result;
}

/**
 * Truncates generated code to up to MAX_SNIPPET_LINES lines and MAX_SNIPPET_CHARS chars.
 * Returns null if the result is empty.
 */
function truncateSnippet(code: string): string | null {
  const lines = code.split("\n").slice(0, MAX_SNIPPET_LINES);
  let snippet = lines.join("\n").trim();
  if (snippet.length > MAX_SNIPPET_CHARS) {
    snippet = `${snippet.slice(0, MAX_SNIPPET_CHARS)}…`;
  }
  return snippet || null;
}

/**
 * Extracts the identifier name from the LHS of an assignment expression, or null.
 * Handles: direct assignment (x = ...), property (x.foo = ...), prototype (x.prototype.m = ...).
 */
function extractAssignmentTargetName(
  left: t.LVal | t.OptionalMemberExpression,
  identifiers: Set<string>
): string | null {
  // Direct assignment: x = ...
  if (t.isIdentifier(left) && identifiers.has(left.name)) {
    return left.name;
  }

  // Property / prototype assignment: x.foo = ... or x.prototype.m = ...
  if (t.isMemberExpression(left)) {
    const obj = left.object;
    // x.prototype.method — drill through one level
    if (
      t.isMemberExpression(obj) &&
      t.isIdentifier(obj.object) &&
      identifiers.has(obj.object.name)
    ) {
      return obj.object.name;
    }
    if (t.isIdentifier(obj) && identifiers.has(obj.name)) {
      return obj.name;
    }
  }

  return null;
}

/**
 * Collects assignment context for module-level identifiers.
 * Finds direct assignments (Cj = ...), property assignments (Cj.create = ...),
 * and prototype assignments (Cj.prototype.method = ...).
 */
export function collectAssignmentContext(
  ast: t.File,
  identifiers: Set<string>
): Record<string, string[]> {
  const assignments: Record<string, string[]> = {};
  for (const id of identifiers) {
    assignments[id] = [];
  }

  traverse(ast, {
    AssignmentExpression(path: babelTraverse.NodePath<t.AssignmentExpression>) {
      const name = extractAssignmentTargetName(path.node.left, identifiers);

      if (!name) return;
      if (assignments[name].length >= MAX_CONTEXT_SNIPPETS) return;

      // Get the containing expression statement for cleaner output
      const statement = path.findParent((p: babelTraverse.NodePath) =>
        p.isStatement()
      );
      const node = statement ? statement.node : path.node;

      try {
        const code = generate(node).code;
        const snippet = truncateSnippet(code);
        if (snippet && !assignments[name].includes(snippet)) {
          assignments[name].push(snippet);
        }
      } catch {
        // Skip if generation fails
      }
    }
  });

  return assignments;
}

/**
 * Returns true if any parameter of the function looks minified.
 */
function hasMinifiedParam(
  fn: FunctionNode,
  looksMinified: LooksMinifiedFn
): boolean {
  const params = fn.path.node.params;
  return params.some((p: any) => {
    if (t.isIdentifier(p)) return looksMinified(p.name);
    if (t.isAssignmentPattern(p) && t.isIdentifier(p.left))
      return looksMinified(p.left.name);
    if (t.isRestElement(p) && t.isIdentifier(p.argument))
      return looksMinified(p.argument.name);
    return false;
  });
}

/**
 * Returns the containing statement/declaration for an Identifier path, for usage context.
 * Returns null if this identifier should be skipped (declaration, assignment LHS, etc.).
 */
function getIdentifierUsageStatement(
  path: babelTraverse.NodePath<t.Identifier>,
  name: string,
  identifiers: Set<string>,
  assignmentCounts: Record<string, number>,
  examples: Record<string, string[]>
): babelTraverse.NodePath | null {
  if (!identifiers.has(name)) return null;

  // Cap total context: assignments + usages ≤ MAX_CONTEXT_SNIPPETS
  const remaining = MAX_CONTEXT_SNIPPETS - (assignmentCounts[name] || 0);
  if (examples[name].length >= remaining) return null;

  // Skip the declaration itself
  if (path.isBindingIdentifier()) return null;

  // Cast needed: after isBindingIdentifier() narrows to `never`, TS loses parent/findParent
  const p = path as babelTraverse.NodePath<t.Identifier>;

  // Skip if this is an assignment LHS (already captured by collectAssignmentContext)
  const parent = p.parent;
  if (t.isAssignmentExpression(parent) && parent.left === p.node) return null;

  return p.findParent(
    (pp: babelTraverse.NodePath) => pp.isStatement() || pp.isDeclaration()
  );
}

/**
 * Collects usage examples for module-level identifiers (up to MAX_CONTEXT_SNIPPETS per identifier).
 */
export function collectUsageExamples(
  ast: t.File,
  identifiers: Set<string>,
  assignmentCounts: Record<string, number>
): Record<string, string[]> {
  const examples: Record<string, string[]> = {};
  for (const id of identifiers) {
    examples[id] = [];
  }

  traverse(ast, {
    Identifier(path: babelTraverse.NodePath<t.Identifier>) {
      const name = path.node.name;
      const statement = getIdentifierUsageStatement(
        path,
        name,
        identifiers,
        assignmentCounts,
        examples
      );
      if (!statement) return;

      try {
        const code = generate(statement.node).code;
        if (code) {
          const snippet = truncateSnippet(code);
          if (snippet && !examples[name].includes(snippet)) {
            examples[name].push(snippet);
          }
        }
      } catch {
        // Skip if generation fails for this node
      }
    }
  });

  return examples;
}
