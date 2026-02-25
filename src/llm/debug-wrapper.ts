/**
 * Debug wrapper for LLM providers that logs all prompts and responses.
 */

import type { LLMContext } from "../analysis/types.js";
import type {
  LLMProvider,
  NameSuggestion,
  BatchRenameRequest,
  BatchRenameResponse
} from "./types.js";
import { debug } from "../debug.js";

/**
 * Wraps an LLM provider to log all requests and responses when debug mode is enabled.
 * Logs request+response together as a single block to avoid interleaving under concurrency.
 */
export class DebugLLMProvider implements LLMProvider {
  constructor(
    private inner: LLMProvider,
    private model?: string
  ) {}

  async suggestName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    const userPrompt = `Function code:\n${context.functionCode}\n\nCallees: ${context.calleeSignatures.map(c => c.name).join(", ")}\nCall sites: ${context.callsites.join(", ")}\nUsed names: ${[...context.usedIdentifiers].slice(0, 20).join(", ")}...`;

    debug.log("llm", `suggestName → ${currentName}`);
    const start = Date.now();
    try {
      const result = await this.inner.suggestName(currentName, context);
      debug.llmRoundtrip("suggestName", {
        model: this.model,
        currentName,
        userPrompt,
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmRoundtrip("suggestName", {
        model: this.model,
        currentName,
        userPrompt,
        error: error as Error,
        durationMs: Date.now() - start
      });
      throw error;
    }
  }

  async suggestFunctionName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    if (!this.inner.suggestFunctionName) {
      return this.suggestName(currentName, context);
    }

    const userPrompt = `Function code:\n${context.functionCode}`;

    debug.log("llm", `suggestFunctionName → ${currentName}`);
    const start = Date.now();
    try {
      const result = await this.inner.suggestFunctionName(currentName, context);
      debug.llmRoundtrip("suggestFunctionName", {
        model: this.model,
        currentName,
        userPrompt,
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmRoundtrip("suggestFunctionName", {
        model: this.model,
        currentName,
        userPrompt,
        error: error as Error,
        durationMs: Date.now() - start
      });
      throw error;
    }
  }

  async retrySuggestName(
    currentName: string,
    rejectedName: string,
    reason: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    if (!this.inner.retrySuggestName) {
      const updatedContext = {
        ...context,
        usedIdentifiers: new Set([...context.usedIdentifiers, rejectedName])
      };
      return this.suggestName(currentName, updatedContext);
    }

    const userPrompt = `Rejected: "${rejectedName}" (${reason})\n\nFunction code:\n${context.functionCode}`;

    debug.log("llm", `retrySuggestName → ${currentName} (rejected: ${rejectedName})`);
    const start = Date.now();
    try {
      const result = await this.inner.retrySuggestName(currentName, rejectedName, reason, context);
      debug.llmRoundtrip("retrySuggestName", {
        model: this.model,
        currentName,
        userPrompt,
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmRoundtrip("retrySuggestName", {
        model: this.model,
        currentName,
        userPrompt,
        error: error as Error,
        durationMs: Date.now() - start
      });
      throw error;
    }
  }

  async retryFunctionName(
    currentName: string,
    rejectedName: string,
    reason: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    if (!this.inner.retryFunctionName) {
      if (this.inner.retrySuggestName) {
        return this.retrySuggestName(currentName, rejectedName, reason, context);
      }
      const updatedContext = {
        ...context,
        usedIdentifiers: new Set([...context.usedIdentifiers, rejectedName])
      };
      return this.suggestFunctionName(currentName, updatedContext);
    }

    const userPrompt = `Rejected: "${rejectedName}" (${reason})`;

    debug.log("llm", `retryFunctionName → ${currentName} (rejected: ${rejectedName})`);
    const start = Date.now();
    try {
      const result = await this.inner.retryFunctionName(currentName, rejectedName, reason, context);
      debug.llmRoundtrip("retryFunctionName", {
        model: this.model,
        currentName,
        userPrompt,
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmRoundtrip("retryFunctionName", {
        model: this.model,
        currentName,
        userPrompt,
        error: error as Error,
        durationMs: Date.now() - start
      });
      throw error;
    }
  }

  async suggestNames(
    requests: Array<{ name: string; context: LLMContext }>
  ): Promise<NameSuggestion[]> {
    if (!this.inner.suggestNames) {
      const results: NameSuggestion[] = [];
      for (const req of requests) {
        const suggestion = await this.suggestName(req.name, req.context);
        results.push(suggestion);
      }
      return results;
    }

    const identifiers = requests.map(r => r.name);
    debug.log("llm", `suggestNames → identifiers: ${identifiers.join(", ")}`);
    const start = Date.now();
    try {
      const result = await this.inner.suggestNames(requests);
      debug.llmRoundtrip("suggestNames", {
        model: this.model,
        identifiers,
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmRoundtrip("suggestNames", {
        model: this.model,
        identifiers,
        error: error as Error,
        durationMs: Date.now() - start
      });
      throw error;
    }
  }

  async suggestAllNames(request: BatchRenameRequest): Promise<BatchRenameResponse> {
    if (!this.inner.suggestAllNames) {
      return { renames: {} };
    }

    const identifiers = request.identifiers;
    debug.log("llm", `suggestAllNames → identifiers: ${identifiers.join(", ")}`);
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
