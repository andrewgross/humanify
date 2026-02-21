import OpenAI from "openai";
import type { LLMContext } from "../analysis/types.js";
import type { LLMConfig, LLMProvider, NameSuggestion, BatchRenameRequest, BatchRenameResponse } from "./types.js";
import {
  SYSTEM_PROMPT,
  FUNCTION_NAME_SYSTEM_PROMPT,
  BATCH_RENAME_SYSTEM_PROMPT,
  buildUserPrompt,
  buildFunctionNamePrompt,
  buildRetryPrompt,
  buildFunctionRetryPrompt,
  buildBatchRenamePrompt,
  buildBatchRenameRetryPrompt
} from "./prompts.js";
import { sanitizeIdentifier } from "./validation.js";
import { debug, type TokenUsage } from "../debug.js";
import type { ChatCompletion } from "openai/resources/chat/completions.js";

function extractUsage(response: ChatCompletion): TokenUsage | undefined {
  const u = response.usage;
  if (!u) return undefined;
  return {
    promptTokens: u.prompt_tokens,
    completionTokens: u.completion_tokens,
    totalTokens: u.total_tokens,
    reasoningTokens: (u as any).completion_tokens_details?.reasoning_tokens ?? undefined,
  };
}

/**
 * LLM provider for any OpenAI-compatible API endpoint.
 *
 * Supports:
 * - OpenAI directly
 * - OpenRouter (Claude, Llama, etc.)
 * - Local inference servers (vLLM, llama.cpp server, Ollama)
 * - Any other OpenAI-compatible provider
 */
export class OpenAICompatibleProvider implements LLMProvider {
  private client: OpenAI;
  private model: string;
  private maxTokens: number;
  private temperature: number;

  constructor(config: LLMConfig) {
    this.client = new OpenAI({
      baseURL: config.endpoint,
      apiKey: config.apiKey,
      timeout: config.timeout ?? 30000
    });
    this.model = config.model;
    this.maxTokens = config.maxTokens ?? 2000;
    this.temperature = config.temperature ?? 0.3;
  }

  async suggestName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    const response = await this.client.chat.completions.create({
      model: this.model,
      messages: [
        { role: "system", content: SYSTEM_PROMPT },
        { role: "user", content: buildUserPrompt(currentName, context) }
      ],
      response_format: { type: "json_object" },
      temperature: this.temperature,
      max_tokens: this.maxTokens
    });

    const content = response.choices[0]?.message?.content;
    if (!content) {
      return { name: currentName, reasoning: "No response from LLM" };
    }

    try {
      const result = JSON.parse(content);
      return {
        name: sanitizeIdentifier(result.name || currentName),
        reasoning: result.reasoning,
        confidence: result.confidence
      };
    } catch {
      // If JSON parsing fails, try to extract identifier from raw response
      const match = content.match(/[a-zA-Z_$][a-zA-Z0-9_$]*/);
      return {
        name: match ? sanitizeIdentifier(match[0]) : currentName,
        reasoning: "Failed to parse JSON response"
      };
    }
  }

  async suggestFunctionName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    const response = await this.client.chat.completions.create({
      model: this.model,
      messages: [
        { role: "system", content: FUNCTION_NAME_SYSTEM_PROMPT },
        { role: "user", content: buildFunctionNamePrompt(currentName, context) }
      ],
      response_format: { type: "json_object" },
      temperature: this.temperature,
      max_tokens: this.maxTokens
    });

    const content = response.choices[0]?.message?.content;
    if (!content) {
      return { name: currentName, reasoning: "No response from LLM" };
    }

    try {
      const result = JSON.parse(content);
      return {
        name: sanitizeIdentifier(result.name || currentName),
        reasoning: result.reasoning,
        confidence: result.confidence
      };
    } catch {
      const match = content.match(/[a-zA-Z_$][a-zA-Z0-9_$]*/);
      return {
        name: match ? sanitizeIdentifier(match[0]) : currentName,
        reasoning: "Failed to parse JSON response"
      };
    }
  }

  async retrySuggestName(
    currentName: string,
    rejectedName: string,
    reason: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    // Use conversation history to show the LLM its rejected attempt
    const response = await this.client.chat.completions.create({
      model: this.model,
      messages: [
        { role: "system", content: SYSTEM_PROMPT },
        { role: "user", content: buildUserPrompt(currentName, context) },
        { role: "assistant", content: JSON.stringify({ name: rejectedName }) },
        { role: "user", content: buildRetryPrompt(currentName, rejectedName, context, reason) }
      ],
      response_format: { type: "json_object" },
      temperature: this.temperature,
      max_tokens: this.maxTokens
    });

    const content = response.choices[0]?.message?.content;
    if (!content) {
      return { name: currentName, reasoning: "No response from LLM on retry" };
    }

    try {
      const result = JSON.parse(content);
      return {
        name: sanitizeIdentifier(result.name || currentName),
        reasoning: result.reasoning,
        confidence: result.confidence
      };
    } catch {
      const match = content.match(/[a-zA-Z_$][a-zA-Z0-9_$]*/);
      return {
        name: match ? sanitizeIdentifier(match[0]) : currentName,
        reasoning: "Failed to parse JSON response on retry"
      };
    }
  }

  async retryFunctionName(
    currentName: string,
    rejectedName: string,
    reason: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    const response = await this.client.chat.completions.create({
      model: this.model,
      messages: [
        { role: "system", content: FUNCTION_NAME_SYSTEM_PROMPT },
        { role: "user", content: buildFunctionNamePrompt(currentName, context) },
        { role: "assistant", content: JSON.stringify({ name: rejectedName }) },
        { role: "user", content: buildFunctionRetryPrompt(currentName, rejectedName, context, reason) }
      ],
      response_format: { type: "json_object" },
      temperature: this.temperature,
      max_tokens: this.maxTokens
    });

    const content = response.choices[0]?.message?.content;
    if (!content) {
      return { name: currentName, reasoning: "No response from LLM on retry" };
    }

    try {
      const result = JSON.parse(content);
      return {
        name: sanitizeIdentifier(result.name || currentName),
        reasoning: result.reasoning,
        confidence: result.confidence
      };
    } catch {
      const match = content.match(/[a-zA-Z_$][a-zA-Z0-9_$]*/);
      return {
        name: match ? sanitizeIdentifier(match[0]) : currentName,
        reasoning: "Failed to parse JSON response on retry"
      };
    }
  }

  async suggestNames(
    requests: Array<{ name: string; context: LLMContext }>
  ): Promise<NameSuggestion[]> {
    // For providers that don't support true batching,
    // process sequentially with the same client
    const results: NameSuggestion[] = [];
    for (const req of requests) {
      const suggestion = await this.suggestName(req.name, req.context);
      results.push(suggestion);
    }
    return results;
  }

  async suggestAllNames(request: BatchRenameRequest): Promise<BatchRenameResponse> {
    const systemPrompt = request.systemPrompt || BATCH_RENAME_SYSTEM_PROMPT;
    const userPrompt = request.userPrompt
      ? request.userPrompt
      : request.isRetry && request.failures
        ? buildBatchRenameRetryPrompt(
            request.code,
            request.identifiers,
            request.usedNames,
            request.previousAttempt || {},
            request.failures
          )
        : buildBatchRenamePrompt(
            request.code,
            request.identifiers,
            request.usedNames,
            request.calleeSignatures,
            request.callsites,
            request.contextVars
          );

    const startTime = Date.now();
    const requestBody = {
      model: this.model,
      messages: [
        { role: "system", content: systemPrompt },
        { role: "user", content: userPrompt }
      ],
      response_format: { type: "json_object" },
      temperature: this.temperature,
      max_tokens: this.maxTokens * 3
    };

    const roundtripBase = {
      model: this.model,
      systemPrompt,
      userPrompt,
      identifiers: request.identifiers,
      requestHttp: {
        method: "POST",
        url: `${this.client.baseURL}/chat/completions`
      }
    };

    let response;
    try {
      response = await this.client.chat.completions.create(requestBody as any);
    } catch (error: any) {
      // Extract HTTP details from OpenAI SDK error
      const responseHttp: any = {};
      if (error.status) responseHttp.statusCode = error.status;
      if (error.headers) {
        responseHttp.responseHeaders = {};
        error.headers.forEach?.((value: string, key: string) => {
          responseHttp.responseHeaders[key] = value;
        });
      }
      if (error.error) {
        responseHttp.responseBody = JSON.stringify(error.error);
      }

      debug.llmRoundtrip("suggestAllNames", {
        ...roundtripBase,
        error: error as Error,
        durationMs: Date.now() - startTime,
        responseHttp: Object.keys(responseHttp).length > 0 ? responseHttp : undefined
      });
      throw error;
    }

    const batchUsage = extractUsage(response);
    const finishReason = response.choices[0]?.finish_reason;
    const content = response.choices[0]?.message?.content;
    if (!content) {
      // Extract HTTP metadata from the SDK response for debugging
      const responseHttp: any = {};
      const rawResponse = (response as any)._response;
      if (rawResponse?.status) responseHttp.statusCode = rawResponse.status;
      if (rawResponse?.headers) {
        responseHttp.responseHeaders = {};
        rawResponse.headers.forEach?.((value: string, key: string) => {
          responseHttp.responseHeaders[key] = value;
        });
      }

      debug.llmRoundtrip("suggestAllNames", {
        ...roundtripBase,
        rawResponse: `(empty) finish_reason=${finishReason ?? "null"} choices=${JSON.stringify(response.choices)}`,
        parsedResult: {},
        durationMs: Date.now() - startTime,
        usage: batchUsage,
        responseHttp: Object.keys(responseHttp).length > 0 ? responseHttp : undefined
      });
      return { renames: {}, finishReason: finishReason ?? undefined };
    }

    try {
      const result = JSON.parse(content);
      // Sanitize all returned names
      const renames: Record<string, string> = {};
      for (const [oldName, newName] of Object.entries(result)) {
        if (typeof newName === "string") {
          renames[oldName] = sanitizeIdentifier(newName);
        }
      }

      debug.llmRoundtrip("suggestAllNames", {
        ...roundtripBase,
        rawResponse: content,
        parsedResult: { ...renames, _finishReason: finishReason },
        durationMs: Date.now() - startTime,
        usage: batchUsage
      });

      return { renames, finishReason: finishReason ?? undefined };
    } catch {
      // Try to extract key-value pairs from malformed JSON
      const renames: Record<string, string> = {};
      const pattern = /"([^"]+)"\s*:\s*"([^"]+)"/g;
      let match;
      while ((match = pattern.exec(content)) !== null) {
        renames[match[1]] = sanitizeIdentifier(match[2]);
      }

      debug.llmRoundtrip("suggestAllNames", {
        ...roundtripBase,
        rawResponse: content,
        parsedResult: { ...renames, _note: "Extracted from malformed JSON", _finishReason: finishReason },
        durationMs: Date.now() - startTime,
        usage: batchUsage
      });

      return { renames, finishReason: finishReason ?? undefined };
    }
  }
}

/**
 * Creates an OpenAI provider with default OpenAI endpoint.
 */
export function createOpenAIProvider(
  apiKey: string,
  model = "gpt-4o-mini",
  options: Partial<LLMConfig> = {}
): OpenAICompatibleProvider {
  return new OpenAICompatibleProvider({
    endpoint: "https://api.openai.com/v1",
    apiKey,
    model,
    ...options
  });
}

/**
 * Creates an OpenRouter provider.
 */
export function createOpenRouterProvider(
  apiKey: string,
  model = "anthropic/claude-3-haiku",
  options: Partial<LLMConfig> = {}
): OpenAICompatibleProvider {
  return new OpenAICompatibleProvider({
    endpoint: "https://openrouter.ai/api/v1",
    apiKey,
    model,
    ...options
  });
}

/**
 * Creates an Ollama provider.
 */
export function createOllamaProvider(
  model = "llama3.1",
  host = "http://localhost:11434",
  options: Partial<LLMConfig> = {}
): OpenAICompatibleProvider {
  return new OpenAICompatibleProvider({
    endpoint: `${host}/v1`,
    apiKey: "ollama", // Ollama requires a non-empty key
    model,
    ...options
  });
}

/**
 * Creates a vLLM provider.
 */
export function createVLLMProvider(
  model: string,
  host = "http://localhost:8000",
  options: Partial<LLMConfig> = {}
): OpenAICompatibleProvider {
  return new OpenAICompatibleProvider({
    endpoint: `${host}/v1`,
    apiKey: "none",
    model,
    ...options
  });
}
