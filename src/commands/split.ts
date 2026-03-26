import * as fs from "node:fs";
import * as path from "node:path";
import type { Command } from "commander";
import type { SplitStrategyType } from "../split/adapters/types.js";
import { generateManifest, splitAndEmit, splitDryRun } from "../split/index.js";
import type { SplitPlan } from "../split/types.js";

const VALID_STRATEGIES = new Set([
  "auto",
  "esbuild-esm",
  "esbuild-cjs",
  "call-graph"
]);

type SplitOpts = {
  output: string;
  dryRun?: boolean;
  verbose?: boolean;
  minClusterSize: string;
  proximity?: boolean;
  splitStrategy?: string;
};

/** Recursively list .js files, returning paths relative to rootDir. */
function listJsFilesRecursive(dir: string, rootDir: string): string[] {
  const results: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...listJsFilesRecursive(fullPath, rootDir));
    } else if (entry.name.endsWith(".js")) {
      results.push(path.relative(rootDir, fullPath));
    }
  }
  return results;
}

function parseSplitInput(
  input: string
): { inputPath: string } | { error: string } {
  const inputPath = path.resolve(input);
  if (!fs.existsSync(inputPath)) {
    return { error: `Error: Input path does not exist: ${inputPath}` };
  }
  return { inputPath };
}

function buildPlan(
  inputPath: string,
  outputDir: string,
  isDryRun: boolean,
  clusterOptions: { minClusterSize?: number; proximityFallback?: boolean }
): SplitPlan {
  if (isDryRun) {
    return splitDryRun([inputPath], clusterOptions);
  }
  return splitAndEmit([inputPath], outputDir, clusterOptions);
}

function printPlanSummary(plan: SplitPlan, opts: SplitOpts): void {
  console.log(`Clusters: ${plan.stats.totalClusters}`);
  console.log(`Total top-level functions: ${plan.stats.totalFunctions}`);
  console.log(`Avg cluster size: ${plan.stats.avgClusterSize.toFixed(1)}`);
  console.log(
    `Shared functions: ${plan.stats.sharedFunctions} (${(plan.stats.sharedRatio * 100).toFixed(1)}%)`
  );
  console.log(`Orphan functions: ${plan.stats.orphanFunctions}`);
  console.log(`MQ score: ${plan.stats.mqScore.toFixed(3)}`);

  if (opts.verbose) {
    console.log();
    console.log("Cluster details:");
    for (const cluster of plan.clusters) {
      console.log(
        `  ${cluster.id}: ${cluster.members.size} members, roots: ${cluster.rootFunctions.join(", ")}`
      );
    }

    if (plan.shared.size > 0) {
      console.log();
      console.log(`Shared: ${Array.from(plan.shared).sort().join(", ")}`);
    }
  }
}

function writeManifestAndListFiles(
  plan: SplitPlan,
  inputPath: string,
  outputDir: string,
  isDryRun: boolean
): void {
  fs.mkdirSync(outputDir, { recursive: true });
  const manifest = generateManifest(plan, [inputPath]);
  const manifestPath = path.join(outputDir, "manifest.json");
  fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log();
  console.log(`Manifest written to: ${manifestPath}`);

  if (!isDryRun) {
    const emittedFiles = listJsFilesRecursive(outputDir, outputDir).sort();
    console.log();
    console.log("Emitted files:");
    for (const f of emittedFiles) {
      const stats = fs.statSync(path.join(outputDir, f));
      const lines = fs
        .readFileSync(path.join(outputDir, f), "utf-8")
        .split("\n").length;
      console.log(`  ${f} (${lines} lines, ${stats.size} bytes)`);
    }
  }
}

export function configureSplitCommand(program: Command): void {
  program
    .command("split")
    .description("Split a unminified JavaScript file into multiple modules")
    .argument("<input>", "Input file or directory of .js files")
    .option("-o, --output <dir>", "Output directory", "split-output")
    .option("--dry-run", "Show proposed structure without writing files")
    .option("-v, --verbose", "Show clustering stats")
    .option(
      "--min-cluster-size <n>",
      "Merge clusters with this many or fewer members (0 = no merging)",
      "0"
    )
    .option(
      "--proximity",
      "Merge isolated singletons into nearest cluster by source proximity"
    )
    .option(
      "--split-strategy <strategy>",
      "Split strategy: auto, esbuild-esm, esbuild-cjs, call-graph (default: auto)"
    )
    .action(async (input: string, opts: SplitOpts) => {
      const parsed = parseSplitInput(input);
      if ("error" in parsed) {
        console.error(parsed.error);
        process.exit(1);
      }
      const { inputPath } = parsed;

      if (opts.splitStrategy && !VALID_STRATEGIES.has(opts.splitStrategy)) {
        console.error(
          `Error: Invalid split strategy "${opts.splitStrategy}". ` +
            `Valid options: ${Array.from(VALID_STRATEGIES).join(", ")}`
        );
        process.exit(1);
      }

      const isDryRun = !!opts.dryRun;
      const minClusterSize = parseInt(opts.minClusterSize, 10);
      const strategy =
        opts.splitStrategy && opts.splitStrategy !== "auto"
          ? (opts.splitStrategy as SplitStrategyType)
          : undefined;
      const clusterOptions = {
        minClusterSize: minClusterSize > 0 ? minClusterSize : undefined,
        proximityFallback: opts.proximity,
        splitStrategy: strategy
      };

      console.log(`Splitting: ${inputPath}`);
      console.log(`Mode: ${isDryRun ? "dry-run" : "emit"}`);
      console.log();

      const outputDir = path.resolve(opts.output);
      const plan = buildPlan(inputPath, outputDir, isDryRun, clusterOptions);

      printPlanSummary(plan, opts);

      writeManifestAndListFiles(plan, inputPath, outputDir, isDryRun);
    });
}
