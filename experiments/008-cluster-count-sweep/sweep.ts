/**
 * Experiment 008: Cluster Count Sweep
 *
 * Sweeps target cluster counts from 3 to 50 for each fixture,
 * recording ARI at each count. Finds the optimal count and
 * correlates with function count / line count.
 *
 * Usage:
 *   tsx experiments/008-cluster-count-sweep/sweep.ts <fixture-name>
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { buildFunctionGraph } from "../../src/analysis/function-graph.js";
import { parseFile } from "../../src/split/index.js";
import { referenceCluster } from "../../src/split/reference-cluster.js";
import { extractGroundTruth, extractSplitAssignment } from "../ground-truth.js";
import { computeClusteringMetrics } from "../metrics.js";

const FIXTURES_DIR = join(import.meta.dirname, "..", "fixtures");

interface SweepResult {
  targetCount: number;
  ari: number;
  vMeasure: number;
  homogeneity: number;
  completeness: number;
  actualFileCount: number;
}

async function sweep(fixtureName: string): Promise<void> {
  const fixtureDir = join(FIXTURES_DIR, fixtureName);
  const bundlePath = join(fixtureDir, "bundle.js");
  const mapPath = join(fixtureDir, "bundle.js.map");
  const source = readFileSync(bundlePath, "utf-8");
  const lineCount = source.split("\n").length;

  console.log(`\n=== Cluster Count Sweep: ${fixtureName} ===\n`);

  // Parse
  console.log("Parsing bundle...");
  const { ast } = parseFile(bundlePath);
  const functions = buildFunctionGraph(ast, bundlePath);
  const topLevel = functions.filter((fn) => !fn.scopeParent);
  console.log(`  ${topLevel.length} top-level functions, ${lineCount} lines\n`);

  // Ground truth
  const groundTruth = await extractGroundTruth(functions, mapPath);
  console.log(`  ${groundTruth.sourceFiles.length} original source files\n`);

  const parsedFiles = [{ ast, filePath: bundlePath, source }];

  // Sweep
  const results: SweepResult[] = [];
  const counts = [2, 3, 4, 5, 6, 7, 8, 10, 12, 15, 20, 25, 30, 40, 50];

  console.log("Sweeping target counts...");
  console.log(
    `  ${"Target".padStart(8)} ${"ARI".padStart(8)} ${"V-Meas".padStart(8)} ${"Homog".padStart(8)} ${"Compl".padStart(8)} ${"Files".padStart(8)}`
  );
  console.log(`  ${"─".repeat(48)}`);

  for (const targetCount of counts) {
    if (targetCount > topLevel.length / 2) break;

    const clusterFileMap = referenceCluster(
      functions,
      parsedFiles,
      targetCount
    );
    const splitAssignment = extractSplitAssignment(functions, clusterFileMap);
    const { metrics } = computeClusteringMetrics(groundTruth, splitAssignment);
    const actualFileCount = new Set(clusterFileMap.values()).size;

    const result: SweepResult = {
      targetCount,
      ari: metrics.ari,
      vMeasure: metrics.vMeasure,
      homogeneity: metrics.homogeneity,
      completeness: metrics.completeness,
      actualFileCount
    };
    results.push(result);

    console.log(
      `  ${String(targetCount).padStart(8)} ${metrics.ari.toFixed(3).padStart(8)} ${metrics.vMeasure.toFixed(3).padStart(8)} ${metrics.homogeneity.toFixed(3).padStart(8)} ${metrics.completeness.toFixed(3).padStart(8)} ${String(actualFileCount).padStart(8)}`
    );
  }

  // Find optimal
  const bestByAri = results.reduce((best, r) => (r.ari > best.ari ? r : best));
  const bestByVmeasure = results.reduce((best, r) =>
    r.vMeasure > best.vMeasure ? r : best
  );

  console.log(`\n=== Best Results ===`);
  console.log(
    `  Best ARI:       ${bestByAri.ari.toFixed(3)} at target=${bestByAri.targetCount} (actual=${bestByAri.actualFileCount} files)`
  );
  console.log(
    `  Best V-Measure: ${bestByVmeasure.vMeasure.toFixed(3)} at target=${bestByVmeasure.targetCount} (actual=${bestByVmeasure.actualFileCount} files)`
  );
  console.log(`  Ground truth:   ${groundTruth.sourceFiles.length} files`);

  // Correlation analysis
  const optimalRatio = bestByAri.targetCount / topLevel.length;
  const optimalPerFile = topLevel.length / bestByAri.targetCount;
  console.log(`\n=== Heuristic Calibration ===`);
  console.log(`  Optimal ratio (target/functions): ${optimalRatio.toFixed(4)}`);
  console.log(
    `  Functions per file at optimal:    ${optimalPerFile.toFixed(1)}`
  );
  console.log(
    `  Lines per file at optimal:        ${(lineCount / bestByAri.targetCount).toFixed(0)}`
  );
}

const fixtureName = process.argv[2];
if (!fixtureName) {
  console.log(
    "Usage: tsx experiments/008-cluster-count-sweep/sweep.ts <fixture-name>"
  );
  process.exit(1);
}

sweep(fixtureName).catch((err) => {
  console.error(err);
  process.exit(1);
});
