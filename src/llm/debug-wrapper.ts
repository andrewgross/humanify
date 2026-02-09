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
    debug.llmRequest("suggestName", {
      model: this.model,
      currentName,
      userPrompt: `Function code:\n${context.functionCode}\n\nCallees: ${context.calleeSignatures.map(c => c.name).join(", ")}\nCall sites: ${context.callsites.join(", ")}\nUsed names: ${[...context.usedIdentifiers].slice(0, 20).join(", ")}...`
    });

    const start = Date.now();
    try {
      const result = await this.inner.suggestName(currentName, context);
      debug.llmResponse("suggestName", {
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmResponse("suggestName", {
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

    debug.llmRequest("suggestFunctionName", {
      model: this.model,
      currentName,
      userPrompt: `Function code:\n${context.functionCode}`
    });

    const start = Date.now();
    try {
      const result = await this.inner.suggestFunctionName(currentName, context);
      debug.llmResponse("suggestFunctionName", {
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmResponse("suggestFunctionName", {
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

    debug.llmRequest("retrySuggestName", {
      model: this.model,
      currentName,
      userPrompt: `Rejected: "${rejectedName}" (${reason})\n\nFunction code:\n${context.functionCode}`
    });

    const start = Date.now();
    try {
      const result = await this.inner.retrySuggestName(currentName, rejectedName, reason, context);
      debug.llmResponse("retrySuggestName", {
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmResponse("retrySuggestName", {
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

    debug.llmRequest("retryFunctionName", {
      model: this.model,
      currentName,
      userPrompt: `Rejected: "${rejectedName}" (${reason})`
    });

    const start = Date.now();
    try {
      const result = await this.inner.retryFunctionName(currentName, rejectedName, reason, context);
      debug.llmResponse("retryFunctionName", {
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmResponse("retryFunctionName", {
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

    debug.llmRequest("suggestNames", {
      model: this.model,
      identifiers: requests.map(r => r.name)
    });

    const start = Date.now();
    try {
      const result = await this.inner.suggestNames(requests);
      debug.llmResponse("suggestNames", {
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmResponse("suggestNames", {
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

    debug.llmRequest("suggestAllNames", {
      model: this.model,
      identifiers: request.identifiers,
      userPrompt: `Code:\n${request.code}\n\nIdentifiers: ${request.identifiers.join(", ")}\nUsed names: ${[...request.usedNames].slice(0, 30).join(", ")}...\nIs retry: ${request.isRetry}\n${request.failures ? `Failures: duplicates=${request.failures.duplicates.join(",")}, invalid=${request.failures.invalid.join(",")}` : ""}`
    });

    const start = Date.now();
    try {
      const result = await this.inner.suggestAllNames(request);
      debug.llmResponse("suggestAllNames", {
        parsedResult: result,
        durationMs: Date.now() - start
      });
      return result;
    } catch (error) {
      debug.llmResponse("suggestAllNames", {
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
