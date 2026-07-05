/**
 * Debug wrapper for LLM providers that logs all prompts and responses.
 */

import { debug } from "../debug.js";
import type {
  BatchRenameRequest,
  BatchRenameResponse,
  LLMProvider
} from "./types.js";

/**
 * Wraps an LLM provider to log all requests and responses when debug mode is enabled.
 * Logs request+response together as a single block to avoid interleaving under concurrency.
 */
class DebugLLMProvider implements LLMProvider {
  constructor(
    private inner: LLMProvider,
    private model?: string
  ) {}

  async suggestAllNames(
    request: BatchRenameRequest
  ): Promise<BatchRenameResponse> {
    const identifiers = request.identifiers;
    debug.log(
      "llm",
      `suggestAllNames → identifiers: ${identifiers.join(", ")}`
    );
    const start = Date.now();
    try {
      const result = await this.inner.suggestAllNames(request);
      debug.llmRoundtrip("suggestAllNames", {
        model: this.model,
        identifiers,
        userPrompt: request.userPrompt
          ? request.userPrompt
          : `Code:\n${request.code}\n\nIdentifiers: ${identifiers.join(", ")}\nUsed names: ${[...request.usedNames].slice(0, 30).join(", ")}...\nIs retry: ${request.isRetry}\n${request.failures ? `Failures: duplicates=${request.failures.duplicates.join(",")}, invalid=${request.failures.invalid.join(",")}` : ""}`,
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmRoundtrip("suggestAllNames", {
        model: this.model,
        identifiers,
        error: error as Error,
        durationMs: Date.now() - start
      });
      throw error;
    }
  }
}

/**
 * Wraps a provider with debug logging.
 * Always wraps — debug.enabled is a live getter derived from verbose.level,
 * so the inner checks gate output dynamically.
 */
export function withDebug(provider: LLMProvider, model?: string): LLMProvider {
  return new DebugLLMProvider(provider, model);
}
