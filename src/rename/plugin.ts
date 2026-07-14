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
import {
  applyRenameLedger,
  buildRenameLedger,
  type RenameLedger
} from "./rename-ledger.js";
import { debug } from "../debug.js";
import type { BundlerType, MinifierType } from "../detection/types.js";
import type { CommentRegion } from "../library-detection/comment-regions.js";
import { classifyFunctionsByRegion } from "../library-detection/comment-regions.js";
import type { FileContext } from "../pipeline/types.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant,
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
import { deriveExpressionInnerNames } from "./class-id-floor.js";
import { sweepMintedNames } from "./coverage-sweep.js";
import { retryDecoratedNames } from "./decoration-retry.js";
import { isPending, isSettled, markSkipped } from "./lifecycle.js";
import { collectMintedBindings, summarizeCensus } from "./minted-census.js";
import {
  applyPriorVersionIfPresent,
  type TransferStats
} from "./prior-transfer.js";
import { runPriorDiffReconciliation } from "./reconcile-step.js";
import { type DeferredSweepOutcome, runDeferredSweep } from "./sweep-step.js";
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

  /**
   * After generation, diff the output against priorVersionCode and snap
   * rename-noise bindings back to the prior names (diff-reconcile pass).
   * Requires priorVersionCode; skipped when sourceMap is requested.
   */
  reconcilePriorDiff?: boolean;

  /**
   * Close minted-token coverage gaps before generation with the
   * DETERMINISTIC floor passes (class/function-expression inner-id
   * derivation + decoration retry). Cross-version stable, so it reduces
   * both minified leftovers and lineage diff noise. Off by default; the
   * end-of-run census reports leftovers regardless.
   */
  namingFloor?: boolean;

  /**
   * Additionally run the LLM coverage sweep over the minted survivors the
   * deterministic floor leaves (params, decls, var/let). Requires
   * namingFloor. PRIOR-AWARE when combined with priorVersionCode +
   * reconcilePriorDiff: the sweep then defers until after the reconcile
   * pass, which transfers the prior version's name onto every target with
   * a positional counterpart (deterministic, cross-version stable), and
   * the LLM names only the residue — so the sweep closes coverage gaps
   * without re-adding cross-leg naming noise (exp022). Without a prior it
   * runs pre-generate and names every target fresh.
   */
  namingFloorSweep?: boolean;

  /**
   * Build a replayable rename ledger (every rename keyed by byte position,
   * plus the beautified source snapshot it indexes into). Reproduces the
   * final shipped output — including the post-generate reconcile and
   * deferred-sweep passes, captured as chained `post` stages.
   */
  emitRenameLedger?: boolean;
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
   * Result of the flag-gated prior-diff reconciliation pass: how many
   * bindings were snapped/skipped, and the applied (new → prior) name
   * pairs. Present only when the pass ran and replaced the output.
   * Note: `reports`/`coverageData` reflect the PRE-reconcile LLM names;
   * consumers that need the shipped names must apply these pairs.
   */
  priorDiffReconciled?: {
    renames: number;
    skipped: number;
    pairs: import("./reconcile-step.js").AppliedRename[];
  };
  /** Naming-floor stats (minted-token coverage: derive + undecorate + sweep). */
  namingFloor?: {
    derived: number;
    undecorated: number;
    swept: number;
    skipped: number;
  };
  /**
   * Replayable rename ledger + the beautified source snapshot its byte
   * offsets index into. Present only when `emitRenameLedger` is set;
   * `applyRenameLedger(source, ledger)` reproduces the final shipped output
   * (LLM renames plus any reconcile / deferred-sweep `post` stages).
   */
  renameLedger?: { ledger: RenameLedger; source: string };
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

interface PriorDiffStepResult {
  stats: { renames: number; skipped: number };
  renames: import("./reconcile-step.js").AppliedRename[];
  /** Replacement output — present only when renames were actually applied. */
  code?: string;
  ast?: t.File;
}

interface NamingFloorResult {
  /** Class/function-expression inner ids named by derivation. */
  derived: number;
  /** Decorated names restored to their undecorated stem. */
  undecorated: number;
  /** Minted survivors force-named by the LLM coverage sweep. */
  swept: number;
  skipped: number;
}

interface NamingFloorDeps {
  isEligible: IsEligibleFn;
  profiler: Profiler;
  provider: LLMProvider;
  concurrency: number;
}

/**
 * True when the LLM coverage sweep must DEFER until after the prior-diff
 * reconciliation. With a prior present, the reconcile pass's asymmetric
 * tier transfers the prior version's name onto every minted sweep target
 * with a clean positional counterpart — deterministic and cross-version
 * stable — so the LLM should only ever name the residue. Sweeping before
 * generate would replace those transfers with per-leg fresh names,
 * re-creating the cross-version noise the floor exists to remove (exp022).
 */
function isSweepDeferred(options: RenamePluginOptions): boolean {
  return Boolean(
    options.namingFloor &&
      options.namingFloorSweep &&
      options.reconcilePriorDiff &&
      options.priorVersionCode &&
      !options.sourceMap
  );
}

/**
 * Flag-gated naming floor. Two DETERMINISTIC, cross-version-stable passes
 * (class/fn-expression inner-id derivation, decoration retry) always run;
 * the LLM coverage sweep over the remaining minted survivors is opt-in
 * (namingFloorSweep) and runs here only when it cannot be prior-aware —
 * with a prior it defers to after the reconcile pass (isSweepDeferred).
 * All apply through the validated path; every gate skips.
 */
async function maybeRunNamingFloor(
  ast: t.File,
  options: RenamePluginOptions,
  deps: NamingFloorDeps
): Promise<NamingFloorResult | undefined> {
  if (!options.namingFloor) return undefined;
  const span = deps.profiler.startSpan("rename:naming-floor", "pipeline");
  const taint = collectEvalWithTaint(ast);
  const derivation = deriveExpressionInnerNames(ast, deps.isEligible, taint);
  const decoration = retryDecoratedNames(ast, deps.isEligible, taint);
  const sweep =
    options.namingFloorSweep && !isSweepDeferred(options)
      ? await sweepMintedNames(ast, deps.provider, deps.isEligible, taint, {
          concurrency: deps.concurrency
        })
      : { named: 0, skipped: 0, groups: 0 };
  const result: NamingFloorResult = {
    derived: derivation.derived,
    undecorated: decoration.undecorated,
    swept: sweep.named,
    skipped: derivation.skipped.length + decoration.skipped + sweep.skipped
  };
  span.end({
    derived: result.derived,
    undecorated: result.undecorated,
    swept: result.swept
  });
  debug.log(
    "naming-floor",
    `derived ${result.derived} inner id(s), undecorated ${result.undecorated} ` +
      `name(s), swept ${result.swept} (${result.skipped} skipped, ` +
      `${sweep.groups} sweep groups)`
  );
  return result;
}

/**
 * Flag-gated prior-diff reconciliation. This is an OPTIONAL polish pass.
 * It returns undefined only when it did not RUN (flag off, no prior
 * version, sourceMap requested, output already invalid, or an internal
 * error contained in the step). When it ran, it returns its stats — with a
 * replacement code/ast ONLY when renames were applied and survived the
 * pure-rename invariant. The caller ships the pre-reconcile output
 * whenever code/ast are absent. It never fails the run — a best-effort
 * diff cleanup must not discard hours of completed work.
 */
function maybeReconcilePriorDiff(
  outputCode: string,
  options: RenamePluginOptions,
  isEligible: IsEligibleFn,
  genOpts: GeneratorOptions,
  profiler: Profiler,
  outputValid: boolean
): PriorDiffStepResult | undefined {
  if (!options.reconcilePriorDiff || !options.priorVersionCode)
    return undefined;
  if (options.sourceMap || !outputValid) return undefined;

  const span = profiler.startSpan("reconcile-prior-diff", "pipeline");
  const outcome = runPriorDiffReconciliation(
    outputCode,
    options.priorVersionCode,
    isEligible,
    genOpts
  );
  span.end({ renames: outcome?.stats.renames ?? 0 });
  if (!outcome) return undefined;

  debug.log(
    "reconcile-prior-diff",
    `snapped ${outcome.stats.renames} binding(s) to prior-version names ` +
      `(${outcome.stats.skipped} skipped)`
  );
  return {
    stats: outcome.stats,
    renames: outcome.renames,
    code: outcome.code,
    ast: outcome.ast
  };
}

/**
 * The deferred (prior-aware) coverage sweep over the shipping output —
 * the reconciled code when the reconcile pass replaced it, else the
 * generated output. Runs only when isSweepDeferred held back the
 * pre-generate sweep and the output is valid; whatever is still minted
 * after reconciliation truly has no usable prior counterpart.
 * Best-effort: it only ever REPLACES the output with an equally-valid
 * pure rename or leaves it untouched.
 */
async function maybeRunDeferredSweep(
  outputCode: string,
  recon: PriorDiffStepResult | undefined,
  options: RenamePluginOptions,
  deps: NamingFloorDeps,
  genOpts: GeneratorOptions,
  outputValid: boolean
): Promise<DeferredSweepOutcome | undefined> {
  if (!isSweepDeferred(options) || !outputValid) return undefined;
  const span = deps.profiler.startSpan("rename:deferred-sweep", "pipeline");
  const outcome = await runDeferredSweep(
    recon?.code ?? outputCode,
    deps.provider,
    deps.isEligible,
    { concurrency: deps.concurrency, genOpts }
  );
  span.end({ swept: outcome?.named ?? 0 });
  if (outcome) {
    debug.log(
      "naming-floor",
      `deferred sweep named ${outcome.named} residue binding(s) ` +
        `(${outcome.skipped} skipped)`
    );
  }
  return outcome;
}

/**
 * Resolve the shipping code/AST after the optional reconcile and
 * deferred-sweep passes — each only ever replaces the output with an
 * equally-valid pure rename, so the last pass that produced code wins —
 * and fold the deferred sweep's counts into the naming-floor stats.
 */
function resolveFinalOutput(
  outputCode: string,
  workingAst: t.File,
  recon: PriorDiffStepResult | undefined,
  deferredSweep: DeferredSweepOutcome | undefined,
  namingFloor: NamingFloorResult | undefined
): { finalCode: string; finalAst: t.File } {
  if (namingFloor && deferredSweep) {
    namingFloor.swept += deferredSweep.named;
    namingFloor.skipped += deferredSweep.skipped;
  }
  return {
    finalCode: deferredSweep?.code ?? recon?.code ?? outputCode,
    finalAst: deferredSweep?.ast ?? recon?.ast ?? workingAst
  };
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
  graph: ReturnType<typeof buildUnifiedGraph>
): void {
  const taint = collectEvalWithTaint(ast);
  if (taint.siteCount === 0) return;

  let frozenFunctions = 0;
  let frozenBindings = 0;
  for (const [, renameNode] of graph.nodes) {
    if (renameNode.type === "function") {
      if (taint.taintedFunctions.has(renameNode.node.path.node)) {
        markSkipped(renameNode.node, "eval-with-taint");
        frozenFunctions++;
      }
    } else if (taint.moduleTainted) {
      markSkipped(renameNode.node, "eval-with-taint");
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
function markWrapperPreDone(graph: ReturnType<typeof buildUnifiedGraph>): void {
  if (!graph.wrapperPath) return;
  const wrapperNode = graph.wrapperPath.node;
  for (const [, renameNode] of graph.nodes) {
    if (
      renameNode.type === "function" &&
      renameNode.node.path.node === wrapperNode
    ) {
      if (isSettled(renameNode.node)) break; // already frozen by eval-taint
      markSkipped(renameNode.node, "wrapper-iife");
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
  commentRegions: CommentRegion[] | undefined
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
      // A library function already frozen by eval-taint keeps that reason;
      // it still joins the prefix pass either way.
      if (isPending(fn)) markSkipped(fn, "library");
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
  allFunctions: FunctionNode[]
): { libraryFunctions: FunctionNode[]; libraryMap: Map<string, string> } {
  const skipLibs = options.skipLibraries ?? true;
  const commentRegions =
    !skipLibs || graph.wrapperPath ? undefined : context?.commentRegions;
  return markLibraryFunctionsPreDone(allFunctions, commentRegions);
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

    // Freeze scope chains of with/direct-eval sites before any renaming
    markEvalWithTaintPreDone(ast as t.File, graph);

    // Mark wrapper IIFE as pre-done so its children can process without deadlock
    markWrapperPreDone(graph);

    // Collect all function nodes for library detection
    const allFunctions = collectAllFunctions(graph);

    // Filter out library functions from mixed files (Layer 3)
    const { libraryFunctions, libraryMap } = detectAndMarkLibraries(
      options,
      graph,
      context,
      allFunctions
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
      profiler
    );
    priorSpan.end({
      functionsMatched: priorVersionApplied,
      bindingsApplied: priorVersionBindingsApplied,
      closeMatches: priorVersionCloseMatch
    });

    // Settled nodes (frozen / transferred) stay in the graph; the processor
    // derives its done set from node state, and deleting them would leave
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

    // Step 4: Naming floor — close minted-token coverage gaps. Runs after
    // every name is final (derivations copy the final name) and before
    // generate/invariant so the output validation nets it.
    const floorDeps: NamingFloorDeps = {
      isEligible,
      profiler,
      provider,
      concurrency: options.concurrency ?? 50
    };
    const namingFloor = await maybeRunNamingFloor(
      ast as t.File,
      options,
      floorDeps
    );

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

    // Hermetic rename-only invariant: the fully-renamed AST must differ from
    // the pre-rename baseline in binding NAMES only. Checked before generate
    // (no re-parse noise); catches any accidental structural edit — the
    // strongest guarantee we have that the output is a pure rename, and the
    // only one that works on artifacts we can't execute (Bun bytecode).
    const structuralFailure = checkStructuralInvariant(
      ast as t.File,
      semanticBaseline
    );

    metrics.setStage("generating");
    const generateSpan = profiler.startSpan("generate", "pipeline");
    const output = generate(ast, genOpts, genSource);
    generateSpan.end({ codeLength: output.code.length });

    const { parseFailure, semanticFailure: outputSemanticFailure } =
      validateGeneratedOutput(output.code, profiler, semanticBaseline);
    // The structural signature subsumes the free-name/binding-count check and
    // pinpoints the change, so prefer it when both fire.
    const semanticFailure = structuralFailure ?? outputSemanticFailure;
    const outputValid = !parseFailure && !semanticFailure;

    // Optional prior-diff reconciliation. It only ever REPLACES the output
    // with an equally-valid pure rename (or returns undefined); it cannot
    // introduce a failure, so parse/semantic failures stay the originals'.
    const recon = maybeReconcilePriorDiff(
      output.code,
      options,
      isEligible,
      genOpts,
      profiler,
      outputValid
    );

    // Prior-aware coverage sweep, deferred from the naming floor: the
    // reconcile pass has now transferred every prior name it could onto
    // the minted sweep targets, so the LLM names only the residue.
    const deferredSweep = await maybeRunDeferredSweep(
      output.code,
      recon,
      options,
      floorDeps,
      genOpts,
      outputValid
    );
    const { finalCode, finalAst } = resolveFinalOutput(
      output.code,
      ast as t.File,
      recon,
      deferredSweep,
      namingFloor
    );

    // Replayable rename ledger. Base entries reproduce the LLM-rename output
    // (beautified-input space); post stages replay the reconcile and
    // deferred-sweep passes (each in the prior stage's output space), so the
    // ledger reproduces the FINAL shipped code. Self-verified against finalCode.
    const renameLedger = options.emitRenameLedger
      ? buildRenameLedgerBundle(
          originalCode,
          ast as t.File,
          buildLedgerPostStages(output.code, recon, deferredSweep),
          finalCode
        )
      : undefined;

    // Truthful leftover count: walk the FINAL, shipping AST for minted
    // bindings no naming path reached (report-derived counters can't see
    // them, and the reconcile + deferred-sweep passes may have resolved
    // more). exp021's naming floor drives this toward zero.
    coverage.mintedCensus = summarizeCensus(
      collectMintedBindings(finalAst, isEligible)
    );
    const coverageSummary = formatCoverageSummary(coverage);

    metrics.setStage("done");
    return {
      code: finalCode,
      ast: finalAst,
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
      priorDiffReconciled: reconciledStats(recon),
      namingFloor: floorStats(namingFloor),
      renameLedger,
      internalErrors: processor.failed
    };
  };
}

/** The post-generate rename passes as (input, ast) stages, mirroring
 * resolveFinalOutput's precedence: reconcile renames the generated output;
 * the deferred sweep renames the reconciled output (or the generated output
 * when reconcile did not run). Each pass parsed its input string and only
 * mutated identifier names, so `buildRenameLedger(input, ast)` reproduces
 * that pass's output — the same derivation as the base stage. */
function buildLedgerPostStages(
  outputCode: string,
  recon: PriorDiffStepResult | undefined,
  deferredSweep: DeferredSweepOutcome | undefined
): Array<{ input: string; ast: t.File }> {
  const stages: Array<{ input: string; ast: t.File }> = [];
  if (recon?.ast) stages.push({ input: outputCode, ast: recon.ast });
  if (deferredSweep?.ast) {
    stages.push({ input: recon?.code ?? outputCode, ast: deferredSweep.ast });
  }
  return stages;
}

/** Build the rename ledger + snapshot, verifying replay reproduces the FINAL
 * shipped output. `postStages` are the post-generate passes (reconcile,
 * deferred sweep), each keyed by the prior stage's output. A divergence is
 * logged (not fatal — the ledger is a diagnostic artifact), so an incomplete
 * capture is visible but never fails an otherwise-good run. */
function buildRenameLedgerBundle(
  source: string,
  ast: t.File,
  postStages: Array<{ input: string; ast: t.File }>,
  expectedOutput: string
): { ledger: RenameLedger; source: string } {
  const base = buildRenameLedger(source, ast);
  const post = postStages.map(({ input, ast: stageAst }) => {
    const staged = buildRenameLedger(input, stageAst);
    return { sourceSha256: staged.sourceSha256, entries: staged.entries };
  });
  const ledger: RenameLedger = post.length > 0 ? { ...base, post } : base;
  // Self-check (non-fatal — the ledger is a diagnostic artifact). Replay can
  // even throw when a stage's snapshot hash does not line up (e.g. `source`
  // is not a generate() fixed point, so offsets do not align); log it, never
  // fail the run.
  let reproduces = false;
  try {
    reproduces = applyRenameLedger(source, ledger) === expectedOutput;
  } catch {
    reproduces = false;
  }
  if (!reproduces) {
    debug.log(
      "rename-ledger",
      "WARNING: replay does not reproduce the shipped output — " +
        "the ledger may be missing a rename"
    );
  }
  return { ledger, source };
}

function reconciledStats(
  recon: PriorDiffStepResult | undefined
): RenamePluginResult["priorDiffReconciled"] {
  if (!recon) return undefined;
  return {
    renames: recon.stats.renames,
    skipped: recon.stats.skipped,
    pairs: recon.renames
  };
}

function floorStats(
  floor: NamingFloorResult | undefined
): RenamePluginResult["namingFloor"] {
  if (!floor) return undefined;
  return {
    derived: floor.derived,
    undecorated: floor.undecorated,
    swept: floor.swept,
    skipped: floor.skipped
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

/** Maximum lines of a single declaration shown in a module-binding profile.
 *  A multi-thousand-line object literal embedded whole overflows the model
 *  context and 400-fails the batch. */
const MAX_DECLARATION_LINES = 10;
/** Char cap for the same — a giant base64 blob is ONE line (205KB in the
 *  Claude Code fixtures) and sails through any line cap. */
const MAX_DECLARATION_CHARS = 1000;

function capDeclarationText(code: string): string {
  const lines = code.split("\n");
  let text = code;
  if (lines.length > MAX_DECLARATION_LINES) {
    text = `${lines.slice(0, MAX_DECLARATION_LINES).join("\n")}\n  // ...`;
  }
  if (text.length > MAX_DECLARATION_CHARS) {
    text = `${text.slice(0, MAX_DECLARATION_CHARS)}…`;
  }
  return text;
}

/**
 * Returns the declaration text for a function/class declaration binding path.
 */
function getFunctionOrClassDeclarationText(
  name: string,
  bindingPath: babelTraverse.NodePath
): string {
  try {
    return capDeclarationText(generate(bindingPath.node).code);
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
    return capDeclarationText(generate(declPath.node).code);
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
  return capDeclarationText(generate(bindingPath.node).code);
}

/**
 * Returns true if a binding should be skipped from the module binding pool.
 * Function declarations are skipped because they are processed as
 * FunctionNodes by the function graph (including their declaration name,
 * via collectFunctionNameBinding in processor.ts). CLASS declarations are
 * NOT function nodes — excluding them left module-scope class names
 * invisible to every naming path in both legs of a cross-version run
 * (the y6→C6 reroll family), so they stay in the pool.
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

  // Skip function declarations — they're processed as FunctionNodes.
  if (bindingPath.isFunctionDeclaration()) {
    return true;
  }

  // For variable declarators, skip if init is a NAMED function expression
  // (its FunctionNode covers the name). A named CLASS expression has no
  // node — the declarator binding stays nameable here.
  if (bindingPath.isVariableDeclarator()) {
    const init = (bindingPath.node as t.VariableDeclarator).init;
    if (t.isFunctionExpression(init) && init.id) {
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
