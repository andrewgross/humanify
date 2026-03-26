/**
 * Experiment runner: split a bundled fixture and compare to ground truth.
 *
 * Usage:
 *   tsx experiments/run.ts <fixture-name> [options]
 *
 * Options:
 *   --min-cluster-size <n>   Merge clusters with <= n members (default: 0)
 *   --proximity              Enable proximity-based singleton merging
 *   --proximity-merge        Enable post-clustering proximity-based merging
 *   --target-count <n>       Target number of output files (with --proximity-merge)
 *   --target-file-size <n>   Target avg lines per file (with --proximity-merge)
 *   --gap-threshold <n>      Max line gap for proximity merging
 *   --save <name>            Save results to experiments/results/<name>.json
 *   --compare <name>         Compare to saved baseline
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { buildFunctionGraph } from "../src/analysis/function-graph.js";
import type { ClusterOptions } from "../src/split/cluster.js";
import { clusterFunctions } from "../src/split/cluster.js";
import {
  buildClusterFileMap as buildClusterFileMapFromSplit,
  buildFunctionNameMap,
  parseFile
} from "../src/split/index.js";
import {
  assignFunctionsToModules,
  detectModules
} from "../src/split/module-detect.js";
import { computeMQ } from "../src/split/quality.js";
import { extractGroundTruth, extractSplitAssignment } from "./ground-truth.js";
import {
  computeClusteringMetrics,
  computePerFileBreakdown
} from "./metrics.js";
import { prepareFixture } from "./prepare.js";
import {
  groupByProximity,
  mergeByProximity,
  type ProximityMergeOptions
} from "./proximity-merge.js";
import { formatComparison, formatExperimentResult } from "./report.js";
import type { ExperimentMetrics, ExperimentResult } from "./types.js";

const EXPERIMENTS_DIR = import.meta.dirname;
const FIXTURES_DIR = join(EXPERIMENTS_DIR, "fixtures");
const RESULTS_DIR = join(EXPERIMENTS_DIR, "results");

interface RunOptions extends ClusterOptions {
  save?: string;
  compare?: string;
  proximityMerge?: boolean;
  proximityMergeOptions?: ProximityMergeOptions;
  /** Skip call-graph clustering, use pure proximity grouping */
  directGrouping?: boolean;
  directGroupingTarget?: number;
  /** Use bundler module detection (comments or moduleFactory) */
  esbuildDetect?: boolean;
}

/** Run call-graph clustering and return file map with stats. */
function clusterAndMap(
  functions: Parameters<typeof clusterFunctions>[0],
  options: ClusterOptions
) {
  const { clusters, shared, orphans } = clusterFunctions(functions, options);
  const functionNames = buildFunctionNameMap(functions);
  const clusterFileMap = buildClusterFileMapFromSplit(
    clusters,
    shared,
    orphans,
    functionNames
  );
  return {
    clusterFileMap,
    clusterCount: clusters.length,
    mqScore: computeMQ(clusters, functions),
    sharedCount: shared.size,
    orphanCount: orphans.size
  };
}

export async function runExperiment(
  fixtureName: string,
  options: RunOptions = {}
): Promise<ExperimentResult> {
  const fixtureDir = join(FIXTURES_DIR, fixtureName);
  const bundlePath = join(fixtureDir, "bundle.js");
  const mapPath = join(fixtureDir, "bundle.js.map");

  // Auto-prepare if fixture doesn't exist
  if (!existsSync(bundlePath)) {
    console.log(`Fixture ${fixtureName} not found, preparing...`);
    await prepareFixture(fixtureName);
  }

  if (!existsSync(bundlePath) || !existsSync(mapPath)) {
    throw new Error(
      `Fixture ${fixtureName} missing bundle or source map at ${fixtureDir}`
    );
  }

  const totalStart = performance.now();

  // 1. Parse the bundle (reuses src/split parseFile)
  console.log(`Parsing bundle...`);
  const parseStart = performance.now();
  const { ast } = parseFile(bundlePath);
  const functions = buildFunctionGraph(ast, bundlePath);
  const topLevel = functions.filter((fn) => !fn.scopeParent);
  const parseMs = Math.round(performance.now() - parseStart);
  console.log(
    `  ${functions.length} functions (${topLevel.length} top-level) in ${parseMs}ms`
  );

  // 2. Clustering
  console.log(`Clustering...`);
  const splitStart = performance.now();

  let finalClusterFileMap: Map<string, string>;
  let clusterCount: number;
  let mqScore: number;
  let sharedCount: number;
  let orphanCount: number;

  if (options.esbuildDetect) {
    // Use production module detection (comments or moduleFactory)
    const bundleSource = readFileSync(bundlePath, "utf-8");
    console.log(`  Module detection...`);
    const detection = detectModules(bundleSource);
    console.log(
      `  Bundler: ${detection.bundler}, ${detection.modules.length} modules`
    );

    if (detection.modules.length > 0) {
      const fnPositions = functions
        .filter((fn) => !fn.scopeParent)
        .map((fn) => ({
          sessionId: fn.sessionId,
          startLine: fn.path.node.loc?.start.line ?? 0
        }));
      finalClusterFileMap = assignFunctionsToModules(
        fnPositions,
        detection.modules
      );
      clusterCount = new Set(finalClusterFileMap.values()).size;
      const assigned = finalClusterFileMap.size;
      const unassigned = topLevel.length - assigned;
      console.log(`  ${assigned} functions assigned, ${unassigned} unassigned`);

      // For unassigned functions, use call-graph clustering
      if (unassigned > 0) {
        const unassignedFns = functions.filter(
          (fn) => !fn.scopeParent && !finalClusterFileMap.has(fn.sessionId)
        );
        console.log(
          `  Falling back to call-graph for ${unassignedFns.length} unassigned functions`
        );
        const fallbackResult = clusterAndMap(functions, options);
        for (const [id, file] of fallbackResult.clusterFileMap) {
          if (!finalClusterFileMap.has(id)) {
            finalClusterFileMap.set(id, `uncovered/${file}`);
          }
        }
        clusterCount = new Set(finalClusterFileMap.values()).size;
      }
    } else {
      console.log(`  No modules detected, falling back to call graph`);
      const result = clusterAndMap(functions, options);
      finalClusterFileMap = result.clusterFileMap;
      clusterCount = result.clusterCount;
      mqScore = result.mqScore;
      sharedCount = result.sharedCount;
      orphanCount = result.orphanCount;
    }
    mqScore = 0;
    sharedCount = 0;
    orphanCount = 0;
  } else if (options.directGrouping) {
    // Pure proximity grouping (no call graph)
    const target = options.directGroupingTarget ?? 10;
    console.log(`  Direct proximity grouping (target: ${target} groups)...`);
    finalClusterFileMap = groupByProximity(functions, { targetCount: target });
    clusterCount = new Set(finalClusterFileMap.values()).size;
    mqScore = 0;
    sharedCount = 0;
    orphanCount = 0;
    console.log(`  ${clusterCount} groups created`);
  } else {
    // Standard call-graph clustering + optional proximity post-merge
    const result = clusterAndMap(functions, options);
    finalClusterFileMap = result.clusterFileMap;
    clusterCount = result.clusterCount;
    mqScore = result.mqScore;
    sharedCount = result.sharedCount;
    orphanCount = result.orphanCount;

    if (options.proximityMerge) {
      const lineCount = readFileSync(bundlePath, "utf-8").split("\n").length;
      console.log(`  Proximity merging...`);
      finalClusterFileMap = mergeByProximity(
        functions,
        result.clusterFileMap,
        lineCount,
        options.proximityMergeOptions ?? {}
      );
      const newFileCount = new Set(finalClusterFileMap.values()).size;
      console.log(
        `  ${new Set(result.clusterFileMap.values()).size} files → ${newFileCount} files`
      );
    }
  }

  const splitMs = Math.round(performance.now() - splitStart);
  console.log(
    `  ${clusterCount} clusters, ${sharedCount} shared, ${orphanCount} orphans in ${splitMs}ms`
  );

  // 3. Extract ground truth from source map
  console.log(`Extracting ground truth from source map...`);
  const groundTruth = await extractGroundTruth(functions, mapPath);
  console.log(
    `  ${groundTruth.sourceFiles.length} original source files, ${groundTruth.functionToFile.size} functions mapped`
  );

  // 4. Extract split assignment
  const splitAssignment = extractSplitAssignment(
    functions,
    finalClusterFileMap
  );

  // 5. Compute metrics
  const metricsStart = performance.now();
  const { metrics: clusteringMetrics, matchedCount } = computeClusteringMetrics(
    groundTruth,
    splitAssignment
  );
  const perFileBreakdown = computePerFileBreakdown(
    groundTruth,
    splitAssignment
  );
  const metricsMs = Math.round(performance.now() - metricsStart);

  const totalMs = Math.round(performance.now() - totalStart);

  const metrics: ExperimentMetrics = {
    ...clusteringMetrics,
    fileCountRatio:
      splitAssignment.outputFiles.length / groundTruth.sourceFiles.length,
    originalFileCount: groundTruth.sourceFiles.length,
    splitFileCount: splitAssignment.outputFiles.length,
    totalFunctions: topLevel.length,
    functionsMatched: matchedCount,
    mqScore
  };

  return {
    fixture: fixtureName,
    config: {
      minClusterSize: options.minClusterSize ?? 0,
      proximityFallback: options.proximityFallback ?? false,
      proximityMerge: options.proximityMerge ?? false,
      directGrouping: options.directGrouping ?? false,
      directGroupingTarget: options.directGroupingTarget,
      ...(options.proximityMergeOptions ?? {})
    },
    metrics,
    timing: { parseMs, splitMs, metricsMs, totalMs },
    perFileBreakdown
  };
}

function parseArgs(args: string[]): { fixture: string; options: RunOptions } {
  const fixture = args[0];
  if (!fixture || fixture.startsWith("-")) {
    console.log("Usage: tsx experiments/run.ts <fixture-name> [options]");
    console.log("Options:");
    console.log("  --min-cluster-size <n>   Merge clusters with <= n members");
    console.log(
      "  --proximity              Enable proximity-based singleton merging"
    );
    console.log("  --proximity-merge        Post-clustering proximity merge");
    console.log("  --target-count <n>       Target output file count");
    console.log("  --target-file-size <n>   Target avg lines per file");
    console.log("  --gap-threshold <n>      Max gap for proximity merge");
    console.log("  --save <name>            Save results");
    console.log("  --compare <name>         Compare to saved baseline");
    process.exit(1);
  }

  const options: RunOptions = {};
  const proxOpts: ProximityMergeOptions = {};
  for (let i = 1; i < args.length; i++) {
    switch (args[i]) {
      case "--min-cluster-size":
        options.minClusterSize = Number.parseInt(args[++i], 10);
        break;
      case "--proximity":
        options.proximityFallback = true;
        break;
      case "--esbuild-detect":
        options.esbuildDetect = true;
        break;
      case "--direct":
        options.directGrouping = true;
        break;
      case "--direct-target":
        options.directGrouping = true;
        options.directGroupingTarget = Number.parseInt(args[++i], 10);
        break;
      case "--proximity-merge":
        options.proximityMerge = true;
        break;
      case "--target-count":
        options.proximityMerge = true;
        proxOpts.targetCount = Number.parseInt(args[++i], 10);
        break;
      case "--target-file-size":
        options.proximityMerge = true;
        proxOpts.targetAvgFileSize = Number.parseInt(args[++i], 10);
        break;
      case "--gap-threshold":
        options.proximityMerge = true;
        proxOpts.gapThreshold = Number.parseInt(args[++i], 10);
        break;
      case "--save":
        options.save = args[++i];
        break;
      case "--compare":
        options.compare = args[++i];
        break;
    }
  }
  if (options.proximityMerge) {
    options.proximityMergeOptions = proxOpts;
  }

  return { fixture, options };
}

function saveResult(name: string, result: ExperimentResult): void {
  mkdirSync(RESULTS_DIR, { recursive: true });
  const path = join(RESULTS_DIR, `${name}.json`);
  const serializable = {
    ...result,
    perFileBreakdown: result.perFileBreakdown.slice(0, 50)
  };
  writeFileSync(path, JSON.stringify(serializable, null, 2));
  console.log(`Results saved to ${path}`);
}

function loadResult(name: string): ExperimentResult {
  const path = join(RESULTS_DIR, `${name}.json`);
  if (!existsSync(path)) {
    throw new Error(`No saved result: ${path}`);
  }
  return JSON.parse(readFileSync(path, "utf-8"));
}

async function main(): Promise<void> {
  const { fixture, options } = parseArgs(process.argv.slice(2));

  const result = await runExperiment(fixture, options);
  console.log(formatExperimentResult(result));

  if (options.save) {
    saveResult(options.save, result);
  }

  if (options.compare) {
    const baseline = loadResult(options.compare);
    console.log(formatComparison(baseline, result));
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
