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
 * Request for batch renaming all identifiers in a function at once.
 */
export interface BatchRenameRequest {
  /** Current function code */
  code: string;

  /** Identifiers that need renaming */
  identifiers: string[];

  /** Names already in use (must avoid) */
  usedNames: Set<string>;

  /** Callee signatures for context */
  calleeSignatures: Array<{ name: string; params: string[] }>;

  /** Call sites for context */
  callsites: string[];

  /** Whether this is a retry attempt */
  isRetry?: boolean;

  /** Previous attempt (for retry context) */
  previousAttempt?: Record<string, string>;

  /** Failure reasons from previous attempt */
  failures?: {
    duplicates: string[];
    invalid: string[];
    missing: string[];
    unchanged: string[];
  };

  /** Override system prompt (used for module-level renaming) */
  systemPrompt?: string;

  /** Override user prompt — bypasses buildBatchRenamePrompt when set */
  userPrompt?: string;

  /** Parent-scope variable declarations for read-only context */
  contextVars?: string[];

  /** Prior-version humanified code for close-matched functions */
  priorVersionCode?: string;

  /** Already-renamed identifiers from earlier rounds (for retry context) */
  alreadyRenamed?: Record<string, string>;
}

/**
 * Response from batch rename request.
 */
export interface BatchRenameResponse {
  /** Mapping from original name to suggested new name */
  renames: Record<string, string>;

  /** Finish reason from the LLM response (e.g., "stop", "length") */
  finishReason?: string;

  /** Token usage from the LLM response */
  usage?: { totalTokens?: number; inputTokens?: number; outputTokens?: number };
}

/**
 * Provider interface for LLM name suggestions.
 * Implemented by OpenAI-compatible, local llama, etc.
 */
export interface LLMProvider {
  /**
   * Suggest names for ALL identifiers in a function at once.
   * This allows the LLM to understand the function semantically
   * and name related variables consistently.
   */
  suggestAllNames(request: BatchRenameRequest): Promise<BatchRenameResponse>;
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
