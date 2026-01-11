# LLM Integration

## Overview

Support any OpenAI-compatible API endpoint, making it easy to use:
- OpenAI directly
- OpenRouter (access to Claude, Llama, etc.)
- Local inference servers (vLLM, llama.cpp server, Ollama)
- Any other OpenAI-compatible provider

## Unified Provider Interface

```typescript
// src/llm/types.ts

interface LLMConfig {
  // Required
  endpoint: string;   // Base URL, e.g., "https://api.openai.com/v1"
  apiKey: string;     // API key (can be empty for local)
  model: string;      // Model identifier

  // Optional
  maxTokens?: number;
  temperature?: number;
  timeout?: number;
}

interface NameSuggestion {
  name: string;
  reasoning?: string;
  confidence?: number;
}

interface LLMProvider {
  suggestName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion>;

  suggestFunctionName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion>;

  // For batch efficiency (some providers support batching)
  suggestNames?(
    requests: Array<{ name: string; context: LLMContext }>
  ): Promise<NameSuggestion[]>;
}
```

## OpenAI-Compatible Implementation

```typescript
// src/llm/openai-compatible.ts

import OpenAI from 'openai';

export class OpenAICompatibleProvider implements LLMProvider {
  private client: OpenAI;
  private model: string;

  constructor(config: LLMConfig) {
    this.client = new OpenAI({
      baseURL: config.endpoint,
      apiKey: config.apiKey,
    });
    this.model = config.model;
  }

  async suggestName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    const response = await this.client.chat.completions.create({
      model: this.model,
      messages: [
        { role: 'system', content: SYSTEM_PROMPT },
        { role: 'user', content: buildUserPrompt(currentName, context) }
      ],
      response_format: { type: 'json_object' },
      temperature: 0.3,
      max_tokens: 100
    });

    const result = JSON.parse(response.choices[0].message.content);
    return {
      name: sanitizeIdentifier(result.name),
      reasoning: result.reasoning,
      confidence: result.confidence
    };
  }
}
```

## Local LLM Support

For local models via node-llama-cpp:

```typescript
// src/llm/local-llama.ts

import { getLlama, LlamaChatSession } from 'node-llama-cpp';

export class LocalLlamaProvider implements LLMProvider {
  private session: LlamaChatSession;

  static async create(modelPath: string): Promise<LocalLlamaProvider> {
    const llama = await getLlama();
    const model = await llama.loadModel({ modelPath });
    const context = await model.createContext();
    const session = new LlamaChatSession({ context });

    const provider = new LocalLlamaProvider();
    provider.session = session;
    return provider;
  }

  async suggestName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    const prompt = buildUserPrompt(currentName, context);

    // Use grammar to constrain output to valid identifiers
    const response = await this.session.prompt(prompt, {
      grammar: IDENTIFIER_GRAMMAR,
      temperature: 0.5
    });

    return {
      name: sanitizeIdentifier(response.trim()),
    };
  }
}
```

## Prompts

### System Prompt

```typescript
const SYSTEM_PROMPT = `You are an expert JavaScript developer helping to deobfuscate minified code.

Your task is to suggest meaningful, descriptive names for minified identifiers based on their usage context.

Guidelines:
- Use camelCase for variables and functions
- Use PascalCase for classes and constructors
- Be specific but concise (e.g., "userId" not "theIdOfTheUser")
- Consider the function's callees - if it calls "fetchUser", it might be "loadUserProfile"
- Avoid generic names like "data", "result", "temp" unless truly appropriate
- Never use reserved words (if, for, class, etc.)

Respond with JSON: { "name": "suggestedName", "reasoning": "brief explanation" }`;
```

### User Prompt Builder

```typescript
function buildUserPrompt(currentName: string, context: LLMContext): string {
  let prompt = `Suggest a better name for the identifier "${currentName}" in this code:\n\n`;

  prompt += '```javascript\n' + context.functionCode + '\n```\n\n';

  if (context.calleeSignatures.length > 0) {
    prompt += 'This function calls these (already named) functions:\n';
    for (const callee of context.calleeSignatures) {
      prompt += `- ${callee.name}(${callee.params.join(', ')})\n`;
    }
    prompt += '\n';
  }

  if (context.callsites.length > 0) {
    prompt += 'This function is called like:\n';
    for (const site of context.callsites.slice(0, 3)) {
      prompt += `- ${site}\n`;
    }
    prompt += '\n';
  }

  prompt += `Names already in use (avoid these): ${[...context.usedIdentifiers].join(', ')}\n`;

  return prompt;
}
```

### Grammar for Local Models (GBNF)

```typescript
// Constrains output to valid JavaScript identifiers
const IDENTIFIER_GRAMMAR = `
root ::= [a-zA-Z_$] [a-zA-Z0-9_$]{1,30}
`;
```

## CLI Configuration

```bash
# OpenAI (default endpoint)
humanify input.min.js \
  --model gpt-4o-mini \
  --api-key $OPENAI_API_KEY

# OpenRouter
humanify input.min.js \
  --endpoint https://openrouter.ai/api/v1 \
  --model anthropic/claude-3-haiku \
  --api-key $OPENROUTER_API_KEY

# Local vLLM server
humanify input.min.js \
  --endpoint http://localhost:8000/v1 \
  --model local-model \
  --api-key none

# Local llama.cpp
humanify input.min.js \
  --local \
  --model ~/.humanify/models/phi-3-mini.gguf

# Ollama
humanify input.min.js \
  --endpoint http://localhost:11434/v1 \
  --model llama3.1 \
  --api-key ollama
```

## Rate Limiting and Retries

```typescript
interface RateLimitConfig {
  maxConcurrent: number;      // Max parallel requests
  requestsPerMinute?: number; // Rate limit
  retryAttempts: number;      // Retries on failure
  retryDelayMs: number;       // Delay between retries
}

class RateLimitedProvider implements LLMProvider {
  private limiter: Bottleneck;
  private inner: LLMProvider;

  constructor(inner: LLMProvider, config: RateLimitConfig) {
    this.inner = inner;
    this.limiter = new Bottleneck({
      maxConcurrent: config.maxConcurrent,
      minTime: config.requestsPerMinute
        ? 60000 / config.requestsPerMinute
        : 0
    });
  }

  async suggestName(name: string, context: LLMContext): Promise<NameSuggestion> {
    return this.limiter.schedule(() =>
      retry(
        () => this.inner.suggestName(name, context),
        { attempts: 3, delay: 1000 }
      )
    );
  }
}
```

## Fallback Chain

Support multiple providers with fallback:

```typescript
class FallbackProvider implements LLMProvider {
  private providers: LLMProvider[];

  async suggestName(name: string, context: LLMContext): Promise<NameSuggestion> {
    for (const provider of this.providers) {
      try {
        return await provider.suggestName(name, context);
      } catch (error) {
        console.warn(`Provider failed, trying next: ${error.message}`);
      }
    }
    // All failed - return original name
    return { name, reasoning: 'All providers failed' };
  }
}
```

## Cost Estimation

Before processing, estimate API costs:

```typescript
interface CostEstimate {
  totalIdentifiers: number;
  estimatedTokens: number;
  estimatedCost: number;  // USD
  model: string;
}

function estimateCost(
  functions: FunctionNode[],
  model: string
): CostEstimate {
  const totalIdentifiers = functions.reduce(
    (sum, fn) => sum + getOwnBindings(fn.path).length,
    0
  );

  // Rough estimate: ~500 tokens per identifier (prompt + response)
  const estimatedTokens = totalIdentifiers * 500;

  const costPer1kTokens = MODEL_COSTS[model] || 0.001;
  const estimatedCost = (estimatedTokens / 1000) * costPer1kTokens;

  return { totalIdentifiers, estimatedTokens, estimatedCost, model };
}
```

## Response Validation

Validate LLM responses before applying:

```typescript
function validateSuggestion(
  suggestion: NameSuggestion,
  context: LLMContext
): { valid: boolean; reason?: string } {
  const { name } = suggestion;

  // Must be valid JS identifier
  if (!isValidIdentifier(name)) {
    return { valid: false, reason: 'Invalid identifier syntax' };
  }

  // Must not be reserved word
  if (RESERVED_WORDS.has(name)) {
    return { valid: false, reason: 'Reserved word' };
  }

  // Must not conflict with existing bindings
  if (context.usedIdentifiers.has(name)) {
    return { valid: false, reason: 'Name already in use' };
  }

  // Sanity check: not too long
  if (name.length > 50) {
    return { valid: false, reason: 'Name too long' };
  }

  return { valid: true };
}
```

## Metrics and Observability

Track detailed metrics throughout processing to enable progress display and bottleneck identification.

### Metrics Types

```typescript
// src/llm/metrics.ts

interface LLMMetrics {
  totalCalls: number;        // Total LLM calls made
  inFlightCalls: number;     // Currently active requests
  completedCalls: number;    // Successfully completed
  failedCalls: number;       // Failed after retries exhausted
  totalTokens?: number;      // Token usage if available
  avgResponseTimeMs: number; // Average response latency
}

interface FunctionMetrics {
  total: number;             // Total functions to process
  completed: number;         // Functions fully processed
  inProgress: number;        // Currently being processed
  pending: number;           // Waiting for dependencies
  ready: number;             // Ready to process (deps satisfied)
}

interface ProcessingMetrics {
  llm: LLMMetrics;
  functions: FunctionMetrics;
  startTime: number;
  elapsedMs: number;
  estimatedRemainingMs?: number;
}
```

### Metrics Tracker

```typescript
class MetricsTracker {
  // Callback for real-time updates (throttled)
  constructor(options: {
    onMetrics?: (metrics: ProcessingMetrics) => void;
    throttleMs?: number;  // Default 100ms
  });

  // LLM call tracking
  llmCallStart(): () => void;  // Returns done() callback
  llmCallFailed(): void;
  recordTokens(tokens: number): void;

  // Function tracking
  setFunctionTotal(total: number): void;
  functionStarted(): void;
  functionCompleted(): void;
  functionsReady(count: number): void;

  // Get current snapshot
  getMetrics(): ProcessingMetrics;
}
```

### Integration Points

The metrics tracker integrates at two levels:

1. **RateLimitedProvider** - Tracks LLM call metrics:
```typescript
const provider = withRateLimit(
  innerProvider,
  { maxConcurrent: 10 },
  metrics  // Pass metrics tracker
);
```

2. **RenameProcessor** - Tracks function processing metrics:
```typescript
const processor = new RenameProcessor(ast);
await processor.processAll(functions, provider, {
  concurrency: 10,
  metrics  // Pass metrics tracker
});
```

### Display Formats

```typescript
// Compact single-line (for terminal status updates)
formatMetricsCompact(metrics: ProcessingMetrics): string;
// Output: "[45%] 23/51 functions | LLM: 3 in-flight | ETA: 1m 24s"

// Full multi-line summary
formatMetrics(metrics: ProcessingMetrics): string;
// Output:
// Functions: 23/51 done | 3 processing | 5 ready | 20 pending
// LLM Calls: 89 done | 3 in-flight | 0 failed | avg 342ms
// Time: 2m 15s elapsed | ETA: 1m 24s
```

### Why These Metrics Matter

| Metric | Purpose |
|--------|---------|
| `llm.inFlightCalls` | Identify if concurrency is saturated or API is slow |
| `llm.avgResponseTimeMs` | Detect API latency issues |
| `llm.failedCalls` | Alert on API errors or rate limiting |
| `functions.ready` | Understand dependency bottlenecks |
| `functions.pending` | See how much work is blocked |
| `estimatedRemainingMs` | Set user expectations |
