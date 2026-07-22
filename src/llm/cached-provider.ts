/**
 * Disk response cache for LLM rename calls.
 *
 * Motivation (measured 2026-07-22, experiments/034 README): identical
 * pipeline runs agree to ~±115 noise lines within one serving session, but
 * byte-identical code drifted ±2.7k lines across sessions — the local vLLM
 * server's serving state, not our batch order, is the dominant
 * nondeterminism. Caching responses by request content makes every repeated
 * prompt deterministic across sessions, makes eval/walk reruns nearly free,
 * and sharpens A/B probes: prompts shared between candidate and control
 * short-circuit to identical answers, so measured differences isolate to
 * the prompts a change actually touched.
 *
 * Keying: a stable serialization of every semantic request field (sets
 * sorted, object keys sorted) plus the model parameters. Two runs that
 * assemble the same request in a different internal order share one key —
 * strictly MORE deterministic than the live server. Responses with zero
 * renames are never cached (empty content is a serving hiccup the retry
 * path handles); errors pass through uncached.
 *
 * Writes are atomic (temp file + rename), so concurrent lanes racing on
 * one key at worst both write the same content.
 */
import { createHash } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { debug } from "../debug.js";
import type {
  BatchRenameRequest,
  BatchRenameResponse,
  LLMProvider
} from "./types.js";

/** The model parameters that shape a response, mirrored from LLMConfig. */
export interface CacheKeyParams {
  model: string;
  temperature?: number;
  maxTokens?: number;
  reasoningEffort?: string;
}

interface CacheEntry {
  v: 1;
  renames: Record<string, string>;
  finishReason?: string;
  /** The live call's usage, kept for reference; hits report zero spend. */
  originalUsage?: BatchRenameResponse["usage"];
}

/** JSON with recursively sorted keys — a canonical, order-free encoding. */
function canonicalJson(value: unknown): string {
  return JSON.stringify(sortValue(value));
}

function sortValue(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortValue);
  if (value instanceof Set) return [...value].map(String).sort();
  if (value instanceof Map) {
    return sortValue(Object.fromEntries(value.entries()));
  }
  if (typeof value === "object" && value !== null) {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(value).sort()) {
      out[key] = sortValue((value as Record<string, unknown>)[key]);
    }
    return out;
  }
  return value;
}

export class CachedLLMProvider implements LLMProvider {
  readonly stats = { hits: 0, misses: 0 };

  constructor(
    private readonly inner: LLMProvider,
    private readonly dir: string,
    private readonly params: CacheKeyParams
  ) {
    fs.mkdirSync(dir, { recursive: true });
  }

  private keyOf(request: BatchRenameRequest): string {
    const material = canonicalJson({
      cacheVersion: 1,
      params: this.params,
      request: {
        code: request.code,
        identifiers: request.identifiers,
        usedNames: request.usedNames,
        calleeSignatures: request.calleeSignatures,
        callsites: request.callsites,
        contextVars: request.contextVars,
        priorVersionCode: request.priorVersionCode,
        priorVersionNames: request.priorVersionNames,
        priorNameHints: request.priorNameHints,
        alreadyRenamed: request.alreadyRenamed,
        isRetry: request.isRetry,
        previousAttempt: request.previousAttempt,
        failures: request.failures,
        promptBody: request.promptBody,
        userPrompt: request.userPrompt,
        systemPrompt: request.systemPrompt
      }
    });
    return createHash("sha256").update(material).digest("hex");
  }

  private pathOf(key: string): string {
    return path.join(this.dir, key.slice(0, 2), `${key.slice(2)}.json`);
  }

  private read(key: string): CacheEntry | null {
    try {
      const raw = fs.readFileSync(this.pathOf(key), "utf8");
      const entry = JSON.parse(raw) as CacheEntry;
      if (entry.v !== 1 || typeof entry.renames !== "object") return null;
      return entry;
    } catch {
      return null; // missing or corrupt — treat as a miss
    }
  }

  private write(key: string, entry: CacheEntry): void {
    const target = this.pathOf(key);
    try {
      fs.mkdirSync(path.dirname(target), { recursive: true });
      const tmp = `${target}.${process.pid}.tmp`;
      fs.writeFileSync(tmp, JSON.stringify(entry));
      fs.renameSync(tmp, target);
    } catch (err) {
      // A cache write failure must never fail the run.
      debug.log("processor", `llm-cache: write failed for ${key}: ${err}`);
    }
  }

  async suggestAllNames(
    request: BatchRenameRequest
  ): Promise<BatchRenameResponse> {
    const key = this.keyOf(request);
    const cached = this.read(key);
    if (cached) {
      this.stats.hits++;
      return {
        renames: { ...cached.renames },
        finishReason: cached.finishReason,
        // Zero spend: metrics reflect what THIS run cost.
        usage: { totalTokens: 0, inputTokens: 0, outputTokens: 0 }
      };
    }
    this.stats.misses++;
    const response = await this.inner.suggestAllNames(request);
    if (Object.keys(response.renames).length > 0) {
      this.write(key, {
        v: 1,
        renames: response.renames,
        finishReason: response.finishReason,
        originalUsage: response.usage
      });
    }
    return response;
  }
}
