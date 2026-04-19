import type { MatchResult } from "../../src/analysis/types.js";
import type { ConfusionMatrix, MinifiedGroundTruth } from "./types.js";

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
