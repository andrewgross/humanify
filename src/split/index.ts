import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import { clusterFunctions } from "./cluster.js";
import { collectLedger, assignEntry, verifyComplete, summarize } from "./ledger.js";
import { nameCluster } from "./naming.js";
import { computeMQ } from "./quality.js";
import type { SplitPlan, SplitStats } from "./types.js";
import type { ClusterOptions } from "./cluster.js";

export interface SplitOptions extends ClusterOptions {}

/**
 * Run the split pipeline in dry-run mode.
 * Parses input, builds graph, clusters, verifies ledger, returns SplitPlan.
 */
export function splitDryRun(inputPaths: string[], options?: SplitOptions): SplitPlan {
  // Parse all input files
  const asts: { ast: t.File; filePath: string }[] = [];

  for (const inputPath of inputPaths) {
    const stat = fs.statSync(inputPath);
    const files = stat.isDirectory()
      ? fs.readdirSync(inputPath)
          .filter(f => f.endsWith(".js"))
          .map(f => path.join(inputPath, f))
      : [inputPath];

    for (const filePath of files) {
      const source = fs.readFileSync(filePath, "utf-8");
      // Try module first, fall back to script for scope-hoisted bundles
      // that may have duplicate declarations from LLM rename collisions
      let ast: t.File | null = null;
      for (const sourceType of ["module", "script"] as const) {
        try {
          const result = parseSync(source, { sourceType, filename: filePath });
          if (result && result.type === "File") {
            ast = result;
            break;
          }
        } catch {
          // Try next sourceType
        }
      }
      if (!ast) {
        throw new Error(`Failed to parse ${filePath}`);
      }
      asts.push({ ast, filePath });
    }
  }

  // Build function graphs and ledgers
  const allFunctions = [];
  const ledger = { entries: new Map(), duplicated: new Map() } as SplitPlan["ledger"];

  for (const { ast, filePath } of asts) {
    const functions = buildFunctionGraph(ast, filePath);
    allFunctions.push(...functions);

    const fileLedger = collectLedger(ast, filePath);
    for (const [id, entry] of fileLedger.entries) {
      ledger.entries.set(id, entry);
    }
  }

  // Cluster
  const { clusters, shared, orphans } = clusterFunctions(allFunctions, options);

  // Build function name map for naming
  const functionNames = new Map<string, string>();
  for (const fn of allFunctions) {
    const node = fn.path.node;
    if ("id" in node && node.id && node.id.name) {
      functionNames.set(fn.sessionId, node.id.name);
    }
  }

  // Name clusters and assign ledger entries
  const clusterFileMap = new Map<string, string>(); // sessionId → filename

  for (const cluster of clusters) {
    const fileName = nameCluster(cluster, functionNames);
    for (const member of cluster.members) {
      clusterFileMap.set(member, fileName);
    }
  }

  for (const sessionId of shared) {
    clusterFileMap.set(sessionId, "shared.js");
  }

  for (const sessionId of orphans) {
    clusterFileMap.set(sessionId, "orphans.js");
  }

  // Assign ledger entries based on function clustering
  // For now, assign each top-level statement to the cluster of the first function it contains
  for (const [entryId, entry] of ledger.entries) {
    // Try to find a function in this statement
    const matchingFn = allFunctions.find(fn => {
      const fnLine = fn.path.node.loc?.start.line;
      const entryLine = entry.node.loc?.start.line;
      return fnLine === entryLine && entry.source === fn.sessionId.split(":")[0];
    });

    if (matchingFn && clusterFileMap.has(matchingFn.sessionId)) {
      assignEntry(ledger, entryId, clusterFileMap.get(matchingFn.sessionId)!);
    } else {
      // Non-function statements go to shared for now
      assignEntry(ledger, entryId, "shared.js");
    }
  }

  // Verify ledger completeness
  verifyComplete(ledger);

  // Compute quality
  const mqScore = computeMQ(clusters, allFunctions);

  const totalFunctions = allFunctions.filter(fn => !fn.scopeParent).length;
  const stats: SplitStats = {
    totalFunctions,
    totalClusters: clusters.length,
    avgClusterSize: clusters.length > 0
      ? clusters.reduce((sum, c) => sum + c.members.size, 0) / clusters.length
      : 0,
    sharedFunctions: shared.size,
    sharedRatio: totalFunctions > 0 ? shared.size / totalFunctions : 0,
    orphanFunctions: orphans.size,
    mqScore,
  };

  return { clusters, shared, orphans, ledger, stats };
}

/**
 * Generate the manifest.json content from a SplitPlan.
 */
export function generateManifest(plan: SplitPlan, inputFiles: string[]): object {
  const ledgerSummary = summarize(plan.ledger);

  return {
    version: 1,
    inputFiles,
    clusters: plan.clusters.map(c => ({
      id: c.id,
      rootFunctions: c.rootFunctions,
      memberCount: c.members.size,
      memberHashes: c.memberHashes,
      members: Array.from(c.members).sort(),
    })),
    shared: Array.from(plan.shared).sort(),
    orphans: Array.from(plan.orphans).sort(),
    stats: plan.stats,
    ledger: {
      totalEntries: ledgerSummary.totalEntries,
      assignedEntries: ledgerSummary.assignedEntries,
      unassignedEntries: ledgerSummary.unassignedEntries,
      outputFiles: ledgerSummary.outputFiles,
    },
  };
}
