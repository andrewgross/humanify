import * as fs from "node:fs";
import { extractDeclaredNames } from "./emitter.js";
import type { SplitPlan } from "./types.js";

/**
 * Ground truth: function name → original module path.
 */
export interface GroundTruth {
  description?: string;
  functions: Record<string, string>;
}

/**
 * Result of alignment computation.
 */
export interface AlignmentResult {
  /** Rand Index in [0, 1]. 1.0 = perfect agreement. */
  randIndex: number;
  /** True positives: same cluster AND same original module */
  tp: number;
  /** True negatives: different cluster AND different original module */
  tn: number;
  /** False positives: same cluster but different original module */
  fp: number;
  /** False negatives: different cluster but same original module */
  fn: number;
  /** Total pairs considered */
  totalPairs: number;
  /** Functions found in both output and ground truth */
  matchedFunctions: number;
  /** Functions in ground truth but not in output */
  missingFromOutput: string[];
  /** Functions in output but not in ground truth */
  extraInOutput: string[];
}

/**
 * Load ground truth from a JSON file.
 */
export function loadGroundTruth(path: string): GroundTruth {
  const content = fs.readFileSync(path, "utf-8");
  return JSON.parse(content) as GroundTruth;
}

/**
 * Build a mapping of declared function names → output file from the split plan's ledger.
 */
export function buildOutputMapping(plan: SplitPlan): Map<string, string> {
  const mapping = new Map<string, string>();

  for (const entry of plan.ledger.entries.values()) {
    if (!entry.outputFile || entry.outputFile === "index.js") continue;
    const names = extractDeclaredNames(entry.node);
    for (const name of names) {
      mapping.set(name, entry.outputFile);
    }
  }

  return mapping;
}

/**
 * Compute alignment between output clustering and ground truth using the Rand Index.
 *
 * For each pair of functions present in both mappings:
 * - TP: same output file AND same original module
 * - TN: different output file AND different original module
 * - FP: same output file but different original module
 * - FN: different output file but same original module
 *
 * Rand Index = (TP + TN) / (TP + TN + FP + FN)
 */
export function computeAlignment(
  outputMapping: Map<string, string>,
  groundTruth: GroundTruth
): AlignmentResult {
  // Find intersection of names
  const gtNames = Object.keys(groundTruth.functions);
  const outNames = Array.from(outputMapping.keys());

  const matched = gtNames.filter((name) => outputMapping.has(name));
  const missingFromOutput = gtNames.filter((name) => !outputMapping.has(name));
  const extraInOutput = outNames.filter(
    (name) => !(name in groundTruth.functions)
  );

  let tp = 0,
    tn = 0,
    fp = 0,
    fn = 0;

  // Compare all pairs
  for (let i = 0; i < matched.length; i++) {
    for (let j = i + 1; j < matched.length; j++) {
      const nameA = matched[i];
      const nameB = matched[j];

      const sameOutput = outputMapping.get(nameA) === outputMapping.get(nameB);
      const sameOriginal =
        groundTruth.functions[nameA] === groundTruth.functions[nameB];

      if (sameOutput && sameOriginal) tp++;
      else if (!sameOutput && !sameOriginal) tn++;
      else if (sameOutput && !sameOriginal) fp++;
      else fn++;
    }
  }

  const totalPairs = tp + tn + fp + fn;
  const randIndex = totalPairs > 0 ? (tp + tn) / totalPairs : 1;

  return {
    randIndex,
    tp,
    tn,
    fp,
    fn,
    totalPairs,
    matchedFunctions: matched.length,
    missingFromOutput,
    extraInOutput
  };
}
