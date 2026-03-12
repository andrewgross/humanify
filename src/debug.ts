/**
 * Debug logging for tracing LLM calls, prompts, responses, and rename operations.
 *
 * Enable with -vv (verbosity level 2).
 */

import { verbose } from "./verbose.js";

interface HttpDetails {
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

interface DebugLogger {
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

function formatTokenUsage(u: TokenUsage): string {
  const parts: string[] = [];
  if (u.promptTokens !== undefined) parts.push(`prompt=${u.promptTokens}`);
  if (u.completionTokens !== undefined)
    parts.push(`completion=${u.completionTokens}`);
  if (u.reasoningTokens !== undefined)
    parts.push(`reasoning=${u.reasoningTokens}`);
  if (u.totalTokens !== undefined) parts.push(`total=${u.totalTokens}`);
  return `Tokens: ${parts.join(", ")}`;
}

function writeHttpRequestDetails(
  write: (text: string) => void,
  http: HttpDetails
): void {
  write("\n--- HTTP REQUEST ---");
  if (http.method && http.url) {
    write(`${http.method} ${http.url}`);
  }
  if (http.headers) {
    write("Headers:");
    for (const [key, value] of Object.entries(http.headers)) {
      const displayValue =
        key.toLowerCase().includes("authorization") ||
        key.toLowerCase().includes("api-key")
          ? "[REDACTED]"
          : value;
      write(`  ${key}: ${displayValue}`);
    }
  }
  if (http.requestBody) {
    write("Body:");
    write(indent(JSON.stringify(http.requestBody, null, 2), 2));
  }
}

function writeHttpResponseDetails(
  write: (text: string) => void,
  http: HttpDetails
): void {
  write("\n--- HTTP RESPONSE ---");
  if (http.statusCode !== undefined) {
    write(`Status: ${http.statusCode}`);
  }
  if (http.responseHeaders) {
    write("Headers:");
    for (const [key, value] of Object.entries(http.responseHeaders)) {
      write(`  ${key}: ${value}`);
    }
  }
  if (http.responseBody) {
    write("Body:");
    write(indent(truncate(http.responseBody, 5000), 2));
  }
}

function writeIdentifierMeta(
  write: (text: string) => void,
  params: {
    model?: string;
    currentName?: string;
    identifiers?: string[];
  }
): void {
  if (params.model) write(`Model: ${params.model}`);
  if (params.currentName) write(`Current name: ${params.currentName}`);
  if (params.identifiers)
    write(`Identifiers: ${params.identifiers.join(", ")}`);
}

function writePromptSections(
  write: (text: string) => void,
  params: { systemPrompt?: string; userPrompt?: string }
): void {
  if (params.systemPrompt) {
    write("\n--- SYSTEM PROMPT ---");
    write(truncate(params.systemPrompt, 2000));
  }
  if (params.userPrompt) {
    write("\n--- USER PROMPT ---");
    write(params.userPrompt);
  }
}

function writeRequestParams(
  write: (text: string) => void,
  params: {
    model?: string;
    currentName?: string;
    identifiers?: string[];
    systemPrompt?: string;
    userPrompt?: string;
    http?: HttpDetails;
  }
): void {
  writeIdentifierMeta(write, params);
  if (params.http) {
    writeHttpRequestDetails(write, params.http);
  }
  writePromptSections(write, params);
}

function writeErrorSection(write: (text: string) => void, error: Error): void {
  write("\n--- ERROR ---");
  write(error.message);
  if (error.stack) write(error.stack);
}

function writeResponseBody(
  write: (text: string) => void,
  params: {
    rawResponse?: string;
    parsedResult?: unknown;
    error?: Error;
    http?: HttpDetails;
  }
): void {
  if (params.http) {
    writeHttpResponseDetails(write, params.http);
  }
  if (params.rawResponse) {
    write("\n--- RAW RESPONSE ---");
    write(truncate(params.rawResponse, 2000));
  }
  if (params.parsedResult) {
    write("\n--- PARSED RESULT ---");
    write(JSON.stringify(params.parsedResult, null, 2));
  }
  if (params.error) {
    writeErrorSection(write, params.error);
  }
}

function hasTokenData(u: TokenUsage): boolean {
  return (
    u.promptTokens !== undefined ||
    u.completionTokens !== undefined ||
    u.reasoningTokens !== undefined ||
    u.totalTokens !== undefined
  );
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

    writeRequestParams((t) => this.write(t), params);

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
      this.write(formatTokenUsage(params.usage));
    }

    writeResponseBody((t) => this.write(t), params);

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

    if (params.usage && hasTokenData(params.usage)) {
      this.write(formatTokenUsage(params.usage));
    }

    writeIdentifierMeta((t) => this.write(t), params);

    if (params.requestHttp) {
      writeHttpRequestDetails((t) => this.write(t), params.requestHttp);
    }

    writePromptSections((t) => this.write(t), params);

    if (params.responseHttp) {
      writeHttpResponseDetails((t) => this.write(t), params.responseHttp);
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
      writeErrorSection((t) => this.write(t), params.error);
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
