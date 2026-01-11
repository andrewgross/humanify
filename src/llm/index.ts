// Types
export type {
  LLMConfig,
  LLMProvider,
  NameSuggestion,
  RateLimitConfig,
  CostEstimate,
  ValidationResult
} from "./types.js";

// Prompts
export {
  SYSTEM_PROMPT,
  FUNCTION_NAME_SYSTEM_PROMPT,
  buildUserPrompt,
  buildFunctionNamePrompt,
  IDENTIFIER_GRAMMAR,
  JSON_NAME_GRAMMAR,
  JSON_NAME_REASONING_GRAMMAR
} from "./prompts.js";

// Validation
export {
  RESERVED_WORDS,
  isValidIdentifier,
  validateSuggestion,
  sanitizeIdentifier,
  resolveConflict
} from "./validation.js";

// Providers
export {
  OpenAICompatibleProvider,
  createOpenAIProvider,
  createOpenRouterProvider,
  createOllamaProvider,
  createVLLMProvider
} from "./openai-compatible.js";

export {
  LocalLlamaProvider,
  createLocalProvider,
  type LocalLlamaConfig
} from "./local-llama.js";

// Rate limiting
export { RateLimitedProvider, withRateLimit } from "./rate-limiter.js";

// Fallback
export {
  FallbackProvider,
  withFallback,
  type FallbackOptions
} from "./fallback.js";

// Cost estimation
export {
  MODEL_COSTS,
  estimateCost,
  formatCostEstimate,
  isLocalModel
} from "./cost.js";

// Metrics
export {
  MetricsTracker,
  formatMetrics,
  formatMetricsCompact,
  type LLMMetrics,
  type FunctionMetrics,
  type ProcessingMetrics,
  type MetricsCallback
} from "./metrics.js";
