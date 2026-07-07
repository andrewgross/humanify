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
import {
  classifyBunModules,
  isInsideFactoryBody,
  nameCjsFactories,
  type BunModuleClassification
} from "../analysis/bun-module-classification.js";
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import { collectEvalWithTaint } from "../analysis/soundness.js";
import type { FunctionNode, RenameReport } from "../analysis/types.js";
import { findWrapperFunction } from "../analysis/wrapper-detection.js";
import { generate, traverse } from "../babel-utils.js";
import { debug } from "../debug.js";
import type { BundlerType, MinifierType } from "../detection/types.js";
import type { CommentRegion } from "../library-detection/comment-regions.js";
import { classifyFunctionsByRegion } from "../library-detection/comment-regions.js";
import type { FileContext } from "../pipeline/types.js";
import {
  captureSemanticBaseline,
  validateOutput
} from "../output-validation.js";
import type { ProcessingMetrics } from "../llm/metrics.js";
import { MetricsTracker } from "../llm/metrics.js";
import type { LLMProvider } from "../llm/types.js";
import type { Profiler } from "../profiling/profiler.js";
import {
  buildCoverageSummary,
  type CoverageSummary,
  formatCoverageSummary
} from "./coverage.js";
import {
  applyPriorVersionIfPresent,
  type TransferStats
} from "./prior-transfer.js";
import type { IsEligibleFn } from "./rename-eligibility.js";
import { type RunConfig, resolveRunConfig } from "./run-config.js";
import {
  LibraryPrefixResolver,
  sanitizeLibraryName
} from "./library-prefix-resolver.js";
import { RenameProcessor } from "./processor.js";

interface ScopeBinding {
  path: babelTraverse.NodePath;
  identifier: t.Identifier;
  referencePaths?: Array<{ node?: { loc?: { start?: { line?: number } } } }>;
}

interface RenamePluginOptions {
  /** The LLM provider to use for name suggestions */
  provider: LLMProvider;

  /** Maximum number of concurrent function processing (default: 50) */
  concurrency?: number;

  /** Callback for progress updates (receives raw metrics) */
  onProgress?: (metrics: ProcessingMetrics) => void;

  /** Generate a source map alongside the output code */
  sourceMap?: boolean;

  /** Maximum number of module binding batches to process in parallel (default: 20) */
  moduleConcurrency?: number;

  /** Maximum identifiers per LLM batch (default: 10) */
  batchSize?: number;

  /** Per-identifier retry limit (default: 3) */
  maxRetriesPerIdentifier?: number;

  /** Cross-lane collision retry limit (default: 100) */
  maxFreeRetries?: number;

  /** Minimum bindings to enable parallel lanes (default: 25) */
  laneThreshold?: number;

  /** Collection window for cross-function retry batching in ms (default: 25) */
  retryBatchWindowMs?: number;

  /** Profiler instance for performance instrumentation */
  profiler?: Profiler;

  /** Detected minifier type — used to select rename-eligibility rules */
  minifierType?: MinifierType;

  /** Detected bundler type — used to select rename-eligibility rules */
  bundlerType?: BundlerType;

  /**
   * When true (default), library functions in mixed files get deterministic
   * prefix renames instead of LLM processing. When false, all functions
   * (including library) go through the LLM.
   */
  skipLibraries?: boolean;

  /** Prior version humanified code for cross-version rename reuse. */
  priorVersionCode?: string;
}

/**
 * Result from the rename plugin, including output code and diagnostic reports.
 */
export interface RenamePluginResult {
  code: string;
  /** The post-rename AST, available for downstream consumers (e.g., split). */
  ast: t.File;
  reports: ReadonlyArray<RenameReport>;
  sourceMap: GeneratorResult["map"];
  coverageSummary?: string;
  coverageData?: CoverageSummary;
  /** Number of functions with renames transferred from prior version. */
  priorVersionApplied?: number;
  /** Number of functions matched but already correctly named. */
  priorVersionAlreadyNamed?: number;
  /** Number of module bindings matched from prior version. */
  priorVersionBindingsApplied?: number;
  /** Per-binding transfer stats for prior-version matching. */
  transferStats?: {
    exactMatch: TransferStats;
    closeMatch: TransferStats;
  };
  /** Summary of Bun CJS third-party classification, when applicable. */
  thirdPartyClassification?: import("./diagnostics.js").ThirdPartyClassificationReport;
  /** Set when the generated output fails to re-parse (invalid rename applied). */
  parseFailure?: import("../output-validation.js").OutputParseFailure;
  /** Set when a rename invariant was violated (capture / binding split). */
  semanticFailure?: import("../output-validation.js").OutputSemanticFailure;
  /**
   * Internal per-function pipeline errors. LLM provider throws are
   * contained and never counted here — a nonzero value is a programming
   * error and the CLI marks the run failed.
   */
  internalErrors: number;
}

// ---------------------------------------------------------------------------
// Internal helpers for createRenamePlugin phases
// ---------------------------------------------------------------------------

/**
 * Re-parse generated output and check the rename invariants against the
 * pre-rename baseline (free-name set, total binding count). One parse
 * serves both checks; a capture or binding split parses cleanly, so the
 * semantic comparison is the only gate that catches it.
 */
function validateGeneratedOutput(
  code: string,
  profiler: Profiler,
  baseline?: import("../output-validation.js").SemanticBaseline
): {
  parseFailure?: import("../output-validation.js").OutputParseFailure;
  semanticFailure?: import("../output-validation.js").OutputSemanticFailure;
} {
  const validateSpan = profiler.startSpan("validate-output", "pipeline");
  const result = validateOutput(code, baseline);
  validateSpan.end({
    valid: !result.parseFailure && !result.semanticFailure
  });
  if (result.parseFailure) {
    debug.log(
      "validate-output",
      `Generated output does not parse: ${result.parseFailure.message}` +
        (result.parseFailure.excerpt ? `\n${result.parseFailure.excerpt}` : "")
    );
  }
  if (result.semanticFailure) {
    debug.log("validate-output", result.semanticFailure.message);
  }
  return result;
}

/**
 * Freeze everything renaming cannot touch soundly: functions on the scope
 * chain of a `with` block or direct `eval` call (their bindings are
 * resolvable by ORIGINAL name at runtime), and — since scope chains end
 * there — module-level bindings whenever any site exists. Frozen nodes
 * are marked done so neither the LLM pass nor prior-version transfer
 * renames them; everything off the scope chains proceeds normally.
 */
function markEvalWithTaintPreDone(
  ast: t.File,
  graph: ReturnType<typeof buildUnifiedGraph>,
  preDone: FunctionNode[]
): void {
  const taint = collectEvalWithTaint(ast);
  if (taint.siteCount === 0) return;

  let frozenFunctions = 0;
  let frozenBindings = 0;
  for (const [, renameNode] of graph.nodes) {
    if (renameNode.type === "function") {
      if (taint.taintedFunctions.has(renameNode.node.path.node)) {
        renameNode.node.status = "done";
        renameNode.node.renameMapping = { names: {} };
        preDone.push(renameNode.node);
        frozenFunctions++;
      }
    } else if (taint.moduleTainted) {
      renameNode.node.status = "done";
      frozenBindings++;
    }
  }
  debug.log(
    "soundness",
    `with/direct-eval at ${taint.siteCount} site(s): froze ${frozenFunctions} ` +
      `function(s) and ${frozenBindings} module binding(s) — renaming them ` +
      `is unsound (runtime name resolution)`
  );
}

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
      if (renameNode.node.status === "done") break; // already frozen
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

/** Mark library functions as pre-done based on comment regions and return them with library names. */
function markLibraryFunctionsPreDone(
  allFunctions: FunctionNode[],
  commentRegions: CommentRegion[] | undefined,
  preDone: FunctionNode[]
): { libraryFunctions: FunctionNode[]; libraryMap: Map<string, string> } {
  const libraryFunctions: FunctionNode[] = [];
  const emptyResult = {
    libraryFunctions,
    libraryMap: new Map<string, string>()
  };
  if (!commentRegions || commentRegions.length === 0) return emptyResult;

  const libraryMap = classifyFunctionsByRegion(allFunctions, commentRegions);
  if (libraryMap.size === 0) return emptyResult;

  for (const fn of allFunctions) {
    if (libraryMap.has(fn.sessionId)) {
      fn.status = "done";
      fn.renameMapping = { names: {} };
      preDone.push(fn);
      libraryFunctions.push(fn);
    }
  }
  debug.log("mixed-file", `Skipping ${libraryMap.size} library functions`);
  return { libraryFunctions, libraryMap };
}

/** Run the main rename pass on the unified graph. */
async function runRenamePass(
  ast: ReturnType<typeof parseSync>,
  graph: ReturnType<typeof buildUnifiedGraph>,
  provider: LLMProvider,
  options: RenamePluginOptions,
  metrics: MetricsTracker,
  preDone: FunctionNode[],
  config: RunConfig
): Promise<{ processor: RenameProcessor; allReports: RenameReport[] }> {
  const { concurrency = 50 } = options;
  const processor = new RenameProcessor(ast as t.File);
  let allReports: RenameReport[] = [];

  if (graph.nodes.size > 0) {
    await processor.processUnified(graph, provider, {
      concurrency,
      moduleConcurrency: options.moduleConcurrency,
      metrics,
      preDone: preDone.length > 0 ? preDone : undefined,
      batchSize: options.batchSize,
      maxRetriesPerIdentifier: options.maxRetriesPerIdentifier,
      maxFreeRetries: options.maxFreeRetries,
      laneThreshold: options.laneThreshold,
      retryBatchWindowMs: options.retryBatchWindowMs,
      profiler: config.profiler,
      isEligible: config.isEligible,
      bundlerType: config.bundlerType
    });
    allReports = [...processor.reports];
  }

  return { processor, allReports };
}

/**
 * Apply library prefix renames to library functions.
 *
 * Instead of sending library functions through the LLM, deterministically
 * rename their bindings by prefixing with the sanitized library name.
 * e.g., react-dom: Xuo -> react_dom_Xuo
 */
interface LibraryPrefixResult {
  reports: RenameReport[];
  /** Count of library functions skipped because they had no minified bindings */
  libraryNoMinified: number;
}

function runLibraryPrefixPass(
  libraryFunctions: FunctionNode[],
  libraryMap: Map<string, string>,
  isEligible: IsEligibleFn,
  existingReports: RenameReport[]
): LibraryPrefixResult {
  if (libraryFunctions.length === 0 || libraryMap.size === 0) {
    return { reports: existingReports, libraryNoMinified: 0 };
  }

  const newReports: RenameReport[] = [];
  let libraryNoMinified = 0;

  for (const fn of libraryFunctions) {
    const libName = libraryMap.get(fn.sessionId);
    if (!libName) continue;

    const prefix = sanitizeLibraryName(libName);
    const resolver = new LibraryPrefixResolver(prefix);
    const scope = fn.path.scope;
    const bindings = Object.entries(scope.bindings).filter(([name]) =>
      isEligible(name)
    );

    if (bindings.length === 0) {
      libraryNoMinified++;
      continue;
    }

    const identifiers = bindings.map(([name]) => name);
    const names = resolver.resolveNames(identifiers);
    const outcomes: Record<
      string,
      import("../analysis/types.js").IdentifierOutcome
    > = {};

    for (const [oldName, newName] of Object.entries(names)) {
      scope.rename(oldName, newName);
      outcomes[oldName] = { status: "renamed", newName, round: 1 };
    }

    fn.renameMapping = { names };
    fn.renameReport = {
      type: "function",
      strategy: "library-prefix",
      targetId: fn.sessionId,
      totalIdentifiers: identifiers.length,
      renamedCount: identifiers.length,
      outcomes
    };
    newReports.push(fn.renameReport);
  }

  debug.log(
    "library-prefix",
    `Applied library prefix to ${newReports.length} functions`
  );

  return { reports: [...existingReports, ...newReports], libraryNoMinified };
}

/** Detect library functions and mark them as pre-done. */
function detectAndMarkLibraries(
  options: RenamePluginOptions,
  graph: ReturnType<typeof buildUnifiedGraph>,
  context: FileContext | undefined,
  allFunctions: FunctionNode[],
  preDone: FunctionNode[]
): { libraryFunctions: FunctionNode[]; libraryMap: Map<string, string> } {
  const skipLibs = options.skipLibraries ?? true;
  const commentRegions =
    !skipLibs || graph.wrapperPath ? undefined : context?.commentRegions;
  return markLibraryFunctionsPreDone(allFunctions, commentRegions, preDone);
}

/**
 * Creates a rename plugin that processes all functions in dependency order
 * using the provided LLM provider.
 *
 * @param options Configuration options for the rename plugin
 * @returns An async function that transforms code and returns reports
 */
export function createRenamePlugin(options: RenamePluginOptions) {
  const { provider, onProgress } = options;
  const config = resolveRunConfig(options);
  const { profiler, isEligible } = config;

  return async (
    code: string,
    context?: FileContext
  ): Promise<RenamePluginResult> => {
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

    // compact: false forces formatted output regardless of input size.
    // Without it, babel auto-compacts files >500KB, which is why prettier
    // used to follow generate() in the pipeline.
    const genOpts: GeneratorOptions = options.sourceMap
      ? { compact: false, sourceMaps: true, sourceFileName: "input.js" }
      : { compact: false };
    const genSource = options.sourceMap ? originalCode : undefined;

    // Step 1: Build unified graph (functions + module-level bindings)
    metrics.setStage("building-graph");
    const graphSpan = profiler.startSpan("graph-build", "pipeline");
    const graph = buildUnifiedGraph(
      ast,
      "input.js",
      profiler,
      isEligible,
      originalCode
    );
    graphSpan.end({ nodeCount: graph.nodes.size });

    // Rename invariants are checked against this after generation; must be
    // captured before any rename mutates the AST or its scope info.
    const semanticBaseline = captureSemanticBaseline(ast);

    const thirdPartyReport = summarizeThirdPartyClassification(
      ast,
      originalCode,
      graph.classification ?? null
    );

    if (graph.nodes.size === 0) {
      const output = generate(ast, genOpts, genSource);
      return {
        code: output.code,
        ast: ast as t.File,
        reports: [],
        sourceMap: output.map,
        thirdPartyClassification: thirdPartyReport,
        internalErrors: 0
      };
    }

    // Collect pre-done nodes (library + wrapper IIFE + soundness-frozen)
    const preDone: FunctionNode[] = [];

    // Freeze scope chains of with/direct-eval sites before any renaming
    markEvalWithTaintPreDone(ast as t.File, graph, preDone);

    // Mark wrapper IIFE as pre-done so its children can process without deadlock
    markWrapperPreDone(graph, preDone);

    // Collect all function nodes for library detection
    const allFunctions = collectAllFunctions(graph);

    // Filter out library functions from mixed files (Layer 3)
    const { libraryFunctions, libraryMap } = detectAndMarkLibraries(
      options,
      graph,
      context,
      allFunctions,
      preDone
    );

    // Apply prior-version matching if provided
    const priorSpan = profiler.startSpan("prior-version", "pipeline");
    const {
      priorVersionApplied,
      priorVersionAlreadyNamed,
      priorVersionBindingsApplied,
      priorVersionCloseMatch,
      transferStats
    } = applyPriorVersionIfPresent(
      options.priorVersionCode,
      allFunctions,
      graph,
      preDone,
      profiler
    );
    priorSpan.end({
      functionsMatched: priorVersionApplied,
      bindingsApplied: priorVersionBindingsApplied,
      closeMatches: priorVersionCloseMatch
    });

    // Pre-done nodes stay in the graph (status "done"); the processor
    // derives its done set from node status, and deleting them would leave
    // dangling dependency edges.

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
      config
    );
    renameSpan.end({ processedCount: renameReports.length });

    // Step 3: Apply library prefix renames (deterministic, no LLM calls)
    metrics.setStage("library-prefix");
    const libPrefixSpan = profiler.startSpan(
      "rename:library-prefix",
      "pipeline"
    );
    const { reports: allReports, libraryNoMinified } = runLibraryPrefixPass(
      libraryFunctions,
      libraryMap,
      isEligible,
      renameReports
    );
    libPrefixSpan.end();

    const totalSkippedBySkipList = processor.skippedBySkipList;
    const coverage = buildCoverageSummary(
      allReports,
      allFunctions.length,
      metrics.getMetrics(),
      totalSkippedBySkipList,
      processor.skipReasons,
      libraryNoMinified,
      priorVersionApplied,
      priorVersionAlreadyNamed,
      priorVersionBindingsApplied,
      priorVersionCloseMatch
    );
    const coverageSummary = formatCoverageSummary(coverage);

    metrics.setStage("generating");
    const generateSpan = profiler.startSpan("generate", "pipeline");
    const output = generate(ast, genOpts, genSource);
    generateSpan.end({ codeLength: output.code.length });

    const { parseFailure, semanticFailure } = validateGeneratedOutput(
      output.code,
      profiler,
      semanticBaseline
    );

    metrics.setStage("done");
    return {
      code: output.code,
      ast: ast as t.File,
      reports: allReports,
      sourceMap: output.map,
      coverageSummary,
      coverageData: coverage,
      priorVersionApplied,
      priorVersionAlreadyNamed,
      priorVersionBindingsApplied,
      transferStats,
      thirdPartyClassification: thirdPartyReport,
      parseFailure,
      semanticFailure,
      internalErrors: processor.failed
    };
  };
}

/**
 * Apply the naming cascade and count skipped bindings/functions for
 * downstream diagnostics. Returns undefined when no Bun CJS classification
 * is present.
 */
function summarizeThirdPartyClassification(
  ast: t.File,
  source: string,
  classification: BunModuleClassification | null
): import("./diagnostics.js").ThirdPartyClassificationReport | undefined {
  if (!classification || classification.factories.length === 0) {
    return undefined;
  }

  const namedBy = nameCjsFactories(classification, source);

  let bindingsSkipped = 0;
  let functionsSkipped = 0;

  traverse(ast, {
    Function(path: babelTraverse.NodePath<t.Function>) {
      if (isInsideFactoryBody(path, classification)) {
        functionsSkipped++;
      }
    },
    VariableDeclarator(path: babelTraverse.NodePath<t.VariableDeclarator>) {
      if (isInsideFactoryBody(path, classification)) {
        bindingsSkipped++;
      }
    }
  });

  return {
    bundler: "bun-cjs",
    factoriesDetected: classification.factories.length,
    bindingsSkipped,
    functionsSkipped,
    namedBy
  };
}

interface ModuleBinding {
  name: string;
  identifier: t.Identifier;
  declaration: string;
}

/**
 * Result of collecting module-level bindings.
 */
interface ModuleLevelBindingsResult {
  bindings: ModuleBinding[];
  /** The scope used for renaming (program scope or wrapper IIFE scope) */
  targetScope: babelTraverse.Scope;
  /** If a wrapper IIFE was detected, the path to it */
  wrapperPath?: babelTraverse.NodePath<t.Function>;
  /** Third-party classification of Bun CJS factories, when applicable. */
  classification?: BunModuleClassification | null;
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
    const params = ((bindingPath.node as t.FunctionDeclaration).params ?? [])
      .map((p: t.Node) => generate(p).code)
      .join(", ");
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
 * Returns true if a binding should be skipped from the module binding pool.
 * Function/class declarations are always skipped because they are processed
 * as FunctionNodes by the function graph (including their declaration name,
 * via collectFunctionNameBinding in processor.ts).
 *
 * Also skips bindings that live inside a classified Bun CJS factory body —
 * those modules are treated as third-party and won't be renamed.
 */
function shouldSkipBinding(
  bindingPath: babelTraverse.NodePath,
  classification: BunModuleClassification | null
): boolean {
  // Skip bindings inside any third-party CJS factory body.
  if (isInsideFactoryBody(bindingPath, classification)) {
    return true;
  }

  // Always skip function/class declarations — they're processed as FunctionNodes
  if (bindingPath.isFunctionDeclaration() || bindingPath.isClassDeclaration()) {
    return true;
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
 *
 * If `source` is provided and the bundle is a Bun CJS bundle, also classifies
 * CJS factory bodies as third-party and skips bindings inside them.
 */
export function getModuleLevelBindings(
  ast: t.File,
  isEligible: IsEligibleFn,
  source?: string
): ModuleLevelBindingsResult | null {
  let programScope: babelTraverse.Scope | null = null;
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
  const classification = source
    ? classifyBunModules(ast, source, wrapper)
    : null;

  for (const [name, binding] of Object.entries(targetScope.bindings) as [
    string,
    ScopeBinding
  ][]) {
    if (!isEligible(name)) continue;

    const bindingPath = binding.path;

    if (shouldSkipBinding(bindingPath, classification)) continue;

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
    wrapperPath: wrapper?.functionPath,
    classification
  };
}

/** Maximum number of usage/assignment context snippets per identifier */
const MAX_CONTEXT_SNIPPETS = 10;
/** Maximum character length for a single context snippet */
const MAX_SNIPPET_CHARS = 800;
/** Maximum lines to take from a single statement */
const MAX_SNIPPET_LINES = 10;

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
