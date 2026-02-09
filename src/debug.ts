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
  llmRequest(method: string, params: {
    model?: string;
    systemPrompt?: string;
    userPrompt?: string;
    identifiers?: string[];
    currentName?: string;
    http?: HttpDetails;
  }): void;

  /** Log an LLM response */
  llmResponse(method: string, params: {
    rawResponse?: string;
    parsedResult?: unknown;
    error?: Error;
    durationMs?: number;
    http?: HttpDetails;
    usage?: TokenUsage;
  }): void;

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

  /** Log general debug info */
  log(category: string, message: string, data?: unknown): void;
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
  return text.split("\n").map(line => prefix + line).join("\n");
}

class DebugLoggerImpl implements DebugLogger {
  get enabled() { return verbose.level >= 2; }

  private print(category: string, ...args: unknown[]): void {
    if (!this.enabled) return;
    const ts = formatTimestamp();
    console.log(`\n[${ts}] [DEBUG:${category}]`);
    for (const arg of args) {
      if (typeof arg === "string") {
        console.log(indent(arg, 2));
      } else {
        console.log(indent(JSON.stringify(arg, null, 2), 2));
      }
    }
  }

  llmRequest(method: string, params: {
    model?: string;
    systemPrompt?: string;
    userPrompt?: string;
    identifiers?: string[];
    currentName?: string;
    http?: HttpDetails;
  }): void {
    if (!this.enabled) return;

    const ts = formatTimestamp();
    console.log(`\n${"=".repeat(80)}`);
    console.log(`[${ts}] [LLM REQUEST] ${method}`);
    console.log("=".repeat(80));

    if (params.model) {
      console.log(`Model: ${params.model}`);
    }
    if (params.currentName) {
      console.log(`Current name: ${params.currentName}`);
    }
    if (params.identifiers) {
      console.log(`Identifiers: ${params.identifiers.join(", ")}`);
    }

    // HTTP details
    if (params.http) {
      console.log("\n--- HTTP REQUEST ---");
      if (params.http.method && params.http.url) {
        console.log(`${params.http.method} ${params.http.url}`);
      }
      if (params.http.headers) {
        console.log("Headers:");
        for (const [key, value] of Object.entries(params.http.headers)) {
          // Redact sensitive headers
          const displayValue = key.toLowerCase().includes("authorization") ||
            key.toLowerCase().includes("api-key")
            ? "[REDACTED]"
            : value;
          console.log(`  ${key}: ${displayValue}`);
        }
      }
      if (params.http.requestBody) {
        console.log("Body:");
        console.log(indent(JSON.stringify(params.http.requestBody, null, 2), 2));
      }
    }

    if (params.systemPrompt) {
      console.log("\n--- SYSTEM PROMPT ---");
      console.log(truncate(params.systemPrompt, 2000));
    }

    if (params.userPrompt) {
      console.log("\n--- USER PROMPT ---");
      console.log(params.userPrompt);
    }

    console.log("-".repeat(80));
  }

  llmResponse(method: string, params: {
    rawResponse?: string;
    parsedResult?: unknown;
    error?: Error;
    durationMs?: number;
    http?: HttpDetails;
    usage?: TokenUsage;
  }): void {
    if (!this.enabled) return;

    const ts = formatTimestamp();
    const status = params.error ? "ERROR" : "SUCCESS";
    console.log(`\n[${ts}] [LLM RESPONSE] ${method} - ${status}`);

    if (params.durationMs !== undefined) {
      console.log(`Duration: ${params.durationMs.toFixed(0)}ms`);
    }

    if (params.usage) {
      const u = params.usage;
      const parts: string[] = [];
      if (u.promptTokens !== undefined) parts.push(`prompt=${u.promptTokens}`);
      if (u.completionTokens !== undefined) parts.push(`completion=${u.completionTokens}`);
      if (u.reasoningTokens !== undefined) parts.push(`reasoning=${u.reasoningTokens}`);
      if (u.totalTokens !== undefined) parts.push(`total=${u.totalTokens}`);
      console.log(`Tokens: ${parts.join(", ")}`);
    }

    // HTTP details
    if (params.http) {
      console.log("\n--- HTTP RESPONSE ---");
      if (params.http.statusCode !== undefined) {
        console.log(`Status: ${params.http.statusCode}`);
      }
      if (params.http.responseHeaders) {
        console.log("Headers:");
        for (const [key, value] of Object.entries(params.http.responseHeaders)) {
          console.log(`  ${key}: ${value}`);
        }
      }
      if (params.http.responseBody) {
        console.log("Body:");
        console.log(indent(truncate(params.http.responseBody, 5000), 2));
      }
    }

    if (params.rawResponse) {
      console.log("\n--- RAW RESPONSE ---");
      console.log(truncate(params.rawResponse, 2000));
    }

    if (params.parsedResult) {
      console.log("\n--- PARSED RESULT ---");
      console.log(JSON.stringify(params.parsedResult, null, 2));
    }

    if (params.error) {
      console.log("\n--- ERROR ---");
      console.log(params.error.message);
      if (params.error.stack) {
        console.log(params.error.stack);
      }
    }

    console.log("=".repeat(80));
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
    console.log(`[RENAME] ${params.oldName} -> ${params.newName}${retry} [fn:${params.functionId}]`);
  }

  validation(params: {
    valid: Record<string, string>;
    duplicates: string[];
    invalid: string[];
    missing: string[];
  }): void {
    if (!this.enabled) return;

    console.log("\n[VALIDATION RESULT]");
    console.log(`  Valid: ${JSON.stringify(params.valid)}`);
    if (params.duplicates.length > 0) {
      console.log(`  Duplicates: ${params.duplicates.join(", ")}`);
    }
    if (params.invalid.length > 0) {
      console.log(`  Invalid: ${params.invalid.join(", ")}`);
    }
    if (params.missing.length > 0) {
      console.log(`  Missing: ${params.missing.join(", ")}`);
    }
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
