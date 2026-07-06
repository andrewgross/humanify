import { debug } from "../debug.js";
import { buildRenameResponseInstruction } from "../llm/prompts.js";
import type {
  BatchRenameRequest,
  BatchRenameResponse,
  LLMProvider
} from "../llm/types.js";

/**
 * Collects collision-retry requests from concurrently processing
 * functions/lanes and dispatches them as shared LLM calls.
 *
 * During the retry tail each function used to grind its own serial loop of
 * tiny 1-3 identifier calls. Submissions arriving within a short window
 * merge into one call per pool (same system prompt), keeping lanes full and
 * amortizing per-call overhead. Groups whose identifier names overlap stay
 * in separate calls — the flat JSON response cannot distinguish two `e`s
 * from different scopes.
 */

/** Structural subset of MetricsTracker the batcher reports through. */
interface BatcherMetrics {
  llmCallStart(): (() => void) | undefined;
  recordTokens(total: number, input?: number, output?: number): void;
}

export interface RetryBatcherOptions {
  /** Collection window from the first pending submission (default 25ms). */
  windowMs?: number;
  /** Flush a pool once its pending identifier count reaches this (default 10). */
  maxBatch?: number;
}

interface PendingEntry {
  request: BatchRenameRequest;
  resolve: (response: BatchRenameResponse) => void;
  reject: (error: unknown) => void;
}

const DEFAULT_WINDOW_MS = 25;
const DEFAULT_MAX_BATCH = 10;

export class RetryBatcher {
  private pools = new Map<string, PendingEntry[]>();
  private timers = new Map<string, NodeJS.Timeout>();
  private windowMs: number;
  private maxBatch: number;

  constructor(
    private llm: LLMProvider,
    private metrics?: BatcherMetrics,
    options?: RetryBatcherOptions
  ) {
    this.windowMs = options?.windowMs ?? DEFAULT_WINDOW_MS;
    this.maxBatch = options?.maxBatch ?? DEFAULT_MAX_BATCH;
  }

  /**
   * Queue a retry request; resolves with the renames for exactly this
   * request's identifiers. Rejects when the underlying LLM call throws —
   * callers keep their existing containment handling.
   */
  submit(request: BatchRenameRequest): Promise<BatchRenameResponse> {
    const poolKey = request.systemPrompt ?? "";
    return new Promise<BatchRenameResponse>((resolve, reject) => {
      const pool = this.pools.get(poolKey) ?? [];
      pool.push({ request, resolve, reject });
      this.pools.set(poolKey, pool);

      const pendingIds = pool.reduce(
        (n, e) => n + e.request.identifiers.length,
        0
      );
      if (pendingIds >= this.maxBatch) {
        this.flushPool(poolKey);
      } else if (!this.timers.has(poolKey)) {
        this.timers.set(
          poolKey,
          setTimeout(() => this.flushPool(poolKey), this.windowMs)
        );
      }
    });
  }

  /** Snapshot and dispatch everything pending for a pool. */
  private flushPool(poolKey: string): void {
    const timer = this.timers.get(poolKey);
    if (timer) {
      clearTimeout(timer);
      this.timers.delete(poolKey);
    }
    const entries = this.pools.get(poolKey);
    if (!entries || entries.length === 0) return;
    this.pools.delete(poolKey);

    for (const group of partitionIntoCalls(entries, this.maxBatch)) {
      void this.dispatchCall(group);
    }
  }

  /** Issue one LLM call for a group and route results back per entry. */
  private async dispatchCall(group: PendingEntry[]): Promise<void> {
    const request =
      group.length === 1 ? group[0].request : buildMergedRequest(group);
    if (group.length > 1) {
      debug.log(
        "retry-batcher",
        `merged ${group.length} retry groups (${request.identifiers.length} identifiers) into one call`
      );
    }

    let response: BatchRenameResponse;
    try {
      const done = this.metrics?.llmCallStart();
      response = await this.llm.suggestAllNames(request);
      done?.();
      this.metrics?.recordTokens(
        response.usage?.totalTokens ?? 0,
        response.usage?.inputTokens,
        response.usage?.outputTokens
      );
    } catch (error) {
      for (const entry of group) entry.reject(error);
      return;
    }

    for (const entry of group) {
      const renames: Record<string, string> = {};
      for (const id of entry.request.identifiers) {
        if (response.renames[id]) renames[id] = response.renames[id];
      }
      // Usage intentionally omitted: recorded once above, per actual call.
      entry.resolve({ renames, finishReason: response.finishReason });
    }
  }
}

/**
 * Greedy partition of pending entries into call groups: identifier names in
 * a group must be pairwise disjoint (flat JSON response), the group must
 * stay within the identifier budget, and only entries carrying a promptBody
 * can merge with others.
 */
function partitionIntoCalls(
  entries: PendingEntry[],
  maxBatch: number
): PendingEntry[][] {
  const groups: PendingEntry[][] = [];
  const groupIds: Array<Set<string>> = [];

  for (const entry of entries) {
    const ids = entry.request.identifiers;
    const target = entry.request.promptBody
      ? findFittingGroup(groups, groupIds, ids, maxBatch)
      : -1;
    if (target === -1) {
      groups.push([entry]);
      groupIds.push(new Set(ids));
    } else {
      groups[target].push(entry);
      for (const id of ids) groupIds[target].add(id);
    }
  }

  return groups;
}

/** Index of the first group this entry can merge into, or -1. */
function findFittingGroup(
  groups: PendingEntry[][],
  groupIds: Array<Set<string>>,
  ids: string[],
  maxBatch: number
): number {
  for (let i = 0; i < groups.length; i++) {
    // Mergeable target: every member must carry a promptBody too
    if (!groups[i].every((e) => e.request.promptBody)) continue;
    if (groupIds[i].size + ids.length > maxBatch) continue;
    if (ids.some((id) => groupIds[i].has(id))) continue;
    return i;
  }
  return -1;
}

/** Combine a group's prompt bodies into one request with a shared tail. */
function buildMergedRequest(group: PendingEntry[]): BatchRenameRequest {
  const allIds: string[] = [];
  const usedNames = new Set<string>();
  const sections: string[] = [];

  for (let i = 0; i < group.length; i++) {
    const req = group[i].request;
    allIds.push(...req.identifiers);
    for (const name of req.usedNames) usedNames.add(name);
    sections.push(`--- Retry group ${i + 1} ---\n${req.promptBody}`);
  }

  const header =
    "Several independent retry groups follow. They come from different " +
    "scopes of the same file — consider each group separately, but respond " +
    "with ONE JSON object covering ALL identifiers.\n\n";

  return {
    code: "",
    identifiers: allIds,
    usedNames,
    calleeSignatures: [],
    callsites: [],
    systemPrompt: group[0].request.systemPrompt,
    userPrompt: `${header}${sections.join("\n\n")}\n\n${buildRenameResponseInstruction(allIds)}`,
    isRetry: true
  };
}
