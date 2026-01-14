/**
 * Unified rename plugin that works with any LLM provider.
 *
 * This replaces the legacy per-provider plugins (openaiRename, geminiRename, localRename)
 * with a single implementation that uses the RenameProcessor for parallel,
 * dependency-ordered function processing.
 */

import { parseSync } from "@babel/core";
import * as babelGenerator from "@babel/generator";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import { RenameProcessor } from "../rename/processor.js";
import { MetricsTracker, formatMetricsCompact } from "../llm/metrics.js";
import type { LLMProvider } from "../llm/types.js";

// Handle ESM/CJS compatibility for babel generator
const generate: typeof babelGenerator.default =
  typeof babelGenerator.default === "function"
    ? babelGenerator.default
    : (babelGenerator.default as any).default;

export interface RenamePluginOptions {
  /** The LLM provider to use for name suggestions */
  provider: LLMProvider;

  /** Maximum number of concurrent function processing (default: 50) */
  concurrency?: number;

  /** Callback for progress updates */
  onProgress?: (message: string) => void;
}

/**
 * Creates a rename plugin that processes all functions in dependency order
 * using the provided LLM provider.
 *
 * @param options Configuration options for the rename plugin
 * @returns An async function that transforms code
 */
export function createRenamePlugin(options: RenamePluginOptions) {
  const { provider, concurrency = 50, onProgress } = options;

  return async (code: string): Promise<string> => {
    const ast = parseSync(code, {
      sourceType: "unambiguous"
    });

    if (!ast) {
      throw new Error("Failed to parse code");
    }

    const functions = buildFunctionGraph(ast, "input.js");

    if (functions.length === 0) {
      // No functions to process, return as-is
      const output = generate(ast);
      return output.code;
    }

    const metrics = new MetricsTracker({
      onMetrics: (m) => onProgress?.(formatMetricsCompact(m))
    });

    const processor = new RenameProcessor(ast);
    await processor.processAll(functions, provider, {
      concurrency,
      metrics
    });

    const output = generate(ast);
    return output.code;
  };
}
