/**
 * Wave-deterministic scheduling primitives for the rename processor.
 *
 * The free-running dispatch loop applies renames as LLM responses arrive, so
 * later prompts observe earlier completions' names — prompt content depends
 * on completion timing. Wave scheduling removes that: wave N is every pending
 * node whose dependencies settled in waves < N (the dispatch loop's own
 * readiness rule); all wave-N prompts read the frozen pre-wave AST state, and
 * every collected rename applies at the wave barrier in deterministic order
 * (graph-node iteration order, then binding order within a node). Prompt
 * bytes become f(input, prior, settled waves) — with the LLM response cache
 * on, reruns are byte-identical (docs/plan-wave-deterministic-context.md).
 *
 * This module holds the scheduler mechanics: wave membership (including the
 * deadlock-break tiers as deterministic wave steps), the rendezvous gate
 * tasks wait on, the deferred-rename entry collector, and the barrier
 * application algorithm. The processor supplies the entries' apply/bookkeep
 * closures.
 */

/** Structural subset of UnifiedGraph needed for wave membership. */
export interface WaveMembershipGraph {
  nodes: Map<string, unknown>;
  dependencies: Map<string, Set<string>>;
  scopeParentEdges: Set<string>;
}

/**
 * One wave of nodes, with the deadlock tier that produced it:
 * - tier 0: all dependencies settled (checkNodeReady semantics)
 * - tier 1: ready once scopeParent edges are ignored
 * - tier 2: force-break — every remaining pending node
 */
export interface WaveMembers {
  ids: string[];
  tier: 0 | 1 | 2;
}

/** True when every dependency of `id` is done (optionally relaxing scopeParent edges). */
export function waveNodeReady(
  graph: WaveMembershipGraph,
  id: string,
  doneIds: ReadonlySet<string>,
  ignoreScopeParent: boolean
): boolean {
  const deps = graph.dependencies.get(id);
  if (!deps) return true;
  for (const dep of deps) {
    if (doneIds.has(dep)) continue;
    if (ignoreScopeParent && graph.scopeParentEdges.has(`${id}->${dep}`))
      continue;
    return false;
  }
  return true;
}

/**
 * Compute the next wave's membership in graph iteration order, applying the
 * same deadlock-break tiers as the free-running loop when no node is ready.
 */
export function computeWaveMembers(
  graph: WaveMembershipGraph,
  pending: ReadonlySet<string>,
  doneIds: ReadonlySet<string>
): WaveMembers {
  if (pending.size === 0) return { ids: [], tier: 0 };
  const pick = (ignoreScopeParent: boolean): string[] => {
    const ids: string[] = [];
    for (const id of graph.nodes.keys()) {
      if (
        pending.has(id) &&
        waveNodeReady(graph, id, doneIds, ignoreScopeParent)
      )
        ids.push(id);
    }
    return ids;
  };
  const ready = pick(false);
  if (ready.length > 0) return { ids: ready, tier: 0 };
  const tier1 = pick(true);
  if (tier1.length > 0) return { ids: tier1, tier: 1 };
  const all: string[] = [];
  for (const id of graph.nodes.keys()) {
    if (pending.has(id)) all.push(id);
  }
  return { ids: all, tier: 2 };
}

/**
 * One deferred rename collected during a wave. Prompts were built against the
 * frozen pre-wave state; the entry applies at the barrier through the
 * processor-supplied `apply` closure (which routes into the validated rename
 * path) in deterministic (nodeIndex, phase, bindingIndex, seq) order.
 */
export interface WaveEntry {
  /** Position of the owning node in graph iteration order. */
  nodeIndex: number;
  /** Which per-node identifier list the entry came from (0 main, 1 shadowed). */
  phase: number;
  /** Position of the OLD name within that phase's identifier list. */
  bindingIndex: number;
  /** Collection sequence — a stable tiebreaker only. */
  seq: number;
  oldName: string;
  newName: string;
  /** Identity record (unrenamed bookkeeping) — applied verbatim, never rejected. */
  identity?: boolean;
  /**
   * Retry entries resolve terminally at the barrier: on rejection, try one
   * deterministic conflict-variant, then give up (no further retry seeds).
   */
  suffixOnReject?: boolean;
  /** Perform the real application (validated rename + bookkeeping). */
  apply(name: string): { applied: boolean; reason?: string };
  /** Live used-name view for barrier-time collision checks and suffixing. */
  liveUsedNames(): Set<string>;
  /** Called with the final applied name (differs from newName after suffixing). */
  onApplied?(finalName: string): void;
  /** Called on rejection (non-suffix entries) or terminal give-up (suffix entries). */
  onRejected?(reason: string, winnerOldName?: string): void;
}

/** A barrier rejection that becomes retry work for the next wave step. */
export interface WaveRejection {
  entry: WaveEntry;
  /** The old name of the entry that holds the contested name, when known. */
  winnerOldName?: string;
}

/** Deterministic barrier order: node, then phase, then binding, then sequence. */
export function sortWaveEntries(entries: WaveEntry[]): WaveEntry[] {
  return entries.sort(
    (a, b) =>
      a.nodeIndex - b.nodeIndex ||
      a.phase - b.phase ||
      a.bindingIndex - b.bindingIndex ||
      a.seq - b.seq
  );
}

/** Accumulates deferred entries during a wave; drains sorted at the barrier. */
export class WaveCollector {
  private entries: WaveEntry[] = [];
  private seq = 0;

  nextSeq(): number {
    return this.seq++;
  }

  add(entry: WaveEntry): void {
    this.entries.push(entry);
  }

  drain(): WaveEntry[] {
    const drained = this.entries;
    this.entries = [];
    return sortWaveEntries(drained);
  }
}

/**
 * Apply a barrier's entries in deterministic order.
 *
 * Every entry re-validates against LIVE state (earlier entries of the same
 * barrier included): first the node's used-name set, then the validated
 * rename path inside `apply`. The first entry claiming a name in a scope
 * wins; later claimants are rejected and returned for the caller to seed
 * next-step retries — except suffixOnReject entries, which resolve
 * terminally here (deterministic conflict-variant, then give up).
 *
 * `winners` maps applied new names to the old name that claimed them,
 * cumulatively across barriers, so rejections can carry the winning pair as
 * retry context.
 */
export function applyWaveBarrier(
  entries: WaveEntry[],
  winners: Map<string, string>,
  resolveConflictFn: (name: string, used: Set<string>) => string
): WaveRejection[] {
  const rejections: WaveRejection[] = [];
  for (const entry of sortWaveEntries(entries)) {
    if (entry.identity) {
      entry.apply(entry.oldName);
      continue;
    }
    applyOneEntry(entry, winners, resolveConflictFn, rejections);
  }
  return rejections;
}

function applyOneEntry(
  entry: WaveEntry,
  winners: Map<string, string>,
  resolveConflictFn: (name: string, used: Set<string>) => string,
  rejections: WaveRejection[]
): void {
  const taken = entry.liveUsedNames().has(entry.newName);
  const attempt = taken
    ? { applied: false, reason: "duplicate" }
    : entry.apply(entry.newName);
  if (attempt.applied) {
    winners.set(entry.newName, entry.oldName);
    entry.onApplied?.(entry.newName);
    return;
  }
  if (entry.suffixOnReject) {
    applySuffixFallback(entry, winners, resolveConflictFn);
    return;
  }
  const winnerOldName = winners.get(entry.newName);
  entry.onRejected?.(attempt.reason ?? "duplicate", winnerOldName);
  rejections.push({ entry, winnerOldName });
}

/** Terminal resolution for retry entries: suffixed variant or give up. */
function applySuffixFallback(
  entry: WaveEntry,
  winners: Map<string, string>,
  resolveConflictFn: (name: string, used: Set<string>) => string
): void {
  const variant = resolveConflictFn(entry.newName, entry.liveUsedNames());
  const attempt =
    variant && variant !== entry.newName
      ? entry.apply(variant)
      : { applied: false, reason: "no-alternative" };
  if (attempt.applied) {
    winners.set(variant, entry.oldName);
    entry.onApplied?.(variant);
    return;
  }
  entry.onRejected?.(attempt.reason ?? "duplicate", winners.get(entry.newName));
}

interface GateWaiter {
  order: number;
  compute: () => unknown;
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}

/**
 * Rendezvous barrier for one wave step's tasks.
 *
 * Each of the `total` tasks either finishes, or arrives (possibly several
 * rounds) to wait for the barrier. `settle()` resolves once every task is
 * waiting or finished — the caller then applies the collected entries and,
 * when waiters remain, calls `release()`, which runs each waiter's barrier
 * compute in ascending order-key order (still inside the barrier — safe for
 * AST mutation) and resumes the tasks with the results.
 */
export class WaveGate {
  private waiters: GateWaiter[] = [];
  private finished = 0;
  private notify: (() => void) | null = null;

  constructor(private readonly total: number) {}

  arrive<T>(order: number, compute: () => T): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      this.waiters.push({
        order,
        compute,
        resolve: resolve as (value: unknown) => void,
        reject
      });
      this.check();
    });
  }

  finish(): void {
    this.finished++;
    if (this.finished + this.waiters.length > this.total) {
      throw new Error(
        `WaveGate accounting broken: ${this.finished} finished + ` +
          `${this.waiters.length} waiting > ${this.total} tasks`
      );
    }
    this.check();
  }

  /** Resolves once all tasks are waiting or finished; true = waiters remain. */
  async settle(): Promise<boolean> {
    if (!this.allAccountedFor()) {
      await new Promise<void>((resolve) => {
        this.notify = resolve;
      });
    }
    return this.waiters.length > 0;
  }

  /** Run waiter computes in order-key order and resume the tasks. */
  release(): void {
    const waiters = [...this.waiters].sort((a, b) => a.order - b.order);
    this.waiters = [];
    for (const waiter of waiters) {
      try {
        waiter.resolve(waiter.compute());
      } catch (error) {
        waiter.reject(error);
      }
    }
  }

  private allAccountedFor(): boolean {
    return this.finished + this.waiters.length >= this.total;
  }

  private check(): void {
    if (this.allAccountedFor() && this.notify) {
      const cb = this.notify;
      this.notify = null;
      cb();
    }
  }
}
