import OpenAI from "openai";
import type { ChatCompletion } from "openai/resources/chat/completions.js";
import type { LLMContext } from "../analysis/types.js";
import { debug, type TokenUsage } from "../debug.js";
import {
  BATCH_RENAME_SYSTEM_PROMPT,
  buildBatchRenamePrompt,
  buildBatchRenameRetryPrompt,
  buildFunctionNamePrompt,
  buildFunctionRetryPrompt,
  buildRetryPrompt,
  buildUserPrompt,
  FUNCTION_NAME_SYSTEM_PROMPT,
  SYSTEM_PROMPT
} from "./prompts.js";
import type {
  BatchRenameRequest,
  BatchRenameResponse,
  LLMConfig,
  LLMProvider,
  NameSuggestion
} from "./types.js";
import { sanitizeIdentifier } from "./validation.js";

function extractUsage(response: ChatCompletion): TokenUsage | undefined {
  const u = response.usage;
  if (!u) return undefined;
  return {
    promptTokens: u.prompt_tokens,
    completionTokens: u.completion_tokens,
    totalTokens: u.total_tokens,
    reasoningTokens:
      (u as any).completion_tokens_details?.reasoning_tokens ?? undefined
  };
}

function usageToResult(
  batchUsage: TokenUsage | undefined
):
  | { totalTokens?: number; inputTokens?: number; outputTokens?: number }
  | undefined {
  if (!batchUsage) return undefined;
  return {
    totalTokens: batchUsage.totalTokens,
    inputTokens: batchUsage.promptTokens,
    outputTokens: batchUsage.completionTokens
  };
}

function extractResponseHttp(rawResponse: any): Record<string, any> {
  const responseHttp: Record<string, any> = {};
  if (rawResponse?.status) responseHttp.statusCode = rawResponse.status;
  if (rawResponse?.headers) {
    responseHttp.responseHeaders = {};
    rawResponse.headers.forEach?.((value: string, key: string) => {
      responseHttp.responseHeaders[key] = value;
    });
  }
  return responseHttp;
}

function extractErrorHttp(error: any): Record<string, any> {
  const responseHttp: Record<string, any> = {};
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
  return responseHttp;
}

function parseRenamesFromContent(content: string): Record<string, string> {
  const renames: Record<string, string> = {};
  const pattern = /"([^"]+)"\s*:\s*"([^"]+)"/g;
  let match: RegExpExecArray | null = pattern.exec(content);
  while (match !== null) {
    renames[match[1]] = sanitizeIdentifier(match[2]);
    match = pattern.exec(content);
  }
  return renames;
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
        {
          role: "user",
          content: buildRetryPrompt(currentName, rejectedName, context, reason)
        }
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
        {
          role: "user",
          content: buildFunctionNamePrompt(currentName, context)
        },
        { role: "assistant", content: JSON.stringify({ name: rejectedName }) },
        {
          role: "user",
          content: buildFunctionRetryPrompt(
            currentName,
            rejectedName,
            context,
            reason
          )
        }
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

  private buildBatchUserPrompt(request: BatchRenameRequest): string {
    if (request.userPrompt) return request.userPrompt;
    if (request.isRetry && request.failures) {
      return buildBatchRenameRetryPrompt(
        request.code,
        request.identifiers,
        request.usedNames,
        request.previousAttempt || {},
        request.failures
      );
    }
    return buildBatchRenamePrompt(
      request.code,
      request.identifiers,
      request.usedNames,
      request.calleeSignatures,
      request.callsites,
      request.contextVars
    );
  }

  async suggestAllNames(
    request: BatchRenameRequest
  ): Promise<BatchRenameResponse> {
    const systemPrompt = request.systemPrompt || BATCH_RENAME_SYSTEM_PROMPT;
    const userPrompt = this.buildBatchUserPrompt(request);

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

    let response: any;
    try {
      response = await this.client.chat.completions.create(requestBody as any);
    } catch (error: any) {
      const responseHttp = extractErrorHttp(error);
      debug.llmRoundtrip("suggestAllNames", {
        ...roundtripBase,
        error: error as Error,
        durationMs: Date.now() - startTime,
        responseHttp:
          Object.keys(responseHttp).length > 0 ? responseHttp : undefined
      });
      throw error;
    }

    const batchUsage = extractUsage(response);
    const finishReason = response.choices[0]?.finish_reason;
    const content = response.choices[0]?.message?.content;

    if (!content) {
      const rawResponse = (response as any)._response;
      const responseHttp = extractResponseHttp(rawResponse);

      debug.llmRoundtrip("suggestAllNames", {
        ...roundtripBase,
        rawResponse: `(empty) finish_reason=${finishReason ?? "null"} choices=${JSON.stringify(response.choices)}`,
        parsedResult: {},
        durationMs: Date.now() - startTime,
        usage: batchUsage,
        responseHttp:
          Object.keys(responseHttp).length > 0 ? responseHttp : undefined
      });
      return {
        renames: {},
        finishReason: finishReason ?? undefined,
        usage: usageToResult(batchUsage)
      };
    }

    try {
      const result = JSON.parse(content);
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

      return {
        renames,
        finishReason: finishReason ?? undefined,
        usage: usageToResult(batchUsage)
      };
    } catch {
      const renames = parseRenamesFromContent(content);

      debug.llmRoundtrip("suggestAllNames", {
        ...roundtripBase,
        rawResponse: content,
        parsedResult: {
          ...renames,
          _note: "Extracted from malformed JSON",
          _finishReason: finishReason
        },
        durationMs: Date.now() - startTime,
        usage: batchUsage
      });

      return {
        renames,
        finishReason: finishReason ?? undefined,
        usage: usageToResult(batchUsage)
      };
    }
  }
}
