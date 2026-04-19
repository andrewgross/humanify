import type {
  FingerprintIndex,
  MatchResult
} from "../../src/analysis/types.js";
import type { ConfusionMatrix, MinifiedGroundTruth } from "./types.js";

/**
 * Score a matcher run at the v1-function level.
 *
 * Classification per v1 function:
 * - TP: matcher produced a match (the cascade only considers same-hash
 *   candidates, so every match is structurally valid by construction).
 * - FN: didn't match, but a candidate with the same exactHash exists in v2.
 *   We lost a valid match somewhere in the disambiguation cascade.
 * - TN: didn't match, and no candidate exists in v2. Correct — function was
 *   genuinely perturbed or removed.
 * - FP: would require the matcher to resolve to a different-hash function,
 *   which the current cascade never does. Always 0 under this scoring.
 *
 * Caveat: this doesn't detect "wrong twin" matches (e.g., paired identical
 * helpers where the matcher picked the wrong side). Those appear as TPs here.
 */
export function scoreByHashAvailability(
  v1Index: FingerprintIndex,
  v2Index: FingerprintIndex,
  matchResult: MatchResult
): ConfusionMatrix {
  let tp = 0;
  const fp = 0;
  let tn = 0;
  let fn = 0;

  for (const [v1Id, v1Fp] of v1Index.fingerprints) {
    const gotMatched = matchResult.matches.has(v1Id);
    const hashExistsInV2 =
      (v2Index.byExactHash.get(v1Fp.exactHash)?.length ?? 0) > 0;

    if (gotMatched) {
      tp++;
    } else if (hashExistsInV2) {
      fn++;
    } else {
      tn++;
    }
  }

  return withDerived(tp, fp, tn, fn);
}

/**
 * Score using per-function correspondence ground truth.
 *
 * For each ground-truth pair (v1Id, v2Id):
 * - TP: matcher.matches.get(v1Id) === v2Id (correct pairing)
 * - FP: matcher.matches.get(v1Id) === some other v2 ID (wrong twin!)
 * - FN: v1Id not in matcher.matches (missed match)
 *
 * For v1-only functions (no v2 counterpart):
 * - TN: v1Id correctly unmatched
 * - FP: v1Id incorrectly matched to something
 */
export function scoreByCorrespondence(
  groundTruth: MinifiedGroundTruth,
  matchResult: MatchResult
): ConfusionMatrix {
  let tp = 0;
  let fp = 0;
  let tn = 0;
  let fn = 0;

  // Check each expected pair
  for (const pair of groundTruth.pairs) {
    const matchedTo = matchResult.matches.get(pair.v1SessionId);
    if (matchedTo === pair.v2SessionId) {
      tp++;
    } else if (matchedTo !== undefined) {
      fp++; // Wrong twin!
    } else {
      fn++; // Missed match
    }
  }

  // v1-only functions should NOT be matched
  for (const v1Sid of groundTruth.v1Only) {
    if (matchResult.matches.has(v1Sid)) {
      fp++; // Incorrectly matched
    } else {
      tn++; // Correctly unmatched
    }
  }

  return withDerived(tp, fp, tn, fn);
}

function withDerived(
  tp: number,
  fp: number,
  tn: number,
  fn: number
): ConfusionMatrix {
  const total = tp + fp + tn + fn;
  const accuracy = total > 0 ? (tp + tn) / total : 0;
  const precision = tp + fp > 0 ? tp / (tp + fp) : 0;
  const recall = tp + fn > 0 ? tp / (tp + fn) : 0;
  const f1 =
    precision + recall > 0 ? (2 * precision * recall) / (precision + recall) : 0;
  return { tp, fp, tn, fn, accuracy, precision, recall, f1 };
}
