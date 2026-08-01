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
 * ## Why this describes every statement, and used not to
 *
 * The first version detailed seven tiers of ten and merely COUNTED the rest,
 * reasoning that hash and name place ~90% of statements uneventfully and that
 * 35,903 entries would bloat an already ~100 MB diagnostics file.
 *
 * Both halves of that were wrong. On a real bundle it described **1,192 of
 * 35,903 statements** — 3.3% — and held **zero** entries for `hash`, the tier
 * that makes most of the decisions. So when exp057 asked why a group of 26
 * declarations changed file between two releases (churning every importer's
 * alias at 399 usage sites in one consumer), the trail had nothing to say about
 * any of them. *The instrument could not explain the placements that matter
 * most, because it did not record the tier that makes most of them.*
 *
 * And the size fear was misplaced: the vote ARRAYS are the bulk, not the
 * entries. Describing every statement while keeping those arrays only where
 * they explain something costs a few MB, against 100 MB for the rest.
 *
 * So: every statement gets an entry; the bulky evidence is kept only when it
 * has something to say — see `keepsEvidence`.
 */

/** Tiers whose individual decisions always warrant the full evidence: the ones
 * that lost (locality), and the ones whose gates are new enough to want
 * verifying. */
const DETAILED_TIERS = new Set([
  "conflict",
  "novote",
  "allsame",
  "anchor",
  "anchorPreempt",
  "preempt",
  "fill"
]);

/**
 * Names beyond this are noise in a trail — a big statement declares hundreds.
 *
 * Raised from 8 once the trail became the index a reader SEARCHES by name. The
 * 2.1.215→216 statement that dragged 32 declarations into the wrong file
 * recorded the first 8; looking up any of the other 24 — `localPendingTasks`,
 * `taskStatuses`, `sessionStatusLabels` — found nothing, so the one entry that
 * explained 962 git lines of churn was unreachable from 75% of its own names.
 * `nameCount` records the real total whenever the list is cut.
 */
const MAX_NAMES = 32;

/**
 * Why the hash tier — the strongest, order-free and name-free evidence —
 * did not settle a statement. Undefined when it did.
 *
 * This is the first question to ask of a statement that moved: hash placement
 * is the only tier that CANNOT move one, so a move means the hash missed, and
 * these are the only four ways it can.
 */
export type HashMiss =
  /** The prior ledger carries no usable hashes at all (first release, or a
   * hash-version bump) — the tier is off, not abstaining. */
  | "no-prior-hashes"
  /** This statement's hash does not appear in the prior release: its content
   * genuinely changed. */
  | "absent"
  /** The hash appears, but a different number of times than this release has,
   * so the occurrences cannot be paired 1:1. */
  | "count"
  /** The hash appears the right number of times but its prior occurrences were
   * spread across more than one file, so there is no single home to inherit. */
  | "split";

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
  /** What the statement declares, truncated at `MAX_NAMES`. */
  names: string[];
  /** How many names the statement actually declares — present only when
   * `names` is a truncation of it, so a full list never reads as a partial one. */
  nameCount?: number;
  /** The tier that placed it: hash / preempt / name / ordinal / allsame /
   * fill / anchor, or conflict / novote when nothing had evidence. */
  placedBy: string;
  file: string;
  /**
   * The file this statement occupied in the prior release, according to the
   * strongest prior-identity evidence that HAD an opinion — whether or not that
   * evidence is what placed it. `priorFile !== file` is a MOVE, the thing that
   * churns two files plus every importer.
   *
   * Read it together with `priorFileFrom`: the three sources are not equally
   * trustworthy, and the move-prone statements are exactly the ones where the
   * best source abstained.
   */
  priorFile?: string;
  /** Which evidence `priorFile` came from, strongest first: identical content
   * (`hash`), a fingerprint-matched binding (`identity`), or rare string
   * literals identifying one prior statement (`anchor`). Never the name vote —
   * see `priorHome` in `stable-split.ts` for why that made it vacuous. */
  priorFileFrom?: "hash" | "identity" | "anchor";
  /** Why the hash tier abstained, when it did. */
  hashMiss?: HashMiss;
  /** What the tiers that did NOT win would have said, recorded only when at
   * least one of them DISAGREES with the winner. Unanimous evidence explains
   * itself; a disagreement is the whole story. */
  alternatives?: Record<string, string>;
  evidence: PlacementEvidence;
}

export interface PlacementTrailReport {
  /** tier → statements placed. Mirrors the run log's inheritance summary. */
  tiers: Record<string, number>;
  /** One entry per statement. */
  trails: PlacementTrailEntry[];
}

/** The dissenting subset of `alternatives`, or undefined when all agree. */
function dissenters(
  alternatives: Record<string, string> | undefined,
  file: string
): Record<string, string> | undefined {
  if (!alternatives) return undefined;
  const out: Record<string, string> = {};
  let any = false;
  for (const [tier, candidate] of Object.entries(alternatives)) {
    if (candidate === file) continue;
    out[tier] = candidate;
    any = true;
  }
  return any ? out : undefined;
}

/**
 * Whether an entry keeps its vote arrays. They are the only part of a trail
 * that is big, so they are kept exactly where they answer a question:
 *
 * - the tier that placed it lost to locality, or is new enough to want checking;
 * - some other tier disagreed with the winner;
 * - or the statement MOVED, which a reviewer always wants explained.
 */
function keepsEvidence(
  entry: PlacementTrailEntry,
  alternatives: Record<string, string> | undefined
): boolean {
  return (
    DETAILED_TIERS.has(entry.placedBy) ||
    alternatives !== undefined ||
    (entry.priorFile !== undefined && entry.priorFile !== entry.file)
  );
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
    const alternatives = dissenters(entry.alternatives, entry.file);
    this.trails.push({
      ...entry,
      names: entry.names.slice(0, MAX_NAMES),
      nameCount:
        entry.names.length > MAX_NAMES ? entry.names.length : undefined,
      alternatives,
      evidence: keepsEvidence(entry, alternatives) ? entry.evidence : {}
    });
  }

  report(): PlacementTrailReport {
    return { tiers: { ...this.tiers }, trails: this.trails };
  }
}

/** Module singleton — same shape as `strategyTrail` and `debug`. */
export const placementTrail = new PlacementTrailRecorder();
