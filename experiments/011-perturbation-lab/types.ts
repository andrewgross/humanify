/**
 * Types for the perturbation-lab experiment.
 *
 * The harness applies known AST transformations to a source file (v1 → v2),
 * minifies both sides, runs matchFunctions, and scores the result against
 * the ground truth embedded in the perturbation. No source maps needed.
 */

export interface CorpusItem {
  /** Stable identifier for reports. */
  id: string;
  /** Absolute path to the source file. */
  sourcePath: string;
  /** Short description. */
  description?: string;
}

export interface PerturbationResult {
  /** The transformed source. */
  source: string;
  /**
   * Names of source-level functions whose body structure changed.
   * Transitive parents (enclosing scopes) are NOT listed here — the runner
   * computes cascade automatically from the source AST.
   */
  directlyModified: string[];
  /** Functions added in v2 that don't exist in v1. */
  added: string[];
  /** Functions removed in v2 that existed in v1. */
  removed: string[];
  /** Human-readable change description. */
  description: string;
}

export interface Perturbation {
  id: string;
  description: string;
  apply(source: string): PerturbationResult;
}

/**
 * Ground truth computed from v1 source and a PerturbationResult.
 * Expressed in terms of *source-level* function counts.
 */
export interface SourceGroundTruth {
  v1FunctionCount: number;
  v2FunctionCount: number;
  /** Source fns whose hash is identical in v1 and v2 (should match). */
  expectedMatches: number;
  /** Source fns that were directly modified OR transitively touched. */
  expectedUnmatched: number;
  /** Source fns added in v2 (no v1 counterpart). */
  expectedAdded: number;
  /** Source fns removed in v2. */
  expectedRemoved: number;
  /** Names of v1 functions that should match (for per-function scoring later). */
  matchableV1Names: string[];
}

/**
 * Per-function expected pairing: a v1 minified sessionId should match
 * a specific v2 minified sessionId. Built from property-key identity maps.
 */
export interface GroundTruthPair {
  propertyKey: string;
  v1SessionId: string;
  v2SessionId: string;
}

/**
 * Full per-function ground truth for scoring.
 */
export interface MinifiedGroundTruth {
  /** Expected correct pairings (v1 → v2). */
  pairs: GroundTruthPair[];
  /** v1 sessionIds with no v2 counterpart (removed/renamed away). */
  v1Only: string[];
  /** v2 sessionIds with no v1 counterpart (added/renamed in). */
  v2Only: string[];
}

export interface MatcherOutcome {
  /** Functions in v1 minified output. */
  v1MinifiedCount: number;
  /** Functions in v2 minified output. */
  v2MinifiedCount: number;
  /** Matcher produced a match for this many old→new pairs. */
  matched: number;
  /** Matcher found multiple candidates but couldn't pick. */
  ambiguous: number;
  /** Matcher found no candidates. */
  unmatched: number;
}

export interface ConfusionMatrix {
  tp: number;
  fp: number;
  tn: number;
  fn: number;
  accuracy: number;
  precision: number;
  recall: number;
  f1: number;
}

export interface ExperimentRow {
  corpus: string;
  perturbation: string;
  perturbationDescription: string;
  minifier: string;
  sourceGroundTruth: SourceGroundTruth;
  matcher: MatcherOutcome;
  score: ConfusionMatrix;
}

export interface ExperimentResult {
  name: string;
  timestamp: string;
  /** Arbitrary record of knobs used (max resolution, feature toggles, etc.). */
  config: Record<string, unknown>;
  rows: ExperimentRow[];
  summary: {
    totalRuns: number;
    avgAccuracy: number;
    avgF1: number;
    avgRecall: number;
    avgPrecision: number;
  };
}
