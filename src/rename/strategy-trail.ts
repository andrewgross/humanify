/**
 * Per-identifier strategy attempt trail — debugging provenance for the
 * transfer tiers.
 *
 * Every naming strategy that CONSIDERS a binding records its attempt
 * (applied / rejected / abstained / vote-routed + reason). Recording for
 * a binding stops at the first applied entry — the name is settled —
 * and later attempts only bump `postSettleAttempts`, which doubles as a
 * clobber detector (a tier trying to rename an already-settled binding
 * is a phase-ordering bug worth seeing).
 *
 * Off by default: a module singleton (same shape as `debug`) enabled by
 * `--diagnostics`, drained into the diagnostics JSON as raw trails plus
 * a per-strategy funnel rollup. When disabled, record() is a no-op and
 * the hot paths pay one boolean check.
 */
import type { Binding } from "@babel/traverse";

export interface StrategyAttempt {
  strategy: string;
  outcome: "applied" | "rejected" | "abstained" | "vote";
  reason?: string;
  newName?: string;
}

export interface StrategyTrailEntry {
  oldName: string;
  /** Declaration position of the binding ("line:col"), fresh-side coords. */
  loc: string;
  trail: StrategyAttempt[];
  /** Strategy of the applied entry, when one landed. */
  settledBy?: string;
  /** Strategy of the LAST applied entry across ALL passes — the terminal
   *  namer. Equals settledBy unless a post pass (floor, reconcile, sweep)
   *  re-renamed the binding afterwards. */
  terminalBy?: string;
  /** Rename attempts recorded after settling — should be 0; >0 flags a
   *  phase-ordering clobber. */
  postSettleAttempts: number;
  /** Vote testimony recorded after settling — expected (later matched
   *  callers keep testifying); counted, never logged. */
  postSettleVotes: number;
}

export interface StrategyTrailReport {
  trails: StrategyTrailEntry[];
  /** strategy → outcome → count, across all recorded attempts. */
  funnel: Record<string, Record<string, number>>;
}

class StrategyTrailRecorder {
  private enabled = false;
  private entries = new Map<Binding, StrategyTrailEntry>();

  /** Clear state and set enablement for the coming run. */
  reset(enabled: boolean): void {
    this.enabled = enabled;
    this.entries = new Map();
  }

  record(binding: Binding, oldName: string, attempt: StrategyAttempt): void {
    if (!this.enabled) return;
    const entry = this.entryFor(binding, oldName);
    if (entry.settledBy) {
      if (attempt.outcome === "vote") entry.postSettleVotes++;
      else entry.postSettleAttempts++;
      return;
    }
    entry.trail.push(attempt);
    if (attempt.outcome === "applied") {
      entry.settledBy = attempt.strategy;
      entry.terminalBy = attempt.strategy;
    }
  }

  /**
   * Record an attempt by a POST pass (naming floor, reconcile, sweep).
   * These legitimately act on settled bindings, so the entry keeps
   * accepting attempts — no settled-stop, no clobber counting — and an
   * applied entry updates `terminalBy` (the last namer wins).
   */
  recordPostPass(
    binding: Binding,
    oldName: string,
    attempt: StrategyAttempt
  ): void {
    if (!this.enabled) return;
    const entry = this.entryFor(binding, oldName);
    entry.trail.push(attempt);
    if (attempt.outcome === "applied") entry.terminalBy = attempt.strategy;
  }

  private entryFor(binding: Binding, oldName: string): StrategyTrailEntry {
    let entry = this.entries.get(binding);
    if (!entry) {
      const loc = binding.identifier.loc;
      entry = {
        oldName,
        loc: loc ? `${loc.start.line}:${loc.start.column}` : "?",
        trail: [],
        postSettleAttempts: 0,
        postSettleVotes: 0
      };
      this.entries.set(binding, entry);
    }
    return entry;
  }

  report(): StrategyTrailReport {
    const trails = [...this.entries.values()];
    const funnel: Record<string, Record<string, number>> = {};
    for (const entry of trails) {
      for (const attempt of entry.trail) {
        let byOutcome = funnel[attempt.strategy];
        if (!byOutcome) {
          byOutcome = {};
          funnel[attempt.strategy] = byOutcome;
        }
        byOutcome[attempt.outcome] = (byOutcome[attempt.outcome] ?? 0) + 1;
      }
    }
    return { trails, funnel };
  }
}

export const strategyTrail = new StrategyTrailRecorder();
