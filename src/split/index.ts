import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import type { FunctionNode } from "../analysis/types.js";
import type { ClusterOptions } from "./cluster.js";
import { clusterFunctions } from "./cluster.js";
import {
  buildFileContents,
  collectReferencedNames,
  extractDeclaredNames
} from "./emitter.js";
import {
  assignEntry,
  collectLedger,
  summarize,
  verifyComplete
} from "./ledger.js";
import { nameCluster } from "./naming.js";
import { computeMQ } from "./quality.js";
import type {
  Cluster,
  ParsedFile,
  SplitLedger,
  SplitLedgerEntry,
  SplitPlan,
  SplitStats
} from "./types.js";

export interface SplitOptions extends ClusterOptions {}

/**
 * Parse input paths into ParsedFile objects.
 */
function parseInputFiles(inputPaths: string[]): ParsedFile[] {
  const parsed: ParsedFile[] = [];

  for (const inputPath of inputPaths) {
    const stat = fs.statSync(inputPath);
    const files = stat.isDirectory()
      ? fs
          .readdirSync(inputPath)
          .filter((f) => f.endsWith(".js"))
          .map((f) => path.join(inputPath, f))
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
  const ledger = {
    entries: new Map(),
    duplicated: new Map()
  } as SplitPlan["ledger"];

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

  // Reassign public-export orphans to the best-matching cluster
  reassignPublicOrphans(parsedFiles, allFunctions, clusters, orphans);

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
    const matchingFn = allFunctions.find((fn) => {
      const fnLine = fn.path.node.loc?.start.line;
      const entryLine = entry.node.loc?.start.line;
      return (
        fnLine === entryLine && entry.source === fn.sessionId.split(":")[0]
      );
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
          referencingFiles.set(
            ownerFile,
            (referencingFiles.get(ownerFile) ?? 0) + 1
          );
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

  // Post-processing: resolve circular imports and clean up shared.js
  resolveImportCycles(ledger);

  // Verify ledger completeness
  verifyComplete(ledger);

  // Compute quality
  const mqScore = computeMQ(clusters, allFunctions);

  const totalFunctions = allFunctions.filter((fn) => !fn.scopeParent).length;
  const stats: SplitStats = {
    totalFunctions,
    totalClusters: clusters.length,
    avgClusterSize:
      clusters.length > 0
        ? clusters.reduce((sum, c) => sum + c.members.size, 0) / clusters.length
        : 0,
    sharedFunctions: shared.size,
    sharedRatio: totalFunctions > 0 ? shared.size / totalFunctions : 0,
    orphanFunctions: orphans.size,
    mqScore
  };

  return {
    plan: { clusters, shared, orphans, ledger, stats },
    allFunctions
  };
}

/**
 * Run the split pipeline in dry-run mode.
 * Parses input, builds graph, clusters, verifies ledger, returns SplitPlan.
 */
export function splitDryRun(
  inputPaths: string[],
  options?: SplitOptions
): SplitPlan {
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
 * Reassign orphan functions that are public exports to the best-matching cluster.
 *
 * An orphan may have zero internal callers/callees (e.g., `toChildArray` in Preact)
 * but still be a public API export. Rather than putting it in orphans.js, place it
 * with the cluster whose functions it references most.
 */
function reassignPublicOrphans(
  parsedFiles: ParsedFile[],
  allFunctions: FunctionNode[],
  clusters: Cluster[],
  orphans: Set<string>
): void {
  if (orphans.size === 0 || clusters.length === 0) return;

  // Extract barrel export names from the AST (look for `export { ... }` without declaration)
  const barrelExportNames = new Set<string>();
  for (const { ast } of parsedFiles) {
    for (const stmt of ast.program.body) {
      if (
        t.isExportNamedDeclaration(stmt) &&
        !stmt.declaration &&
        stmt.specifiers.length > 0
      ) {
        for (const spec of stmt.specifiers) {
          if (t.isExportSpecifier(spec)) {
            barrelExportNames.add(spec.local.name);
          }
        }
      }
    }
  }

  if (barrelExportNames.size === 0) return;

  // Build orphan sessionId → function name map
  const orphanNames = new Map<string, string>();
  for (const fn of allFunctions) {
    if (!orphans.has(fn.sessionId)) continue;
    const node = fn.path.node;
    if ("id" in node && node.id && node.id.name) {
      orphanNames.set(fn.sessionId, node.id.name);
    }
  }

  // Build name → cluster index for all cluster members
  const nameToClusterIdx = new Map<string, number>();
  for (let ci = 0; ci < clusters.length; ci++) {
    for (const memberId of clusters[ci].members) {
      const fn = allFunctions.find((f) => f.sessionId === memberId);
      if (fn) {
        const node = fn.path.node;
        if ("id" in node && node.id && node.id.name) {
          nameToClusterIdx.set(node.id.name, ci);
        }
      }
    }
  }

  // For each orphan that is a public export, try to reassign
  const toReassign: Array<{ sessionId: string; clusterIdx: number }> = [];

  for (const sessionId of orphans) {
    const fnName = orphanNames.get(sessionId);
    if (!fnName || !barrelExportNames.has(fnName)) continue;

    // Find the orphan's FunctionNode to get its body references
    const fn = allFunctions.find((f) => f.sessionId === sessionId);
    if (!fn) continue;

    // Find the ledger entry for this function to collect referenced names
    // We need the AST node — use the function's path node directly
    const bodyNode = fn.path.node;
    // Wrap in a statement for collectReferencedNames
    const refs = collectReferencedNames(
      t.isFunctionDeclaration(bodyNode)
        ? bodyNode
        : t.expressionStatement(bodyNode as any)
    );

    // Count which cluster owns the most referenced names
    const clusterCounts = new Map<number, number>();
    for (const ref of refs) {
      const ci = nameToClusterIdx.get(ref);
      if (ci !== undefined) {
        clusterCounts.set(ci, (clusterCounts.get(ci) ?? 0) + 1);
      }
    }

    let bestCluster = -1;
    let bestCount = 0;

    if (clusterCounts.size > 0) {
      for (const [ci, count] of clusterCounts) {
        if (
          count > bestCount ||
          (count === bestCount &&
            (bestCluster === -1 || clusters[ci].id < clusters[bestCluster].id))
        ) {
          bestCluster = ci;
          bestCount = count;
        }
      }
    }

    // Fallback: largest cluster
    if (bestCluster === -1) {
      let maxSize = 0;
      for (let ci = 0; ci < clusters.length; ci++) {
        if (
          clusters[ci].members.size > maxSize ||
          (clusters[ci].members.size === maxSize &&
            (bestCluster === -1 || clusters[ci].id < clusters[bestCluster].id))
        ) {
          maxSize = clusters[ci].members.size;
          bestCluster = ci;
        }
      }
    }

    if (bestCluster >= 0) {
      toReassign.push({ sessionId, clusterIdx: bestCluster });
    }
  }

  // Execute reassignments
  for (const { sessionId, clusterIdx } of toReassign) {
    orphans.delete(sessionId);
    clusters[clusterIdx].members.add(sessionId);
  }
}

/**
 * Post-processing: ensure shared.js is a leaf (no imports) and break circular imports.
 *
 * Strategy:
 * 1. Iteratively move entries out of shared.js until it has no imports from other files.
 *    Each entry with external deps goes to its primary consumer. Iterating handles
 *    cascading dependencies (e.g., moving `options` makes `oldBeforeDiff` need to move too).
 * 2. Break remaining 2-file cycles by moving the declaring entries for the minority
 *    import direction to the other file (e.g., move `doRender` from hooks→core).
 */
function resolveImportCycles(ledger: SplitLedger): void {
  // Helper: rebuild the name→file map and per-file import graph from current assignments
  function rebuildGraph() {
    const nameToFile = new Map<string, string>();
    const fileEntries = new Map<string, SplitLedgerEntry[]>();

    for (const entry of ledger.entries.values()) {
      const file = entry.outputFile;
      if (!file || file === "index.js") continue;
      if (!fileEntries.has(file)) fileEntries.set(file, []);
      fileEntries.get(file)!.push(entry);
      for (const name of extractDeclaredNames(entry.node)) {
        nameToFile.set(name, file);
      }
    }

    // Build per-file: local names and imports
    const fileImports = new Map<string, Map<string, Set<string>>>();
    for (const [fileName, entries] of fileEntries) {
      const localNames = new Set<string>();
      for (const entry of entries) {
        for (const name of extractDeclaredNames(entry.node)) {
          localNames.add(name);
        }
      }

      const imports = new Map<string, Set<string>>();
      for (const entry of entries) {
        const refs = collectReferencedNames(entry.node);
        for (const ref of refs) {
          if (localNames.has(ref)) continue;
          const fromFile = nameToFile.get(ref);
          if (fromFile && fromFile !== fileName) {
            if (!imports.has(fromFile)) imports.set(fromFile, new Set());
            imports.get(fromFile)!.add(ref);
          }
        }
      }
      fileImports.set(fileName, imports);
    }

    return { nameToFile, fileEntries, fileImports };
  }

  // Step 1: Iteratively clean shared.js — move entries with external deps to consumers
  for (let iteration = 0; iteration < 10; iteration++) {
    const { nameToFile, fileEntries, fileImports } = rebuildGraph();
    const sharedImports = fileImports.get("shared.js");
    if (!sharedImports || sharedImports.size === 0) break; // shared.js is clean

    const sharedEntries = fileEntries.get("shared.js") || [];
    let moved = false;

    for (const entry of sharedEntries) {
      const refs = collectReferencedNames(entry.node);
      const declNames = extractDeclaredNames(entry.node);

      // Check if this entry references names from other files
      let hasExternalDeps = false;
      for (const ref of refs) {
        const fromFile = nameToFile.get(ref);
        if (fromFile && fromFile !== "shared.js") {
          hasExternalDeps = true;
          break;
        }
      }

      if (!hasExternalDeps) continue;

      // Find which file consumes the names this entry declares
      const consumerCounts = new Map<string, number>();
      for (const declName of declNames) {
        // Check all other files' imports from shared.js
        for (const [otherFile, otherImports] of fileImports) {
          if (otherFile === "shared.js") continue;
          if (otherImports.get("shared.js")?.has(declName)) {
            consumerCounts.set(
              otherFile,
              (consumerCounts.get(otherFile) ?? 0) + 1
            );
          }
        }
      }

      // For expression statements (no declared names), use referenced names
      if (declNames.length === 0) {
        for (const ref of refs) {
          const ownerFile = nameToFile.get(ref);
          if (ownerFile && ownerFile !== "shared.js") {
            consumerCounts.set(
              ownerFile,
              (consumerCounts.get(ownerFile) ?? 0) + 1
            );
          }
        }
      }

      if (consumerCounts.size > 0) {
        let bestFile = "shared.js";
        let bestCount = 0;
        for (const [file, count] of consumerCounts) {
          if (count > bestCount) {
            bestCount = count;
            bestFile = file;
          }
        }
        if (bestFile !== "shared.js") {
          entry.outputFile = bestFile;
          moved = true;
        }
      }
    }

    if (!moved) break; // No progress, stop iterating
  }

  // Step 2: Break remaining 2-file cycles
  const { fileImports } = rebuildGraph();
  const processed = new Set<string>();

  for (const [fileA, importsA] of fileImports) {
    for (const [fileB] of importsA) {
      const key = [fileA, fileB].sort().join("\u2194");
      if (processed.has(key)) continue;
      processed.add(key);

      const importsB = fileImports.get(fileB);
      if (!importsB?.has(fileA)) continue;

      // Circular: fileA imports from fileB AND fileB imports from fileA
      const namesAFromB = importsA.get(fileB)!;
      const namesBFromA = importsB.get(fileA)!;

      // Move the declaring entries for the minority direction
      // If A imports 1 name from B and B imports 9 from A, move that 1 entry to A
      const moveFromB = namesAFromB.size <= namesBFromA.size;
      const namesToMove = moveFromB ? namesAFromB : namesBFromA;
      const sourceFile = moveFromB ? fileB : fileA;
      const targetFile = moveFromB ? fileA : fileB;

      for (const name of namesToMove) {
        // Find the entry in sourceFile that declares this name
        for (const entry of ledger.entries.values()) {
          if (entry.outputFile !== sourceFile) continue;
          const declared = extractDeclaredNames(entry.node);
          if (declared.includes(name)) {
            entry.outputFile = targetFile;
          }
        }
      }
    }
  }
}

/**
 * Generate the manifest.json content from a SplitPlan.
 */
export function generateManifest(
  plan: SplitPlan,
  inputFiles: string[]
): object {
  const ledgerSummary = summarize(plan.ledger);

  return {
    version: 1,
    inputFiles,
    clusters: plan.clusters.map((c) => ({
      id: c.id,
      rootFunctions: c.rootFunctions,
      memberCount: c.members.size,
      memberHashes: c.memberHashes,
      members: Array.from(c.members).sort()
    })),
    shared: Array.from(plan.shared).sort(),
    orphans: Array.from(plan.orphans).sort(),
    stats: plan.stats,
    ledger: {
      totalEntries: ledgerSummary.totalEntries,
      assignedEntries: ledgerSummary.assignedEntries,
      unassignedEntries: ledgerSummary.unassignedEntries,
      outputFiles: ledgerSummary.outputFiles
    }
  };
}
