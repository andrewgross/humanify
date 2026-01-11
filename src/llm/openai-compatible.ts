import OpenAI from "openai";
import type { LLMContext } from "../analysis/types.js";
import type { LLMConfig, LLMProvider, NameSuggestion } from "./types.js";
import {
  SYSTEM_PROMPT,
  FUNCTION_NAME_SYSTEM_PROMPT,
  buildUserPrompt,
  buildFunctionNamePrompt,
  buildRetryPrompt,
  buildFunctionRetryPrompt
} from "./prompts.js";
import { sanitizeIdentifier } from "./validation.js";

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
    this.maxTokens = config.maxTokens ?? 100;
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
