/**
 * Unified rename plugin that works with any LLM provider.
 *
 * This replaces the legacy per-provider plugins (openaiRename, geminiRename, localRename)
 * with a single implementation that uses the RenameProcessor for parallel,
 * dependency-ordered function processing.
 */

import { parseSync } from "@babel/core";
import * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import { RenameProcessor } from "../rename/processor.js";
import { MetricsTracker, formatMetricsCompact } from "../llm/metrics.js";
import type { LLMProvider, BatchRenameRequest } from "../llm/types.js";
import type { FunctionRenameReport, FunctionNode } from "../analysis/types.js";
import { classifyFunctionsByRegion } from "../library-detection/comment-regions.js";
import type { CommentRegion } from "../library-detection/comment-regions.js";
import {
  sanitizeIdentifier,
  isValidIdentifier,
  RESERVED_WORDS
} from "../llm/validation.js";
import {
  MODULE_LEVEL_RENAME_SYSTEM_PROMPT,
  buildModuleLevelRenamePrompt
} from "../llm/prompts.js";
import { debug } from "../debug.js";
import { generate, traverse } from "../babel-utils.js";
import { looksMinified } from "../rename/minified-heuristic.js";
import type { GeneratorOptions, GeneratorResult } from "@babel/generator";

export interface RenamePluginOptions {
  /** The LLM provider to use for name suggestions */
  provider: LLMProvider;

  /** Maximum number of concurrent function processing (default: 50) */
  concurrency?: number;

  /** Callback for progress updates */
  onProgress?: (message: string) => void;

  /** Generate a source map alongside the output code */
  sourceMap?: boolean;

  /**
   * Comment regions for mixed-file detection (Rollup/esbuild bundles).
   * When set, functions inside these regions are classified as library code
   * and skipped during processing. Read fresh each invocation so callers
   * can update it per-file.
   */
  commentRegions?: CommentRegion[];
}

/**
 * Result from the rename plugin, including output code and diagnostic reports.
 */
export interface RenamePluginResult {
  code: string;
  reports: ReadonlyArray<FunctionRenameReport>;
  sourceMap: GeneratorResult["map"];
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

  return async (code: string): Promise<RenamePluginResult> => {
    const originalCode = code;
    const ast = parseSync(code, {
      sourceType: "unambiguous"
    });

    if (!ast) {
      throw new Error("Failed to parse code");
    }

    const metrics = new MetricsTracker({
      onMetrics: (m) => onProgress?.(formatMetricsCompact(m))
    });

    const genOpts: GeneratorOptions = options.sourceMap
      ? { sourceMaps: true, sourceFileName: "input.js" }
      : {};
    const genSource = options.sourceMap ? originalCode : undefined;

    // Phase 1: Rename module-level bindings before function processing
    let moduleResult: ModuleRenameResult = {};
    if (provider.suggestAllNames) {
      moduleResult = await renameModuleBindings(ast, provider, metrics);
    }

    // Phase 2: Build function graph and process functions
    const functions = buildFunctionGraph(ast, "input.js");

    if (functions.length === 0) {
      const output = generate(ast, genOpts, genSource);
      return { code: output.code, reports: [], sourceMap: output.map };
    }

    // Collect pre-done functions (library + wrapper IIFE)
    const preDone: FunctionNode[] = [];

    // Mark wrapper IIFE as pre-done so its children can process without deadlock
    if (moduleResult.wrapperPath) {
      const wrapperNode = moduleResult.wrapperPath.node;
      for (const fn of functions) {
        if (fn.path.node === wrapperNode) {
          fn.status = "done";
          fn.renameMapping = { names: {} };
          preDone.push(fn);
          debug.log("wrapper", `Marked wrapper function ${fn.sessionId} as pre-done`);
          break;
        }
      }
    }

    // Filter out library functions from mixed files (Layer 3)
    // ONLY use comment regions when there is NO wrapper — wrapper bundles
    // are single-file CJS where comment regions don't work reliably
    const commentRegions = moduleResult.wrapperPath ? undefined : options.commentRegions;
    const libraryFunctions: FunctionNode[] = [];
    let novelFunctions = functions;
    if (commentRegions && commentRegions.length > 0) {
      const libraryIds = classifyFunctionsByRegion(functions, commentRegions);
      if (libraryIds.size > 0) {
        // Mark library functions as done so they don't block callers
        for (const fn of functions) {
          if (libraryIds.has(fn.sessionId)) {
            fn.status = "done";
            fn.renameMapping = { names: {} };
            preDone.push(fn);
            libraryFunctions.push(fn);
          }
        }
        novelFunctions = functions.filter((fn) => !libraryIds.has(fn.sessionId));
        debug.log("mixed-file", `Skipping ${libraryIds.size} library functions, processing ${novelFunctions.length} app functions`);
      }
    }

    // Also filter out the wrapper IIFE itself from novel functions
    novelFunctions = novelFunctions.filter(fn => fn.status !== "done");

    const processor = new RenameProcessor(ast);
    let allReports: FunctionRenameReport[] = [];

    if (novelFunctions.length > 0) {
      // Phase 2: Process app functions
      await processor.processAll(novelFunctions, provider, {
        concurrency,
        metrics,
        preDone: preDone.length > 0 ? preDone : undefined,
      });
      allReports = [...processor.reports];
    }

    // Phase 3: Rename library function parameters (lightweight param-only mode)
    if (libraryFunctions.length > 0 && provider.suggestAllNames) {
      // Filter to library functions that have minified-looking params
      const libraryWithMinifiedParams = libraryFunctions.filter(fn => {
        const params = fn.path.node.params;
        return params.some((p: any) => {
          if (t.isIdentifier(p)) return looksMinified(p.name);
          if (t.isAssignmentPattern(p) && t.isIdentifier(p.left)) return looksMinified(p.left.name);
          if (t.isRestElement(p) && t.isIdentifier(p.argument)) return looksMinified(p.argument.name);
          return false;
        });
      });

      if (libraryWithMinifiedParams.length > 0) {
        debug.log("library-params", `Phase 3: processing params for ${libraryWithMinifiedParams.length} library functions`);

        // Reset their status so processor can work on them
        for (const fn of libraryWithMinifiedParams) {
          fn.status = "pending";
        }

        const paramProcessor = new RenameProcessor(ast);
        await paramProcessor.processAll(libraryWithMinifiedParams, provider, {
          concurrency,
          metrics,
          paramOnly: true,
        });
        allReports = [...allReports, ...paramProcessor.reports];
      }
    }

    const output = generate(ast, genOpts, genSource);
    return { code: output.code, reports: allReports, sourceMap: output.map };
  };
}

/** Minimum number of bindings for an IIFE to be considered a wrapper */
const WRAPPER_IIFE_BINDING_THRESHOLD = 50;

/** Maximum identifiers per batch for module-level renaming */
const MODULE_BATCH_SIZE = 30;

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
    const fn = expr.callee;

    // (function(){...})() or (() => {...})()
    if (t.isFunctionExpression(fn) || t.isArrowFunctionExpression(fn)) {
      callee = fn;
    }

    // (function(){}).call(this, ...) or .apply(...)
    if (
      t.isMemberExpression(fn) &&
      t.isIdentifier(fn.property) &&
      (fn.property.name === "call" || fn.property.name === "apply") &&
      (t.isFunctionExpression(fn.object) || t.isArrowFunctionExpression(fn.object))
    ) {
      callee = fn.object;
    }
  }

  // !function(){...}()
  if (
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
          debug.log("wrapper", `Detected wrapper function with ${bindingCount} bindings`);
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
 * Collects module-level bindings that look minified and aren't functions/classes.
 * When a giant wrapper IIFE is detected, uses the wrapper's scope instead of programScope.
 */
function getModuleLevelBindings(ast: t.File): ModuleLevelBindingsResult | null {
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

  for (const [name, binding] of Object.entries(targetScope.bindings) as [string, any][]) {
    // Skip if not minified-looking
    if (!looksMinified(name)) continue;

    const bindingPath = binding.path;

    // Skip function/class declarations when NOT in wrapper mode — in normal
    // program scope they're handled by the function pipeline (getOwnBindings).
    // But when a wrapper IIFE is detected, the wrapper is marked preDone so
    // its getOwnBindings never runs — function declaration names must be
    // renamed here at module level.
    if (!wrapper) {
      if (
        bindingPath.isFunctionDeclaration() ||
        bindingPath.isClassDeclaration()
      ) {
        continue;
      }
    }

    // For variable declarators, skip if init is a NAMED function/class expression
    // (the function pipeline renames the name via getOwnBindings).
    // Do NOT skip arrow functions or anonymous function expressions — they have
    // no own name, so the variable binding must be renamed here at module level.
    if (bindingPath.isVariableDeclarator()) {
      const init = bindingPath.node.init;
      if (
        (t.isFunctionExpression(init) && init.id) ||
        (t.isClassExpression(init) && init.id)
      ) {
        continue;
      }
    }

    // Get the declaration text for context
    let declaration = "";
    if (bindingPath.isFunctionDeclaration() || bindingPath.isClassDeclaration()) {
      // For function/class declarations in wrapper scope, truncate to just the
      // signature — the full body could be huge
      const params = bindingPath.node.params?.map((p: any) => generate(p).code).join(", ") ?? "";
      declaration = `function ${name}(${params}) { ... }`;
    } else if (bindingPath.isVariableDeclarator()) {
      const declPath = bindingPath.parentPath;
      if (declPath) {
        declaration = generate(declPath.node).code;
      }
    } else if (bindingPath.isImportSpecifier() || bindingPath.isImportDefaultSpecifier() || bindingPath.isImportNamespaceSpecifier()) {
      const importPath = bindingPath.parentPath;
      if (importPath) {
        declaration = generate(importPath.node).code;
      }
    } else {
      declaration = generate(bindingPath.node).code;
    }

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
  };
}

/**
 * Collects usage examples for module-level identifiers (up to 3 per identifier).
 */
function collectUsageExamples(
  ast: t.File,
  identifiers: Set<string>
): Record<string, string[]> {
  const examples: Record<string, string[]> = {};
  for (const id of identifiers) {
    examples[id] = [];
  }

  traverse(ast, {
    Identifier(path: babelTraverse.NodePath<t.Identifier>) {
      const name = path.node.name;
      if (!identifiers.has(name)) return;
      if (examples[name].length >= 3) return;

      // Skip the declaration itself
      if (path.isBindingIdentifier()) return;

      // Get the containing statement for context
      // Cast needed: after isBindingIdentifier() narrows to `never`, TS loses findParent
      const statement = (path as babelTraverse.NodePath<t.Identifier>).findParent((p: babelTraverse.NodePath) => p.isStatement() || p.isDeclaration());
      if (statement) {
        try {
          const code = generate(statement.node).code;
          if (code) {
            const line = code.split("\n")[0].trim();
            if (line.length <= 80 && !examples[name].includes(line)) {
              examples[name].push(line);
            }
          }
        } catch {
          // Skip if generation fails for this node
        }
      }
    }
  });

  return examples;
}

/**
 * Result of module-level renaming, including wrapper info for pre-done marking.
 */
interface ModuleRenameResult {
  /** Path to the wrapper IIFE function, if one was detected */
  wrapperPath?: babelTraverse.NodePath<t.Function>;
}

/**
 * Renames module-level bindings using the LLM.
 * When many bindings are present (e.g., giant IIFE wrapper), batches them
 * into groups of MODULE_BATCH_SIZE for the LLM.
 */
async function renameModuleBindings(
  ast: t.File,
  provider: LLMProvider,
  metrics: MetricsTracker
): Promise<ModuleRenameResult> {
  const result = getModuleLevelBindings(ast);
  if (!result) return {};

  const { bindings, targetScope, wrapperPath } = result;

  // Collect all names already in use
  const usedNames = new Set<string>();
  for (const name of Object.keys(targetScope.bindings)) {
    usedNames.add(name);
  }

  // Collect usage examples once for all identifiers
  const allIdentifiers = bindings.map(b => b.name);
  const usageExamples = collectUsageExamples(ast, new Set(allIdentifiers));

  // Batch bindings if there are many
  const batches: ModuleBinding[][] = [];
  for (let i = 0; i < bindings.length; i += MODULE_BATCH_SIZE) {
    batches.push(bindings.slice(i, i + MODULE_BATCH_SIZE));
  }

  debug.log("module-level", `${bindings.length} bindings in ${batches.length} batch(es)${wrapperPath ? " (wrapper IIFE detected)" : ""}`);

  const seenNewNames = new Set<string>();

  for (let batchIdx = 0; batchIdx < batches.length; batchIdx++) {
    const batch = batches[batchIdx];

    // De-duplicate declarations within this batch
    const declarations = [...new Set(batch.map(b => b.declaration))];
    const identifiers = batch.map(b => b.name);

    const userPrompt = buildModuleLevelRenamePrompt(
      declarations,
      usageExamples,
      identifiers,
      usedNames
    );

    const request: BatchRenameRequest = {
      code: "",
      identifiers,
      usedNames,
      calleeSignatures: [],
      callsites: [],
      systemPrompt: MODULE_LEVEL_RENAME_SYSTEM_PROMPT,
      userPrompt
    };

    debug.log("module-level", `Batch ${batchIdx + 1}/${batches.length}: ${identifiers.join(", ")}`);

    const done = metrics.llmCallStart();
    const response = await provider.suggestAllNames!(request);
    done?.();

    debug.log("module-level", `Batch ${batchIdx + 1} response`, response.renames);

    // Validate and apply renames
    for (const binding of batch) {
      const rawNewName = response.renames[binding.name];
      if (!rawNewName) {
        debug.log("module-level", `${binding.name}: no suggestion in response`);
        continue;
      }

      const newName = sanitizeIdentifier(rawNewName);

      if (newName === binding.name) {
        debug.log("module-level", `${binding.name}: same as original, skipping`);
        continue;
      }

      if (!isValidIdentifier(newName)) {
        debug.log("module-level", `${binding.name} → ${newName}: invalid identifier, skipping`);
        continue;
      }
      if (RESERVED_WORDS.has(newName)) {
        debug.log("module-level", `${binding.name} → ${newName}: reserved word, skipping`);
        continue;
      }

      if (seenNewNames.has(newName) || usedNames.has(newName)) {
        debug.log("module-level", `${binding.name} → ${newName}: duplicate/in-use, skipping`);
        continue;
      }

      debug.rename({
        functionId: "module-level",
        oldName: binding.name,
        newName,
        wasRetry: false,
        attemptNumber: 1
      });

      targetScope.rename(binding.name, newName);
      usedNames.add(newName);
      seenNewNames.add(newName);
    }
  }

  return { wrapperPath };
}
