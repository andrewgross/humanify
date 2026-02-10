/**
 * Unified rename plugin that works with any LLM provider.
 *
 * This replaces the legacy per-provider plugins (openaiRename, geminiRename, localRename)
 * with a single implementation that uses the RenameProcessor for parallel,
 * dependency-ordered function processing.
 */

import { parseSync } from "@babel/core";
import * as babelGenerator from "@babel/generator";
import * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import { RenameProcessor } from "../rename/processor.js";
import { MetricsTracker, formatMetricsCompact } from "../llm/metrics.js";
import type { LLMProvider, BatchRenameRequest } from "../llm/types.js";
import type { FunctionRenameReport } from "../analysis/types.js";
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

// Handle ESM/CJS compatibility for babel generator
const generate: typeof babelGenerator.default =
  typeof babelGenerator.default === "function"
    ? babelGenerator.default
    : (babelGenerator.default as any).default;

const traverse: typeof babelTraverse.default =
  typeof babelTraverse.default === "function"
    ? babelTraverse.default
    : (babelTraverse.default as any).default;

export interface RenamePluginOptions {
  /** The LLM provider to use for name suggestions */
  provider: LLMProvider;

  /** Maximum number of concurrent function processing (default: 50) */
  concurrency?: number;

  /** Callback for progress updates */
  onProgress?: (message: string) => void;
}

/**
 * Result from the rename plugin, including output code and diagnostic reports.
 */
export interface RenamePluginResult {
  code: string;
  reports: ReadonlyArray<FunctionRenameReport>;
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
    const ast = parseSync(code, {
      sourceType: "unambiguous"
    });

    if (!ast) {
      throw new Error("Failed to parse code");
    }

    const metrics = new MetricsTracker({
      onMetrics: (m) => onProgress?.(formatMetricsCompact(m))
    });

    // Phase 1: Rename module-level bindings before function processing
    if (provider.suggestAllNames) {
      await renameModuleBindings(ast, provider, metrics);
    }

    // Phase 2: Build function graph and process functions
    const functions = buildFunctionGraph(ast, "input.js");

    if (functions.length === 0) {
      const output = generate(ast);
      return { code: output.code, reports: [] };
    }

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, provider, {
      concurrency,
      metrics
    });

    const output = generate(ast);
    return { code: output.code, reports: processor.reports };
  };
}

interface ModuleBinding {
  name: string;
  identifier: t.Identifier;
  declaration: string;
}

/**
 * Checks if a name looks minified (1-2 characters, not common short names).
 */
function looksMinified(name: string): boolean {
  if (name.length > 2) return false;
  // Allow common short names that aren't minified
  const commonShort = new Set(["id", "fn", "cb", "el", "db", "io", "fs", "os", "vm", "ip"]);
  return !commonShort.has(name);
}

/**
 * Collects module-level bindings that look minified and aren't functions/classes.
 */
function getModuleLevelBindings(ast: t.File): { bindings: ModuleBinding[]; programScope: any } | null {
  let programScope: any = null;
  const bindings: ModuleBinding[] = [];

  traverse(ast, {
    Program(path: babelTraverse.NodePath<t.Program>) {
      programScope = path.scope;
      path.stop();
    }
  });

  if (!programScope) return null;

  for (const [name, binding] of Object.entries(programScope.bindings) as [string, any][]) {
    // Skip if not minified-looking
    if (!looksMinified(name)) continue;

    // Skip function/class declarations — handled by function pipeline
    const bindingPath = binding.path;
    if (
      bindingPath.isFunctionDeclaration() ||
      bindingPath.isClassDeclaration()
    ) {
      continue;
    }

    // For variable declarators, skip if init is a function/class expression
    if (bindingPath.isVariableDeclarator()) {
      const init = bindingPath.node.init;
      if (
        t.isFunctionExpression(init) ||
        t.isArrowFunctionExpression(init) ||
        t.isClassExpression(init)
      ) {
        continue;
      }
    }

    // Get the declaration text for context
    let declaration = "";
    if (bindingPath.isVariableDeclarator()) {
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

  return bindings.length > 0 ? { bindings, programScope } : null;
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
      const statement = path.findParent((p: babelTraverse.NodePath) => p.isStatement() || p.isDeclaration());
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
 * Renames module-level bindings using the LLM.
 */
async function renameModuleBindings(
  ast: t.File,
  provider: LLMProvider,
  metrics: MetricsTracker
): Promise<void> {
  const result = getModuleLevelBindings(ast);
  if (!result) return;

  const { bindings, programScope } = result;

  // Collect all names already in use at module level
  const usedNames = new Set<string>();
  for (const name of Object.keys(programScope.bindings)) {
    usedNames.add(name);
  }

  // De-duplicate declarations (multiple vars in one statement)
  const declarations = [...new Set(bindings.map(b => b.declaration))];
  const identifiers = bindings.map(b => b.name);
  const usageExamples = collectUsageExamples(ast, new Set(identifiers));

  // Build the batch request using the module-level prompt
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

  debug.log("module-level", `Requesting renames for: ${identifiers.join(", ")}`);

  const done = metrics.llmCallStart();
  const response = await provider.suggestAllNames!(request);
  done?.();

  debug.log("module-level", `LLM response`, response.renames);

  // Validate and apply renames
  const seenNewNames = new Set<string>();
  for (const binding of bindings) {
    const rawNewName = response.renames[binding.name];
    if (!rawNewName) {
      debug.log("module-level", `${binding.name}: no suggestion in response`);
      continue;
    }

    const newName = sanitizeIdentifier(rawNewName);

    // Skip if same as original
    if (newName === binding.name) {
      debug.log("module-level", `${binding.name}: same as original, skipping`);
      continue;
    }

    // Skip invalid names
    if (!isValidIdentifier(newName)) {
      debug.log("module-level", `${binding.name} → ${newName}: invalid identifier, skipping`);
      continue;
    }
    if (RESERVED_WORDS.has(newName)) {
      debug.log("module-level", `${binding.name} → ${newName}: reserved word, skipping`);
      continue;
    }

    // Skip duplicates
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

    // Apply rename — propagates to all references throughout the file
    programScope.rename(binding.name, newName);
    usedNames.add(newName);
    seenNewNames.add(newName);
  }
}
