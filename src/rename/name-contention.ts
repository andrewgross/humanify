/**
 * Name-contention recorder (exp063 Task 0).
 *
 * Design axiom (Andrew, 2026-08-13): a request to reuse a name that is
 * already taken proves an error somewhere — a wrong holder, a duplicate
 * heir, or a corrupted vote. exp061 measured 86 collision-decorated hint
 * landings per hop and every one was invisible: the collision ladder
 * resolved silently and nothing counted it. This recorder makes each
 * contention a diagnostics event so the error classes stay measurable
 * run over run.
 *
 * exp063's ceilings said the ledger-visible cost of ADJUDICATING these
 * today is ~2 lines (the decorated landings sit on lines whose churn has
 * upstream causes) — so this ships as an INSTRUMENT, not a lever. The
 * holder's identity and vote history are recoverable offline by joining
 * `requested` against the strategy trails.
 *
 * Off by default; a module singleton (same shape as `strategyTrail`)
 * enabled exactly when --diagnostics is set.
 */

export interface NameContentionEvent {
  /** The name the claimant asked for (hint, vote, or LLM suggestion). */
  requested: string;
  /** What the collision ladder actually applied. */
  resolvedTo: string;
  /** The claimant's minified name at request time. */
  oldName: string;
  /** Which resolution path decorated it. */
  site: "wave" | "remaining";
}

export interface NameContentionReport {
  events: NameContentionEvent[];
}

class NameContentionRecorder {
  private enabled = false;
  private events: NameContentionEvent[] = [];

  reset(enabled: boolean): void {
    this.enabled = enabled;
    this.events = [];
  }

  record(event: NameContentionEvent): void {
    if (!this.enabled) return;
    this.events.push(event);
  }

  report(): NameContentionReport {
    return { events: [...this.events] };
  }
}

export const nameContention = new NameContentionRecorder();
