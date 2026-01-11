import type { LLMContext } from "../analysis/types.js";

/**
 * Configuration for any OpenAI-compatible API endpoint.
 *
 * Supports:
 * - OpenAI directly
 * - OpenRouter (access to Claude, Llama, etc.)
 * - Local inference servers (vLLM, llama.cpp server, Ollama)
 * - Any other OpenAI-compatible provider
 */
export interface LLMConfig {
  /** Base URL, e.g., "https://api.openai.com/v1" */
  endpoint: string;

  /** API key (can be empty for local servers) */
  apiKey: string;

  /** Model identifier */
  model: string;

  /** Maximum tokens in response */
  maxTokens?: number;

  /** Temperature for generation (0-1) */
  temperature?: number;

  /** Request timeout in milliseconds */
  timeout?: number;
}

/**
 * Result from an LLM name suggestion request.
 */
export interface NameSuggestion {
  /** The suggested name */
  name: string;

  /** Optional reasoning from the LLM */
  reasoning?: string;

  /** Confidence score (0-1) if provided */
  confidence?: number;
}

/**
 * Provider interface for LLM name suggestions.
 * Implemented by OpenAI-compatible, local llama, etc.
 */
export interface LLMProvider {
  /**
   * Suggest a name for a single identifier.
   */
  suggestName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion>;

  /**
   * Suggest a name for a function.
   * May use different prompting than variable names.
   */
  suggestFunctionName?(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion>;

  /**
   * Batch name suggestions for efficiency (optional).
   * Some providers support batching for better throughput.
   */
  suggestNames?(
    requests: Array<{ name: string; context: LLMContext }>
  ): Promise<NameSuggestion[]>;
}

/**
 * Rate limiting configuration.
 */
export interface RateLimitConfig {
  /** Max parallel requests */
  maxConcurrent: number;

  /** Rate limit (requests per minute) */
  requestsPerMinute?: number;

  /** Number of retry attempts on failure */
  retryAttempts: number;

  /** Delay between retries in milliseconds */
  retryDelayMs: number;
}

/**
 * Cost estimate for processing a codebase.
 */
export interface CostEstimate {
  /** Total number of identifiers to rename */
  totalIdentifiers: number;

  /** Estimated tokens for all requests */
  estimatedTokens: number;

  /** Estimated cost in USD */
  estimatedCost: number;

  /** Model being used */
  model: string;
}

/**
 * Validation result for a suggested name.
 */
export interface ValidationResult {
  /** Whether the suggestion is valid */
  valid: boolean;

  /** Reason if invalid */
  reason?: string;
}
