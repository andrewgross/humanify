import { buildFingerprintData } from "../../test/e2e/harness/validate.js";

export interface BlastRadius {
  /** Total bytes different (simple string comparison) */
  bytesChanged: number;
  /** Total size of original minified output */
  totalBytes: number;
  /** bytesChanged / totalBytes */
  changeRatio: number;
  /** Functions whose structuralHash changed between original and perturbed */
  functionsAffected: number;
  /** Total functions in original */
  totalFunctions: number;
  /** functionsAffected / totalFunctions */
  functionChangeRatio: number;
  /** Categorization based on function-level impact */
  category: "local" | "moderate" | "global";
}

/**
 * Measure the blast radius of a perturbation by comparing original minified
 * code to the cleaned (marker-removed) perturbed minified code.
 *
 * Uses the fingerprint index to compare structuralHashes at the function level,
 * giving precise data on how many functions were structurally affected.
 */
export function measureBlastRadius(
  originalCode: string,
  perturbedCode: string
): BlastRadius {
  // Byte-level diff: count characters that differ
  const bytesChanged = countBytesDifferent(originalCode, perturbedCode);
  const totalBytes = originalCode.length;
  const changeRatio = totalBytes > 0 ? bytesChanged / totalBytes : 0;

  // Function-level diff via structuralHash comparison
  const origData = buildFingerprintData(originalCode, "original.min.js");
  const pertData = buildFingerprintData(perturbedCode, "perturbed.min.js");

  const origHashes = collectStructuralHashes(origData.index.fingerprints);
  const pertHashes = collectStructuralHashes(pertData.index.fingerprints);

  const totalFunctions = origHashes.length;

  // Build multisets of hashes and compare
  const origMultiset = toMultiset(origHashes);
  const pertMultiset = toMultiset(pertHashes);

  let functionsAffected = 0;
  for (const [hash, count] of origMultiset) {
    const pertCount = pertMultiset.get(hash) ?? 0;
    if (pertCount < count) {
      functionsAffected += count - pertCount;
    }
  }

  const functionChangeRatio =
    totalFunctions > 0 ? functionsAffected / totalFunctions : 0;

  const category = categorize(functionsAffected, totalFunctions);

  return {
    bytesChanged,
    totalBytes,
    changeRatio,
    functionsAffected,
    totalFunctions,
    functionChangeRatio,
    category
  };
}

function categorize(
  affected: number,
  total: number
): "local" | "moderate" | "global" {
  if (affected <= 2) return "local";
  if (total > 0 && affected / total > 0.3) return "global";
  return "moderate";
}

/**
 * Count bytes that differ between two strings using a simple character-level comparison.
 */
function countBytesDifferent(a: string, b: string): number {
  const maxLen = Math.max(a.length, b.length);
  let diff = 0;
  for (let i = 0; i < maxLen; i++) {
    if (a[i] !== b[i]) diff++;
  }
  return diff;
}

function collectStructuralHashes(
  fingerprints: Map<string, { structuralHash: string }>
): string[] {
  const hashes: string[] = [];
  for (const fp of fingerprints.values()) {
    hashes.push(fp.structuralHash);
  }
  return hashes;
}

function toMultiset(items: string[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const item of items) {
    m.set(item, (m.get(item) ?? 0) + 1);
  }
  return m;
}
