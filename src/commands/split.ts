import type { Command } from "commander";
import * as fs from "node:fs";
import * as path from "node:path";
import { splitDryRun, generateManifest } from "../split/index.js";

export function configureSplitCommand(program: Command): void {
  program
    .command("split")
    .description("Split a unminified JavaScript file into multiple modules")
    .argument("<input>", "Input file or directory of .js files")
    .option("-o, --output <dir>", "Output directory", "split-output")
    .option("--dry-run", "Show proposed structure without writing files", true)
    .option("-v, --verbose", "Show clustering stats")
    .option("--min-cluster-size <n>", "Merge clusters with this many or fewer members (0 = no merging)", "0")
    .option("--proximity", "Merge isolated singletons into nearest cluster by source proximity")
    .action(async (input: string, opts: { output: string; dryRun: boolean; verbose?: boolean; minClusterSize: string; proximity?: boolean }) => {
      const inputPath = path.resolve(input);

      if (!fs.existsSync(inputPath)) {
        console.error(`Error: Input path does not exist: ${inputPath}`);
        process.exit(1);
      }

      console.log(`Splitting: ${inputPath}`);
      console.log(`Mode: dry-run (Phase 1)`);
      console.log();

      const minClusterSize = parseInt(opts.minClusterSize, 10);
      const plan = splitDryRun([inputPath], {
        minClusterSize: minClusterSize > 0 ? minClusterSize : undefined,
        proximityFallback: opts.proximity,
      });

      // Print summary
      console.log(`Clusters: ${plan.stats.totalClusters}`);
      console.log(`Total top-level functions: ${plan.stats.totalFunctions}`);
      console.log(`Avg cluster size: ${plan.stats.avgClusterSize.toFixed(1)}`);
      console.log(`Shared functions: ${plan.stats.sharedFunctions} (${(plan.stats.sharedRatio * 100).toFixed(1)}%)`);
      console.log(`Orphan functions: ${plan.stats.orphanFunctions}`);
      console.log(`MQ score: ${plan.stats.mqScore.toFixed(3)}`);

      if (opts.verbose) {
        console.log();
        console.log("Cluster details:");
        for (const cluster of plan.clusters) {
          console.log(`  ${cluster.id}: ${cluster.members.size} members, roots: ${cluster.rootFunctions.join(", ")}`);
        }

        if (plan.shared.size > 0) {
          console.log();
          console.log(`Shared: ${Array.from(plan.shared).sort().join(", ")}`);
        }
      }

      // Write manifest
      const outputDir = path.resolve(opts.output);
      fs.mkdirSync(outputDir, { recursive: true });

      const manifest = generateManifest(plan, [inputPath]);
      const manifestPath = path.join(outputDir, "manifest.json");
      fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");
      console.log();
      console.log(`Manifest written to: ${manifestPath}`);
    });
}
