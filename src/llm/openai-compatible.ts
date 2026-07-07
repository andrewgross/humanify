import OpenAI from "openai";
import type { ChatCompletion } from "openai/resources/chat/completions.js";
import { debug, type TokenUsage } from "../debug.js";
import {
  BATCH_RENAME_SYSTEM_PROMPT,
  buildBatchRenamePrompt,
  buildBatchRenameRetryPrompt
} from "./prompts.js";
import type {
  BatchRenameRequest,
  BatchRenameResponse,
  LLMConfig,
  LLMProvider
} from "./types.js";

function extractUsage(response: ChatCompletion): TokenUsage | undefined {
  const u = response.usage;
  if (!u) return undefined;
  return {
    promptTokens: u.prompt_tokens,
    completionTokens: u.completion_tokens,
    totalTokens: u.total_tokens,
    reasoningTokens:
      (
        u as unknown as {
          completion_tokens_details?: { reasoning_tokens?: number };
        }
      ).completion_tokens_details?.reasoning_tokens ?? undefined
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

function extractResponseHttp(
  rawResponse: Record<string, unknown>
): Record<string, unknown> {
  const responseHttp: Record<string, unknown> = {};
  if (rawResponse?.status) responseHttp.statusCode = rawResponse.status;
  if (rawResponse?.headers) {
    const headers: Record<string, string> = {};
    (
      rawResponse.headers as {
        forEach?: (fn: (value: string, key: string) => void) => void;
      }
    )?.forEach?.((value: string, key: string) => {
      headers[key] = value;
    });
    responseHttp.responseHeaders = headers;
  }
  return responseHttp;
}

function extractErrorHttp(
  error: Record<string, unknown>
): Record<string, unknown> {
  const responseHttp: Record<string, unknown> = {};
  if (error.status) responseHttp.statusCode = error.status;
  if (error.headers) {
    const headers: Record<string, string> = {};
    (
      error.headers as {
        forEach?: (fn: (value: string, key: string) => void) => void;
      }
    )?.forEach?.((value: string, key: string) => {
      headers[key] = value;
    });
    responseHttp.responseHeaders = headers;
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
    // Pass the raw model name through unchanged — the batch validator
    // classifies invalid/reserved names and drives a retry, rather than the
    // adapter silently sanitizing them here.
    renames[match[1]] = match[2];
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
  private reasoningEffort?: "low" | "medium" | "high";

  constructor(config: LLMConfig) {
    this.client = new OpenAI({
      baseURL: config.endpoint,
      apiKey: config.apiKey,
      timeout: config.timeout ?? 30000
    });
    this.model = config.model;
    this.maxTokens = config.maxTokens ?? 6000;
    // 0 (deterministic) — reproducible reruns are a cross-version diff requirement
    this.temperature = config.temperature ?? 0;
    this.reasoningEffort = config.reasoningEffort;
  }

  private buildBatchUserPrompt(request: BatchRenameRequest): string {
    if (request.userPrompt) return request.userPrompt;
    if (request.isRetry && request.failures) {
      return buildBatchRenameRetryPrompt(
        request.code,
        request.identifiers,
        request.usedNames,
        request.previousAttempt || {},
        request.failures,
        request.priorVersionCode,
        request.alreadyRenamed
      );
    }
    return buildBatchRenamePrompt(
      request.code,
      request.identifiers,
      request.usedNames,
      request.calleeSignatures,
      request.callsites,
      request.contextVars,
      request.priorVersionCode,
      request.priorVersionNames,
      request.alreadyRenamed
    );
  }

  /** Assemble the chat-completions request body for a rename call. */
  private buildRequestBody(
    systemPrompt: string,
    userPrompt: string
  ): Record<string, unknown> {
    const body: Record<string, unknown> = {
      model: this.model,
      messages: [
        { role: "system", content: systemPrompt },
        { role: "user", content: userPrompt }
      ],
      response_format: { type: "json_object" },
      temperature: this.temperature,
      max_tokens: this.maxTokens
    };
    if (this.reasoningEffort) body.reasoning_effort = this.reasoningEffort;
    return body;
  }

  async suggestAllNames(
    request: BatchRenameRequest
  ): Promise<BatchRenameResponse> {
    const systemPrompt = request.systemPrompt || BATCH_RENAME_SYSTEM_PROMPT;
    const userPrompt = this.buildBatchUserPrompt(request);

    const startTime = Date.now();
    const requestBody = this.buildRequestBody(systemPrompt, userPrompt);

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

    let response: ChatCompletion;
    try {
      response = (await this.client.chat.completions.create(
        requestBody as unknown as Parameters<
          typeof this.client.chat.completions.create
        >[0]
      )) as ChatCompletion;
    } catch (error: unknown) {
      const responseHttp = extractErrorHttp(error as Record<string, unknown>);
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
      const rawResponse = (response as unknown as Record<string, unknown>)
        ._response as Record<string, unknown>;
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
          // Raw pass-through: validity is decided downstream by the batch
          // validator so an invalid name can be rejected and retried.
          renames[oldName] = newName;
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
