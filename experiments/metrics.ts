/**
 * Clustering comparison metrics for evaluating split quality.
 *
 * Compares split output assignments to ground truth file assignments
 * using standard information-theoretic and pair-counting metrics.
 */
import type {
  ClusteringMetrics,
  ExperimentMetrics,
  GroundTruthMapping,
  PerFileBreakdown,
  SplitAssignment
} from "./types.js";

// ── Contingency table ───────────────────────────────────────────────

interface ContingencyTable {
  /** table[i][j] = count of items in true class i AND predicted cluster j */
  table: number[][];
  rowSums: number[];
  colSums: number[];
  n: number;
  nRows: number;
  nCols: number;
}

/**
 * Convert label maps to aligned integer label vectors.
 * Only includes items present in both mappings.
 */
function alignLabels(
  groundTruth: Map<string, string>,
  predicted: Map<string, string>
): { trueLabels: number[]; predLabels: number[]; matchedCount: number } {
  const trueLabelMap = new Map<string, number>();
  const predLabelMap = new Map<string, number>();
  const trueLabels: number[] = [];
  const predLabels: number[] = [];

  for (const [fnId, trueFile] of groundTruth) {
    const predFile = predicted.get(fnId);
    if (predFile === undefined) continue;

    if (!trueLabelMap.has(trueFile)) {
      trueLabelMap.set(trueFile, trueLabelMap.size);
    }
    if (!predLabelMap.has(predFile)) {
      predLabelMap.set(predFile, predLabelMap.size);
    }

    trueLabels.push(trueLabelMap.get(trueFile)!);
    predLabels.push(predLabelMap.get(predFile)!);
  }

  return { trueLabels, predLabels, matchedCount: trueLabels.length };
}

function buildContingencyTable(
  trueLabels: number[],
  predLabels: number[]
): ContingencyTable {
  const n = trueLabels.length;
  if (n === 0) {
    return { table: [], rowSums: [], colSums: [], n: 0, nRows: 0, nCols: 0 };
  }

  const nRows = Math.max(...trueLabels) + 1;
  const nCols = Math.max(...predLabels) + 1;

  const table: number[][] = Array.from({ length: nRows }, () =>
    new Array(nCols).fill(0)
  );

  for (let i = 0; i < n; i++) {
    table[trueLabels[i]][predLabels[i]]++;
  }

  const rowSums = table.map((row) => row.reduce((a, b) => a + b, 0));
  const colSums = Array.from({ length: nCols }, (_, j) =>
    table.reduce((sum, row) => sum + row[j], 0)
  );

  return { table, rowSums, colSums, n, nRows, nCols };
}

// ── Adjusted Rand Index ─────────────────────────────────────────────

function choose2(x: number): number {
  return (x * (x - 1)) / 2;
}

function computeARI(ct: ContingencyTable): number {
  if (ct.n <= 1) return 1;

  let sumNijC2 = 0;
  for (const row of ct.table) {
    for (const nij of row) {
      sumNijC2 += choose2(nij);
    }
  }

  const sumAiC2 = ct.rowSums.reduce((s, ai) => s + choose2(ai), 0);
  const sumBjC2 = ct.colSums.reduce((s, bj) => s + choose2(bj), 0);
  const nC2 = choose2(ct.n);

  if (nC2 === 0) return 1;

  const expected = (sumAiC2 * sumBjC2) / nC2;
  const maxRI = 0.5 * (sumAiC2 + sumBjC2);
  const denom = maxRI - expected;

  if (denom === 0) return 1;

  return (sumNijC2 - expected) / denom;
}

// ── V-Measure (homogeneity + completeness) ──────────────────────────

function entropy(counts: number[], total: number): number {
  let h = 0;
  for (const c of counts) {
    if (c > 0) {
      const p = c / total;
      h -= p * Math.log(p);
    }
  }
  return h;
}

/** H(C|K): conditional entropy of true classes given predicted clusters. */
function conditionalEntropyRowsGivenCols(ct: ContingencyTable): number {
  let h = 0;
  for (let j = 0; j < ct.nCols; j++) {
    if (ct.colSums[j] === 0) continue;
    for (let i = 0; i < ct.nRows; i++) {
      const nij = ct.table[i][j];
      if (nij > 0) {
        h -= (nij / ct.n) * Math.log(nij / ct.colSums[j]);
      }
    }
  }
  return h;
}

/** H(K|C): conditional entropy of predicted clusters given true classes. */
function conditionalEntropyColsGivenRows(ct: ContingencyTable): number {
  let h = 0;
  for (let i = 0; i < ct.nRows; i++) {
    if (ct.rowSums[i] === 0) continue;
    for (let j = 0; j < ct.nCols; j++) {
      const nij = ct.table[i][j];
      if (nij > 0) {
        h -= (nij / ct.n) * Math.log(nij / ct.rowSums[i]);
      }
    }
  }
  return h;
}

function computeVMeasure(ct: ContingencyTable): {
  homogeneity: number;
  completeness: number;
  vMeasure: number;
} {
  if (ct.n === 0) return { homogeneity: 1, completeness: 1, vMeasure: 1 };

  const hC = entropy(ct.rowSums, ct.n);
  const hK = entropy(ct.colSums, ct.n);
  const hCgivenK = conditionalEntropyRowsGivenCols(ct);
  const hKgivenC = conditionalEntropyColsGivenRows(ct);

  const homogeneity = hC === 0 ? 1 : 1 - hCgivenK / hC;
  const completeness = hK === 0 ? 1 : 1 - hKgivenC / hK;

  const vm =
    homogeneity + completeness === 0
      ? 0
      : (2 * homogeneity * completeness) / (homogeneity + completeness);

  return { homogeneity, completeness, vMeasure: vm };
}

// ── Purity metrics ──────────────────────────────────────────────────

/** Average purity: for each predicted cluster, fraction from dominant true class. */
function computePurity(ct: ContingencyTable): number {
  if (ct.n === 0) return 1;

  let totalPurity = 0;
  for (let j = 0; j < ct.nCols; j++) {
    if (ct.colSums[j] === 0) continue;
    let maxInCol = 0;
    for (let i = 0; i < ct.nRows; i++) {
      maxInCol = Math.max(maxInCol, ct.table[i][j]);
    }
    totalPurity += maxInCol;
  }

  return totalPurity / ct.n;
}

/** Average inverse purity: for each true class, fraction in dominant predicted cluster. */
function computeInversePurity(ct: ContingencyTable): number {
  if (ct.n === 0) return 1;

  let totalInvPurity = 0;
  for (let i = 0; i < ct.nRows; i++) {
    if (ct.rowSums[i] === 0) continue;
    let maxInRow = 0;
    for (let j = 0; j < ct.nCols; j++) {
      maxInRow = Math.max(maxInRow, ct.table[i][j]);
    }
    totalInvPurity += maxInRow;
  }

  return totalInvPurity / ct.n;
}

// ── Public API ──────────────────────────────────────────────────────

/**
 * Compute all clustering comparison metrics.
 */
export function computeClusteringMetrics(
  groundTruth: GroundTruthMapping,
  splitAssignment: SplitAssignment
): { metrics: ClusteringMetrics; matchedCount: number } {
  const { trueLabels, predLabels, matchedCount } = alignLabels(
    groundTruth.functionToFile,
    splitAssignment.functionToFile
  );

  if (matchedCount === 0) {
    return {
      metrics: {
        ari: 0,
        homogeneity: 0,
        completeness: 0,
        vMeasure: 0,
        purity: 0,
        inversePurity: 0
      },
      matchedCount: 0
    };
  }

  const ct = buildContingencyTable(trueLabels, predLabels);
  const ari = computeARI(ct);
  const { homogeneity, completeness, vMeasure } = computeVMeasure(ct);
  const purity = computePurity(ct);
  const inversePurity = computeInversePurity(ct);

  return {
    metrics: {
      ari,
      homogeneity,
      completeness,
      vMeasure,
      purity,
      inversePurity
    },
    matchedCount
  };
}

/**
 * Compute per-original-file breakdown of splitting quality.
 */
export function computePerFileBreakdown(
  groundTruth: GroundTruthMapping,
  splitAssignment: SplitAssignment
): PerFileBreakdown[] {
  const breakdown: PerFileBreakdown[] = [];

  for (const [sourceFile, fnIds] of groundTruth.fileToFunctions) {
    const fileCounts = new Map<string, number>();
    for (const fnId of fnIds) {
      const outputFile = splitAssignment.functionToFile.get(fnId);
      if (outputFile) {
        fileCounts.set(outputFile, (fileCounts.get(outputFile) ?? 0) + 1);
      }
    }

    let dominantFile = "";
    let maxCount = 0;
    for (const [file, count] of fileCounts) {
      if (count > maxCount) {
        maxCount = count;
        dominantFile = file;
      }
    }

    const matchedCount = Array.from(fileCounts.values()).reduce(
      (a, b) => a + b,
      0
    );

    breakdown.push({
      originalFile: sourceFile,
      functionCount: fnIds.length,
      splitIntoFiles: Array.from(fileCounts.keys()).sort(),
      dominantFile,
      completeness: matchedCount > 0 ? maxCount / matchedCount : 0
    });
  }

  // Sort by worst completeness first (most fragmented)
  return breakdown.sort((a, b) => a.completeness - b.completeness);
}
