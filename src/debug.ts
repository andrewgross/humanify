/**
 * Debug logging for tracing LLM calls, prompts, responses, and rename operations.
 *
 * Enable with -vv (verbosity level 2).
 */

import { verbose } from "./verbose.js";

export interface HttpDetails {
  /** HTTP method */
  method?: string;
  /** Request URL */
  url?: string;
  /** Request headers (sensitive values redacted) */
  headers?: Record<string, string>;
  /** Request body */
  requestBody?: unknown;
  /** Response status code */
  statusCode?: number;
  /** Response headers */
  responseHeaders?: Record<string, string>;
  /** Response body (raw) */
  responseBody?: string;
}

export interface TokenUsage {
  promptTokens?: number;
  completionTokens?: number;
  totalTokens?: number;
  reasoningTokens?: number;
}

export interface DebugLogger {
  enabled: boolean;

  /** Log an LLM request with its prompts */
  llmRequest(
    method: string,
    params: {
      model?: string;
      systemPrompt?: string;
      userPrompt?: string;
      identifiers?: string[];
      currentName?: string;
      http?: HttpDetails;
    }
  ): void;

  /** Log an LLM response */
  llmResponse(
    method: string,
    params: {
      rawResponse?: string;
      parsedResult?: unknown;
      error?: Error;
      durationMs?: number;
      http?: HttpDetails;
      usage?: TokenUsage;
    }
  ): void;

  /** Log a rename operation */
  rename(params: {
    functionId: string;
    oldName: string;
    newName: string;
    wasRetry?: boolean;
    attemptNumber?: number;
  }): void;

  /** Log validation results */
  validation(params: {
    valid: Record<string, string>;
    duplicates: string[];
    invalid: string[];
    missing: string[];
  }): void;

  /** Log a complete LLM roundtrip (request + response together) */
  llmRoundtrip(
    method: string,
    params: {
      model?: string;
      systemPrompt?: string;
      userPrompt?: string;
      identifiers?: string[];
      currentName?: string;
      requestHttp?: HttpDetails;
      rawResponse?: string;
      parsedResult?: unknown;
      error?: Error;
      durationMs?: number;
      responseHttp?: HttpDetails;
      usage?: TokenUsage;
    }
  ): void;

  /** Log rename fallback information with a searchable prefix */
  renameFallback(info: {
    functionId: string;
    identifier: string;
    suggestedName?: string;
    rejectionReason?: string;
    fallbackResult?: string;
    context?: string;
    round?: number;
  }): void;

  /** Log queue state for dispatch/completion/wait events */
  queueState(params: {
    ready: number;
    processing: number;
    pending: number;
    done: number;
    total: number;
    inFlightLLM: number;
    event:
      | "dispatch"
      | "completion"
      | "waiting-on-llm"
      | "waiting-on-deps"
      | "deadlock-break";
    detail?: string;
  }): void;

  /** Log general debug info */
  log(category: string, message: string, data?: unknown): void;

  /** Redirect all debug output to a custom writer */
  setOutput(writer: (text: string) => void): void;

  /** Reset output to default (console.log) */
  resetOutput(): void;
}

function formatTimestamp(): string {
  return new Date().toISOString().replace(/T/, " ").replace(/\..+/, "");
}

function truncate(str: string, maxLen: number): string {
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen - 3) + "...";
}

function indent(text: string, spaces: number): string {
  const prefix = " ".repeat(spaces);
  return text
    .split("\n")
    .map((line) => prefix + line)
    .join("\n");
}

class DebugLoggerImpl implements DebugLogger {
  private _output: (text: string) => void = (text) => console.log(text);

  get enabled() {
    return verbose.level >= 2;
  }

  setOutput(writer: (text: string) => void): void {
    this._output = writer;
  }

  resetOutput(): void {
    this._output = (text) => console.log(text);
  }

  private write(text: string): void {
    this._output(text);
  }

  private print(category: string, ...args: unknown[]): void {
    if (!this.enabled) return;
    const ts = formatTimestamp();
    this.write(`\n[${ts}] [DEBUG:${category}]`);
    for (const arg of args) {
      if (typeof arg === "string") {
        this.write(indent(arg, 2));
      } else {
        this.write(indent(JSON.stringify(arg, null, 2), 2));
      }
    }
  }

  llmRequest(
    method: string,
    params: {
      model?: string;
      systemPrompt?: string;
      userPrompt?: string;
      identifiers?: string[];
      currentName?: string;
      http?: HttpDetails;
    }
  ): void {
    if (!this.enabled) return;

    const ts = formatTimestamp();
    this.write(`\n${"=".repeat(80)}`);
    this.write(`[${ts}] [LLM REQUEST] ${method}`);
    this.write("=".repeat(80));

    if (params.model) {
      this.write(`Model: ${params.model}`);
    }
    if (params.currentName) {
      this.write(`Current name: ${params.currentName}`);
    }
    if (params.identifiers) {
      this.write(`Identifiers: ${params.identifiers.join(", ")}`);
    }

    // HTTP details
    if (params.http) {
      this.write("\n--- HTTP REQUEST ---");
      if (params.http.method && params.http.url) {
        this.write(`${params.http.method} ${params.http.url}`);
      }
      if (params.http.headers) {
        this.write("Headers:");
        for (const [key, value] of Object.entries(params.http.headers)) {
          // Redact sensitive headers
          const displayValue =
            key.toLowerCase().includes("authorization") ||
            key.toLowerCase().includes("api-key")
              ? "[REDACTED]"
              : value;
          this.write(`  ${key}: ${displayValue}`);
        }
      }
      if (params.http.requestBody) {
        this.write("Body:");
        this.write(indent(JSON.stringify(params.http.requestBody, null, 2), 2));
      }
    }

    if (params.systemPrompt) {
      this.write("\n--- SYSTEM PROMPT ---");
      this.write(truncate(params.systemPrompt, 2000));
    }

    if (params.userPrompt) {
      this.write("\n--- USER PROMPT ---");
      this.write(params.userPrompt);
    }

    this.write("-".repeat(80));
  }

  llmResponse(
    method: string,
    params: {
      rawResponse?: string;
      parsedResult?: unknown;
      error?: Error;
      durationMs?: number;
      http?: HttpDetails;
      usage?: TokenUsage;
    }
  ): void {
    if (!this.enabled) return;

    const ts = formatTimestamp();
    const status = params.error ? "ERROR" : "SUCCESS";
    this.write(`\n[${ts}] [LLM RESPONSE] ${method} - ${status}`);

    if (params.durationMs !== undefined) {
      this.write(`Duration: ${params.durationMs.toFixed(0)}ms`);
    }

    if (params.usage) {
      const u = params.usage;
      const parts: string[] = [];
      if (u.promptTokens !== undefined) parts.push(`prompt=${u.promptTokens}`);
      if (u.completionTokens !== undefined)
        parts.push(`completion=${u.completionTokens}`);
      if (u.reasoningTokens !== undefined)
        parts.push(`reasoning=${u.reasoningTokens}`);
      if (u.totalTokens !== undefined) parts.push(`total=${u.totalTokens}`);
      this.write(`Tokens: ${parts.join(", ")}`);
    }

    // HTTP details
    if (params.http) {
      this.write("\n--- HTTP RESPONSE ---");
      if (params.http.statusCode !== undefined) {
        this.write(`Status: ${params.http.statusCode}`);
      }
      if (params.http.responseHeaders) {
        this.write("Headers:");
        for (const [key, value] of Object.entries(
          params.http.responseHeaders
        )) {
          this.write(`  ${key}: ${value}`);
        }
      }
      if (params.http.responseBody) {
        this.write("Body:");
        this.write(indent(truncate(params.http.responseBody, 5000), 2));
      }
    }

    if (params.rawResponse) {
      this.write("\n--- RAW RESPONSE ---");
      this.write(truncate(params.rawResponse, 2000));
    }

    if (params.parsedResult) {
      this.write("\n--- PARSED RESULT ---");
      this.write(JSON.stringify(params.parsedResult, null, 2));
    }

    if (params.error) {
      this.write("\n--- ERROR ---");
      this.write(params.error.message);
      if (params.error.stack) {
        this.write(params.error.stack);
      }
    }

    this.write("=".repeat(80));
  }

  llmRoundtrip(
    method: string,
    params: {
      model?: string;
      systemPrompt?: string;
      userPrompt?: string;
      identifiers?: string[];
      currentName?: string;
      requestHttp?: HttpDetails;
      rawResponse?: string;
      parsedResult?: unknown;
      error?: Error;
      durationMs?: number;
      responseHttp?: HttpDetails;
      usage?: TokenUsage;
    }
  ): void {
    if (!this.enabled) return;

    const ts = formatTimestamp();
    const status = params.error ? "ERROR" : "SUCCESS";
    const duration =
      params.durationMs !== undefined
        ? ` (${params.durationMs.toFixed(0)}ms)`
        : "";

    this.write(`\n${"=".repeat(80)}`);
    this.write(`[${ts}] [LLM] ${method} - ${status}${duration}`);

    if (params.usage) {
      const u = params.usage;
      const parts: string[] = [];
      if (u.promptTokens !== undefined) parts.push(`prompt=${u.promptTokens}`);
      if (u.completionTokens !== undefined)
        parts.push(`completion=${u.completionTokens}`);
      if (u.reasoningTokens !== undefined)
        parts.push(`reasoning=${u.reasoningTokens}`);
      if (u.totalTokens !== undefined) parts.push(`total=${u.totalTokens}`);
      if (parts.length > 0) this.write(`Tokens: ${parts.join(", ")}`);
    }

    if (params.model) this.write(`Model: ${params.model}`);
    if (params.currentName) this.write(`Current name: ${params.currentName}`);
    if (params.identifiers)
      this.write(`Identifiers: ${params.identifiers.join(", ")}`);

    // Request HTTP details
    if (params.requestHttp) {
      this.write("\n--- HTTP REQUEST ---");
      if (params.requestHttp.method && params.requestHttp.url) {
        this.write(`${params.requestHttp.method} ${params.requestHttp.url}`);
      }
      if (params.requestHttp.headers) {
        this.write("Headers:");
        for (const [key, value] of Object.entries(params.requestHttp.headers)) {
          const displayValue =
            key.toLowerCase().includes("authorization") ||
            key.toLowerCase().includes("api-key")
              ? "[REDACTED]"
              : value;
          this.write(`  ${key}: ${displayValue}`);
        }
      }
      if (params.requestHttp.requestBody) {
        this.write("Body:");
        this.write(
          indent(JSON.stringify(params.requestHttp.requestBody, null, 2), 2)
        );
      }
    }

    // Request prompts
    if (params.systemPrompt) {
      this.write("\n--- SYSTEM PROMPT ---");
      this.write(truncate(params.systemPrompt, 2000));
    }
    if (params.userPrompt) {
      this.write("\n--- USER PROMPT ---");
      this.write(params.userPrompt);
    }

    // Response
    if (params.responseHttp) {
      this.write("\n--- HTTP RESPONSE ---");
      if (params.responseHttp.statusCode !== undefined) {
        this.write(`Status: ${params.responseHttp.statusCode}`);
      }
      if (params.responseHttp.responseHeaders) {
        this.write("Headers:");
        for (const [key, value] of Object.entries(
          params.responseHttp.responseHeaders
        )) {
          this.write(`  ${key}: ${value}`);
        }
      }
      if (params.responseHttp.responseBody) {
        this.write("Body:");
        this.write(indent(truncate(params.responseHttp.responseBody, 5000), 2));
      }
    }

    if (params.rawResponse) {
      this.write("\n--- RAW RESPONSE ---");
      this.write(truncate(params.rawResponse, 2000));
    }
    if (params.parsedResult) {
      this.write("\n--- PARSED ---");
      this.write(JSON.stringify(params.parsedResult, null, 2));
    }
    if (params.error) {
      this.write("\n--- ERROR ---");
      this.write(params.error.message);
      if (params.error.stack) this.write(params.error.stack);
    }

    this.write("=".repeat(80));
  }

  rename(params: {
    functionId: string;
    oldName: string;
    newName: string;
    wasRetry?: boolean;
    attemptNumber?: number;
  }): void {
    if (!this.enabled) return;

    const retry = params.wasRetry ? ` (retry #${params.attemptNumber})` : "";
    this.write(
      `[RENAME] ${params.oldName} -> ${params.newName}${retry} [fn:${params.functionId}]`
    );
  }

  validation(params: {
    valid: Record<string, string>;
    duplicates: string[];
    invalid: string[];
    missing: string[];
  }): void {
    if (!this.enabled) return;

    this.write("\n[VALIDATION RESULT]");
    this.write(`  Valid: ${JSON.stringify(params.valid)}`);
    if (params.duplicates.length > 0) {
      this.write(`  Duplicates: ${params.duplicates.join(", ")}`);
    }
    if (params.invalid.length > 0) {
      this.write(`  Invalid: ${params.invalid.join(", ")}`);
    }
    if (params.missing.length > 0) {
      this.write(`  Missing: ${params.missing.join(", ")}`);
    }
  }

  renameFallback(info: {
    functionId: string;
    identifier: string;
    suggestedName?: string;
    rejectionReason?: string;
    fallbackResult?: string;
    context?: string;
    round?: number;
  }): void {
    if (!this.enabled) return;

    const ts = formatTimestamp();
    const parts = [
      `[${ts}] [RENAME-FALLBACK] fn=${info.functionId} id=${info.identifier}`
    ];
    if (info.round !== undefined) parts.push(`round=${info.round}`);
    if (info.suggestedName) parts.push(`suggested=${info.suggestedName}`);
    if (info.rejectionReason) parts.push(`reason=${info.rejectionReason}`);
    if (info.fallbackResult) parts.push(`result=${info.fallbackResult}`);
    this.write(parts.join(" "));
    if (info.context) {
      this.write(indent(truncate(info.context, 500), 2));
    }
  }

  queueState(params: {
    ready: number;
    processing: number;
    pending: number;
    done: number;
    total: number;
    inFlightLLM: number;
    event:
      | "dispatch"
      | "completion"
      | "waiting-on-llm"
      | "waiting-on-deps"
      | "deadlock-break";
    detail?: string;
  }): void {
    if (!this.enabled) return;

    const ts = formatTimestamp();
    const parts = [
      `[${ts}] [QUEUE-STATE] ${params.event}`,
      `ready=${params.ready} processing=${params.processing} pending=${params.pending} done=${params.done}/${params.total}`,
      `inflight-llm=${params.inFlightLLM}`
    ];
    if (params.detail) parts.push(params.detail);
    this.write(parts.join(" | "));
  }

  log(category: string, message: string, data?: unknown): void {
    if (data !== undefined) {
      this.print(category, message, data);
    } else {
      this.print(category, message);
    }
  }
}

export const debug: DebugLogger = new DebugLoggerImpl();
