import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import { clusterFunctions } from "./cluster.js";
import { collectLedger, assignEntry, verifyComplete, summarize } from "./ledger.js";
import { nameCluster } from "./naming.js";
import { computeMQ } from "./quality.js";
import { extractDeclaredNames, collectReferencedNames, buildFileContents } from "./emitter.js";
import type { SplitPlan, SplitStats, ParsedFile } from "./types.js";
import type { ClusterOptions } from "./cluster.js";
import type { FunctionNode } from "../analysis/types.js";

export interface SplitOptions extends ClusterOptions {}

/**
 * Parse input paths into ParsedFile objects.
 */
function parseInputFiles(inputPaths: string[]): ParsedFile[] {
  const parsed: ParsedFile[] = [];

  for (const inputPath of inputPaths) {
    const stat = fs.statSync(inputPath);
    const files = stat.isDirectory()
      ? fs.readdirSync(inputPath)
          .filter(f => f.endsWith(".js"))
          .map(f => path.join(inputPath, f))
      : [inputPath];

    for (const filePath of files) {
      const source = fs.readFileSync(filePath, "utf-8");
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
      parsed.push({ ast, filePath, source });
    }
  }

  return parsed;
}

/**
 * Build the split plan from parsed files.
 * Returns the plan and parsed files (for later emission).
 */
function buildSplitPlan(
  parsedFiles: ParsedFile[],
  options?: SplitOptions
): { plan: SplitPlan; allFunctions: FunctionNode[] } {
  // Build function graphs and ledgers
  const allFunctions: FunctionNode[] = [];
  const ledger = { entries: new Map(), duplicated: new Map() } as SplitPlan["ledger"];

  for (const { ast, filePath } of parsedFiles) {
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

  // Name clusters and build sessionId → filename map
  const clusterFileMap = new Map<string, string>();

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

  // Build nameToFile for non-function statement assignment
  // Maps every top-level declared name (from functions) to its output file
  const nameToFile = new Map<string, string>();
  for (const fn of allFunctions) {
    const node = fn.path.node;
    if ("id" in node && node.id && node.id.name) {
      const file = clusterFileMap.get(fn.sessionId);
      if (file) nameToFile.set(node.id.name, file);
    }
  }

  // Assign ledger entries
  for (const [entryId, entry] of ledger.entries) {
    // 1. Try matching to a clustered function by line number
    const matchingFn = allFunctions.find(fn => {
      const fnLine = fn.path.node.loc?.start.line;
      const entryLine = entry.node.loc?.start.line;
      return fnLine === entryLine && entry.source === fn.sessionId.split(":")[0];
    });

    if (matchingFn && clusterFileMap.has(matchingFn.sessionId)) {
      assignEntry(ledger, entryId, clusterFileMap.get(matchingFn.sessionId)!);
      continue;
    }

    // 2. ExportNamedDeclaration without declaration (barrel export) → index.js
    if (
      t.isExportNamedDeclaration(entry.node) &&
      !entry.node.declaration &&
      entry.node.specifiers.length > 0
    ) {
      assignEntry(ledger, entryId, "index.js");
      continue;
    }

    // 3. Variable/expression: check which files reference or own the declared names
    const declaredNames = extractDeclaredNames(entry.node);
    const referencedNames = collectReferencedNames(entry.node);

    if (declaredNames.length > 0) {
      // Find which files reference these declared names
      const referencingFiles = new Map<string, number>();
      for (const name of declaredNames) {
        for (const [, otherEntry] of ledger.entries) {
          if (otherEntry === entry) continue;
          if (!otherEntry.outputFile) continue;
          // Check if any already-assigned entry references this name
          const otherRefs = collectReferencedNames(otherEntry.node);
          if (otherRefs.has(name)) {
            const file = otherEntry.outputFile;
            referencingFiles.set(file, (referencingFiles.get(file) ?? 0) + 1);
          }
        }
      }

      // Also check which names this statement references (what it depends on)
      for (const ref of referencedNames) {
        const ownerFile = nameToFile.get(ref);
        if (ownerFile) {
          referencingFiles.set(ownerFile, (referencingFiles.get(ownerFile) ?? 0) + 1);
        }
      }

      if (referencingFiles.size === 1) {
        assignEntry(ledger, entryId, referencingFiles.keys().next().value!);
        continue;
      }

      if (referencingFiles.size > 1) {
        // Assign to the file with the most references
        let bestFile = "shared.js";
        let bestCount = 0;
        for (const [file, count] of referencingFiles) {
          if (count > bestCount) {
            bestCount = count;
            bestFile = file;
          }
        }
        assignEntry(ledger, entryId, bestFile);
        continue;
      }
    }

    // 4. Expression statements: look at referenced identifiers
    if (declaredNames.length === 0 && referencedNames.size > 0) {
      const fileCounts = new Map<string, number>();
      for (const ref of referencedNames) {
        const ownerFile = nameToFile.get(ref);
        if (ownerFile) {
          fileCounts.set(ownerFile, (fileCounts.get(ownerFile) ?? 0) + 1);
        }
      }
      if (fileCounts.size > 0) {
        let bestFile = "shared.js";
        let bestCount = 0;
        for (const [file, count] of fileCounts) {
          if (count > bestCount) {
            bestCount = count;
            bestFile = file;
          }
        }
        assignEntry(ledger, entryId, bestFile);
        continue;
      }
    }

    // 5. Fallback: shared.js
    assignEntry(ledger, entryId, "shared.js");
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

  return {
    plan: { clusters, shared, orphans, ledger, stats },
    allFunctions,
  };
}

/**
 * Run the split pipeline in dry-run mode.
 * Parses input, builds graph, clusters, verifies ledger, returns SplitPlan.
 */
export function splitDryRun(inputPaths: string[], options?: SplitOptions): SplitPlan {
  const parsedFiles = parseInputFiles(inputPaths);
  const { plan } = buildSplitPlan(parsedFiles, options);
  return plan;
}

/**
 * Run the split pipeline and emit output files.
 * Returns the plan and writes files to outputDir.
 */
export function splitAndEmit(
  inputPaths: string[],
  outputDir: string,
  options?: SplitOptions
): SplitPlan {
  const parsedFiles = parseInputFiles(inputPaths);
  const { plan } = buildSplitPlan(parsedFiles, options);

  const fileContents = buildFileContents(plan, parsedFiles);

  fs.mkdirSync(outputDir, { recursive: true });
  for (const [fileName, content] of fileContents) {
    const filePath = path.join(outputDir, fileName);
    fs.writeFileSync(filePath, content);
  }

  return plan;
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
