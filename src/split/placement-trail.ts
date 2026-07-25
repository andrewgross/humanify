/**
 * Per-statement FILE PLACEMENT trail — the split's counterpart to
 * `strategy-trail.ts`, which does the same job for naming.
 *
 * The naming side has a registry (`TRANSFER_PIPELINE`) where every tier is a
 * named step, so "which strategy named this binding, and what did the others
 * try?" is recorded as it happens. Placement had only totals: the run log says
 * `922 residue by locality` but not WHICH statements, or what evidence was
 * available and discarded. Recovering that meant replaying the whole assignment
 * offline against the two bundles (experiments/041-content-anchor) — 300 lines
 * of reconstruction for something the splitter knew and threw away.
 *
 * So record it. The payoff is concrete: the reason 2.1.215→216 scattered a
 * 149-line function into an unrelated file is one line of this trail —
 *
 *     placedBy "novote", votes ["…/context-usage.js", "…/socket-logger.js"]
 *
 * — the function's own name voting correctly, a parameter name outvoting it,
 * and the disagreement discarding both.
 *
 * Off by default, exactly like the strategy trail: a module singleton enabled
 * by `--diagnostics`. When disabled `record()` is a no-op past one boolean, and
 * it never influences a decision — it only observes ones already made.
 *
 * Detail is kept for the DIAGNOSABLE population only. The hash and name tiers
 * place ~90% of statements correctly and uneventfully; recording all 35,903 of
 * them would add tens of MB to a diagnostics file that is already ~100 MB. Every
 * tier is counted; only the interesting ones are described.
 */

/** Tiers whose individual decisions are worth describing: the ones that lost
 * (locality), and the ones whose gates are new enough to want verifying. */
const DETAILED_TIERS = new Set([
  "conflict",
  "novote",
  "allsame",
  "anchor",
  "anchorPreempt",
  "preempt",
  "fill"
]);

/** Names beyond this are noise in a trail — a big statement declares hundreds. */
const MAX_NAMES = 8;

export interface PlacementEvidence {
  /** Files this statement's declared names voted for. More than one is a
   * disagreement, which is what sends a statement to locality. */
  votes?: string[];
  /** The subset of votes cast by names with exactly ONE prior home. */
  allSame?: string[];
  /** The file the content anchor identified, when it had a verdict. */
  anchor?: string;
}

export interface PlacementTrailEntry {
  /** Bundle-order index of the top-level statement. */
  index: number;
  /** What the statement declares, truncated — enough to find it by eye. */
  names: string[];
  /** The tier that placed it: hash / preempt / name / ordinal / allsame /
   * fill / anchor, or conflict / novote when nothing had evidence. */
  placedBy: string;
  file: string;
  evidence: PlacementEvidence;
}

export interface PlacementTrailReport {
  /** tier → statements placed. Mirrors the run log's inheritance summary. */
  tiers: Record<string, number>;
  /** Detail for the diagnosable tiers only — see DETAILED_TIERS. */
  trails: PlacementTrailEntry[];
}

class PlacementTrailRecorder {
  private enabled = false;
  private tiers: Record<string, number> = {};
  private trails: PlacementTrailEntry[] = [];

  /** Clear state and set enablement for the coming run. */
  reset(enabled: boolean): void {
    this.enabled = enabled;
    this.tiers = {};
    this.trails = [];
  }

  isEnabled(): boolean {
    return this.enabled;
  }

  record(entry: PlacementTrailEntry): void {
    if (!this.enabled) return;
    this.tiers[entry.placedBy] = (this.tiers[entry.placedBy] ?? 0) + 1;
    if (!DETAILED_TIERS.has(entry.placedBy)) return;
    this.trails.push({
      ...entry,
      names: entry.names.slice(0, MAX_NAMES)
    });
  }

  report(): PlacementTrailReport {
    return { tiers: { ...this.tiers }, trails: this.trails };
  }
}

/** Module singleton — same shape as `strategyTrail` and `debug`. */
export const placementTrail = new PlacementTrailRecorder();
