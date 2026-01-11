import type { FunctionNode } from "../analysis/types.js";
import type { CostEstimate } from "./types.js";

/**
 * Cost per 1K tokens for various models.
 * Prices are approximate and may change.
 * Format: { input: cost, output: cost } per 1K tokens in USD
 */
export const MODEL_COSTS: Record<string, { input: number; output: number }> = {
  // OpenAI models
  "gpt-4o": { input: 0.0025, output: 0.01 },
  "gpt-4o-mini": { input: 0.00015, output: 0.0006 },
  "gpt-4-turbo": { input: 0.01, output: 0.03 },
  "gpt-4": { input: 0.03, output: 0.06 },
  "gpt-3.5-turbo": { input: 0.0005, output: 0.0015 },

  // Claude models (via OpenRouter)
  "anthropic/claude-3-opus": { input: 0.015, output: 0.075 },
  "anthropic/claude-3-sonnet": { input: 0.003, output: 0.015 },
  "anthropic/claude-3-haiku": { input: 0.00025, output: 0.00125 },

  // Open source models (via OpenRouter or local)
  "meta-llama/llama-3-70b": { input: 0.00059, output: 0.00079 },
  "meta-llama/llama-3-8b": { input: 0.00005, output: 0.00005 },
  "mistralai/mixtral-8x7b": { input: 0.00024, output: 0.00024 },

  // Google models
  "gemini-pro": { input: 0.000125, output: 0.000375 },
  "gemini-1.5-pro": { input: 0.00125, output: 0.005 },
  "gemini-1.5-flash": { input: 0.000075, output: 0.0003 },

  // Local models (free)
  local: { input: 0, output: 0 },
  "2b": { input: 0, output: 0 },
  "8b": { input: 0, output: 0 }
};

/**
 * Estimates the number of bindings (variables to rename) in a function.
 */
function countBindings(fn: FunctionNode): number {
  const scope = fn.path.scope;
  return Object.keys(scope.bindings).length;
}

/**
 * Estimates the cost of processing a set of functions.
 *
 * Estimation is based on:
 * - Number of identifiers to rename
 * - Average tokens per request (~500 for prompt + response)
 * - Model pricing
 */
export function estimateCost(
  functions: FunctionNode[],
  model: string
): CostEstimate {
  // Count total identifiers to rename
  const totalIdentifiers = functions.reduce(
    (sum, fn) => sum + countBindings(fn),
    0
  );

  // Estimate tokens: ~400 input + ~100 output per identifier
  const inputTokensPerIdentifier = 400;
  const outputTokensPerIdentifier = 100;

  const totalInputTokens = totalIdentifiers * inputTokensPerIdentifier;
  const totalOutputTokens = totalIdentifiers * outputTokensPerIdentifier;
  const estimatedTokens = totalInputTokens + totalOutputTokens;

  // Get model costs (default to a small amount if unknown)
  const costs = MODEL_COSTS[model] ?? MODEL_COSTS[model.split("/").pop() ?? ""] ?? {
    input: 0.001,
    output: 0.001
  };

  // Calculate cost
  const estimatedCost =
    (totalInputTokens / 1000) * costs.input +
    (totalOutputTokens / 1000) * costs.output;

  return {
    totalIdentifiers,
    estimatedTokens,
    estimatedCost,
    model
  };
}

/**
 * Formats a cost estimate for display.
 */
export function formatCostEstimate(estimate: CostEstimate): string {
  const lines = [
    `Model: ${estimate.model}`,
    `Identifiers to rename: ${estimate.totalIdentifiers}`,
    `Estimated tokens: ~${estimate.estimatedTokens.toLocaleString()}`
  ];

  if (estimate.estimatedCost > 0) {
    lines.push(`Estimated cost: $${estimate.estimatedCost.toFixed(4)}`);
  } else {
    lines.push("Estimated cost: Free (local model)");
  }

  return lines.join("\n");
}

/**
 * Returns true if the model is a local (free) model.
 */
export function isLocalModel(model: string): boolean {
  const costs = MODEL_COSTS[model];
  return costs !== undefined && costs.input === 0 && costs.output === 0;
}
