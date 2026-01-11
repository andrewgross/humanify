import type { LLMContext } from "../analysis/types.js";
import type { LLMProvider, NameSuggestion } from "./types.js";

/**
 * Options for the fallback provider.
 */
export interface FallbackOptions {
  /** Whether to log warnings when a provider fails */
  logWarnings?: boolean;
}

/**
 * Provider that tries multiple LLM providers in sequence.
 * If one fails, it falls back to the next.
 *
 * Useful for:
 * - Redundancy (primary + backup providers)
 * - Cost optimization (try cheap provider first)
 * - Mixed local/remote (try local first, fall back to cloud)
 */
export class FallbackProvider implements LLMProvider {
  private providers: LLMProvider[];
  private options: FallbackOptions;

  constructor(providers: LLMProvider[], options: FallbackOptions = {}) {
    if (providers.length === 0) {
      throw new Error("FallbackProvider requires at least one provider");
    }
    this.providers = providers;
    this.options = options;
  }

  async suggestName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    let lastError: unknown;

    for (const provider of this.providers) {
      try {
        return await provider.suggestName(currentName, context);
      } catch (error) {
        lastError = error;
        if (this.options.logWarnings) {
          console.warn(
            `Provider failed, trying next: ${
              error instanceof Error ? error.message : String(error)
            }`
          );
        }
      }
    }

    // All providers failed - return original name as fallback
    if (this.options.logWarnings) {
      console.warn("All providers failed, returning original name");
    }
    return {
      name: currentName,
      reasoning: `All providers failed: ${
        lastError instanceof Error ? lastError.message : String(lastError)
      }`
    };
  }

  async suggestFunctionName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    let lastError: unknown;

    for (const provider of this.providers) {
      try {
        if (provider.suggestFunctionName) {
          return await provider.suggestFunctionName(currentName, context);
        }
        return await provider.suggestName(currentName, context);
      } catch (error) {
        lastError = error;
        if (this.options.logWarnings) {
          console.warn(
            `Provider failed, trying next: ${
              error instanceof Error ? error.message : String(error)
            }`
          );
        }
      }
    }

    return {
      name: currentName,
      reasoning: `All providers failed: ${
        lastError instanceof Error ? lastError.message : String(lastError)
      }`
    };
  }

  async retrySuggestName(
    currentName: string,
    rejectedName: string,
    reason: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    let lastError: unknown;

    for (const provider of this.providers) {
      try {
        if (provider.retrySuggestName) {
          return await provider.retrySuggestName(
            currentName,
            rejectedName,
            reason,
            context
          );
        }
        // Fallback: re-call suggestName with rejected name added to used set
        const updatedContext = {
          ...context,
          usedIdentifiers: new Set([...context.usedIdentifiers, rejectedName])
        };
        return await provider.suggestName(currentName, updatedContext);
      } catch (error) {
        lastError = error;
        if (this.options.logWarnings) {
          console.warn(
            `Provider failed on retry, trying next: ${
              error instanceof Error ? error.message : String(error)
            }`
          );
        }
      }
    }

    return {
      name: currentName,
      reasoning: `All providers failed on retry: ${
        lastError instanceof Error ? lastError.message : String(lastError)
      }`
    };
  }

  async retryFunctionName(
    currentName: string,
    rejectedName: string,
    reason: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    let lastError: unknown;

    for (const provider of this.providers) {
      try {
        if (provider.retryFunctionName) {
          return await provider.retryFunctionName(
            currentName,
            rejectedName,
            reason,
            context
          );
        }
        // Fallback: try retrySuggestName or suggestFunctionName
        if (provider.retrySuggestName) {
          return await provider.retrySuggestName(
            currentName,
            rejectedName,
            reason,
            context
          );
        }
        const updatedContext = {
          ...context,
          usedIdentifiers: new Set([...context.usedIdentifiers, rejectedName])
        };
        if (provider.suggestFunctionName) {
          return await provider.suggestFunctionName(currentName, updatedContext);
        }
        return await provider.suggestName(currentName, updatedContext);
      } catch (error) {
        lastError = error;
        if (this.options.logWarnings) {
          console.warn(
            `Provider failed on function retry, trying next: ${
              error instanceof Error ? error.message : String(error)
            }`
          );
        }
      }
    }

    return {
      name: currentName,
      reasoning: `All providers failed on function retry: ${
        lastError instanceof Error ? lastError.message : String(lastError)
      }`
    };
  }

  async suggestNames(
    requests: Array<{ name: string; context: LLMContext }>
  ): Promise<NameSuggestion[]> {
    // Try each provider for the whole batch
    for (const provider of this.providers) {
      try {
        if (provider.suggestNames) {
          return await provider.suggestNames(requests);
        }
        // Fall back to individual requests
        const results: NameSuggestion[] = [];
        for (const req of requests) {
          const suggestion = await provider.suggestName(req.name, req.context);
          results.push(suggestion);
        }
        return results;
      } catch (error) {
        if (this.options.logWarnings) {
          console.warn(
            `Provider failed for batch, trying next: ${
              error instanceof Error ? error.message : String(error)
            }`
          );
        }
      }
    }

    // All failed - return original names
    return requests.map((req) => ({
      name: req.name,
      reasoning: "All providers failed"
    }));
  }
}

/**
 * Creates a fallback provider from multiple providers.
 */
export function withFallback(
  providers: LLMProvider[],
  options?: FallbackOptions
): FallbackProvider {
  return new FallbackProvider(providers, options);
}
