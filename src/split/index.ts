import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import type { FunctionNode } from "../analysis/types.js";
import {
  buildClusterFileMap,
  buildFunctionNameMap
} from "./adapters/call-graph.js";
import { selectSplitAdapter } from "./adapters/index.js";
import type { SplitStrategyType } from "./adapters/types.js";
import type { ClusterOptions } from "./cluster.js";
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
import { detectModules } from "./module-detect.js";
import { computeMQ } from "./quality.js";
import type {
  Cluster,
  ParsedFile,
  SplitLedger,
  SplitLedgerEntry,
  SplitPlan,
  SplitStats
} from "./types.js";

interface SplitOptions extends ClusterOptions {
  /** Force a specific split strategy. If omitted, auto-detects. */
  splitStrategy?: SplitStrategyType;
  /** Pre-computed module detection result. If omitted, runs detectModules(). */
  detection?: import("./module-detect.js").ModuleDetectionResult;
}

// Re-export for backward compatibility (experiments/run.ts uses these)
export { buildClusterFileMap, buildFunctionNameMap };

/** Format a single parse error with available diagnostic details. */
function formatParseError(sourceType: string, error: unknown): string {
  const msg = error instanceof Error ? error.message : String(error);
  const parts = [`  sourceType "${sourceType}": ${msg}`];
  if (!error || typeof error !== "object") return parts[0];

  const e = error as Record<string, unknown>;
  if (e.reasonCode) parts.push(`    reasonCode: ${e.reasonCode}`);
  if (e.code) parts.push(`    code: ${e.code}`);
  if (e.missingPlugin) {
    const plugins = Array.isArray(e.missingPlugin)
      ? e.missingPlugin.join(", ")
      : e.missingPlugin;
    parts.push(`    missingPlugin: ${plugins}`);
  }
  if (e.loc && typeof e.loc === "object") {
    const loc = e.loc as Record<string, unknown>;
    parts.push(`    location: line ${loc.line}, column ${loc.column}`);
  }
  return parts.join("\n");
}

/** Parse a single JS file, trying module then script sourceType. */
export function parseFile(filePath: string): ParsedFile {
  const source = fs.readFileSync(filePath, "utf-8");
  const errors: Array<{ sourceType: string; error: unknown }> = [];
  for (const sourceType of ["module", "unambiguous", "script"] as const) {
    try {
      const result = parseSync(source, {
        sourceType,
        filename: filePath,
        parserOpts: { errorRecovery: true }
      });
      if (result && result.type === "File") {
        return { ast: result, filePath, source };
      }
    } catch (e) {
      errors.push({ sourceType, error: e });
    }
  }
  const details = errors
    .map(({ sourceType, error }) => formatParseError(sourceType, error))
    .join("\n");
  const lineCount = source.split("\n").length;
  const sizeKB = (Buffer.byteLength(source, "utf-8") / 1024).toFixed(0);
  throw new Error(
    `Failed to parse ${filePath} (${lineCount} lines, ${sizeKB} KB):\n${details}`
  );
}

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
      parsed.push(parseFile(filePath));
    }
  }

  return parsed;
}

/** Build function graph + ledger from all parsed files. */
function buildGraphAndLedger(parsedFiles: ParsedFile[]): {
  allFunctions: FunctionNode[];
  ledger: SplitPlan["ledger"];
} {
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

  return { allFunctions, ledger };
}

/** Pick the file with the highest count from a file→count map; fallback is "shared.js". */
function pickBestFile(fileCounts: Map<string, number>): string {
  let bestFile = "shared.js";
  let bestCount = 0;
  for (const [file, count] of fileCounts) {
    if (count > bestCount) {
      bestCount = count;
      bestFile = file;
    }
  }
  return bestFile;
}

/** Increment a file count map for all assigned entries that reference a declared name. */
function countFilesReferencingName(
  name: string,
  selfEntry: SplitLedgerEntry,
  ledger: SplitPlan["ledger"],
  fileCounts: Map<string, number>
): void {
  for (const [, otherEntry] of ledger.entries) {
    if (otherEntry === selfEntry) continue;
    if (!otherEntry.outputFile) continue;
    const otherRefs = collectReferencedNames(otherEntry.node);
    if (otherRefs.has(name)) {
      const file = otherEntry.outputFile;
      fileCounts.set(file, (fileCounts.get(file) ?? 0) + 1);
    }
  }
}

/** Count which files reference the declared names of a given entry. */
function countReferencingFiles(
  entry: SplitLedgerEntry,
  declaredNames: string[],
  referencedNames: Set<string>,
  nameToFile: Map<string, string>,
  ledger: SplitPlan["ledger"]
): Map<string, number> {
  const referencingFiles = new Map<string, number>();

  for (const name of declaredNames) {
    countFilesReferencingName(name, entry, ledger, referencingFiles);
  }

  for (const ref of referencedNames) {
    const ownerFile = nameToFile.get(ref);
    if (ownerFile) {
      referencingFiles.set(
        ownerFile,
        (referencingFiles.get(ownerFile) ?? 0) + 1
      );
    }
  }

  return referencingFiles;
}

/** Count which output files own the identifiers referenced by an expression entry. */
function countFilesByRefs(
  referencedNames: Set<string>,
  nameToFile: Map<string, string>
): Map<string, number> {
  const fileCounts = new Map<string, number>();
  for (const ref of referencedNames) {
    const ownerFile = nameToFile.get(ref);
    if (ownerFile) {
      fileCounts.set(ownerFile, (fileCounts.get(ownerFile) ?? 0) + 1);
    }
  }
  return fileCounts;
}

/** Try to assign a single ledger entry. Returns true if assigned. */
function tryAssignEntry(
  entryId: string,
  entry: SplitLedgerEntry,
  allFunctions: FunctionNode[],
  clusterFileMap: Map<string, string>,
  nameToFile: Map<string, string>,
  ledger: SplitPlan["ledger"]
): boolean {
  // 1. Try matching to a clustered function by line number
  const matchingFn = allFunctions.find((fn) => {
    const fnLine = fn.path.node.loc?.start.line;
    const entryLine = entry.node.loc?.start.line;
    return fnLine === entryLine && entry.source === fn.sessionId.split(":")[0];
  });

  if (matchingFn && clusterFileMap.has(matchingFn.sessionId)) {
    assignEntry(
      ledger,
      entryId,
      clusterFileMap.get(matchingFn.sessionId) ?? ""
    );
    return true;
  }

  // 2. Barrel export → index.js
  if (
    t.isExportNamedDeclaration(entry.node) &&
    !entry.node.declaration &&
    entry.node.specifiers.length > 0
  ) {
    assignEntry(ledger, entryId, "index.js");
    return true;
  }

  const declaredNames = extractDeclaredNames(entry.node);
  const referencedNames = collectReferencedNames(entry.node);

  // 3. Variable/expression: check which files reference or own the declared names
  if (declaredNames.length > 0) {
    const referencingFiles = countReferencingFiles(
      entry,
      declaredNames,
      referencedNames,
      nameToFile,
      ledger
    );
    if (referencingFiles.size >= 1) {
      assignEntry(ledger, entryId, pickBestFile(referencingFiles));
      return true;
    }
  }

  // 4. Expression statements: look at referenced identifiers
  if (declaredNames.length === 0 && referencedNames.size > 0) {
    const fileCounts = countFilesByRefs(referencedNames, nameToFile);
    if (fileCounts.size > 0) {
      assignEntry(ledger, entryId, pickBestFile(fileCounts));
      return true;
    }
  }

  return false;
}

/** Reconstruct Cluster objects from a flat sessionId -> fileName map. */
function reconstructClusters(clusterFileMap: Map<string, string>): {
  clusters: Cluster[];
  shared: Set<string>;
  orphans: Set<string>;
} {
  const groups = new Map<string, Set<string>>();
  const shared = new Set<string>();
  const orphans = new Set<string>();

  for (const [sessionId, fileName] of clusterFileMap) {
    if (fileName === "shared.js") {
      shared.add(sessionId);
    } else if (fileName === "orphans.js") {
      orphans.add(sessionId);
    } else {
      const group = groups.get(fileName) ?? new Set<string>();
      group.add(sessionId);
      groups.set(fileName, group);
    }
  }

  const clusters: Cluster[] = Array.from(groups.entries()).map(
    ([name, members]) => ({
      id: name.replace(/\.js$/, "").slice(0, 16),
      rootFunctions: [] as string[],
      members,
      memberHashes: [] as string[]
    })
  );

  return { clusters, shared, orphans };
}

function buildSplitPlan(
  parsedFiles: ParsedFile[],
  options?: SplitOptions
): { plan: SplitPlan; allFunctions: FunctionNode[] } {
  const { allFunctions, ledger } = buildGraphAndLedger(parsedFiles);

  // Detect module boundaries and select adapter
  const detection =
    options?.detection ??
    detectModules(parsedFiles.map((f) => f.source).join("\n"));
  const adapter = selectSplitAdapter(detection, options?.splitStrategy);
  const clusterFileMap = adapter.groupFunctions(
    allFunctions,
    parsedFiles,
    detection,
    options
  );

  // Reconstruct cluster info for stats
  const { clusters, shared, orphans } = reconstructClusters(clusterFileMap);

  // Build nameToFile for non-function statement assignment
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
    const assigned = tryAssignEntry(
      entryId,
      entry,
      allFunctions,
      clusterFileMap,
      nameToFile,
      ledger
    );
    if (!assigned) {
      assignEntry(ledger, entryId, "shared.js");
    }
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
    // Create subdirectories for nested paths (e.g., "src/helpers/util.js")
    const fileDir = path.dirname(filePath);
    if (fileDir !== outputDir) {
      fs.mkdirSync(fileDir, { recursive: true });
    }
    fs.writeFileSync(filePath, content);
  }

  return plan;
}

/** Group ledger entries by output file and build name→file map. */
function groupLedgerEntries(ledger: SplitLedger): {
  nameToFile: Map<string, string>;
  fileEntries: Map<string, SplitLedgerEntry[]>;
} {
  const nameToFile = new Map<string, string>();
  const fileEntries = new Map<string, SplitLedgerEntry[]>();

  for (const entry of ledger.entries.values()) {
    const file = entry.outputFile;
    if (!file || file === "index.js") continue;
    if (!fileEntries.has(file)) fileEntries.set(file, []);
    fileEntries.get(file)?.push(entry);
    for (const name of extractDeclaredNames(entry.node)) {
      nameToFile.set(name, file);
    }
  }

  return { nameToFile, fileEntries };
}

/** Add a ref to the imports map if it resolves to a different file. */
function addRefToImports(
  ref: string,
  fileName: string,
  localNames: Set<string>,
  nameToFile: Map<string, string>,
  imports: Map<string, Set<string>>
): void {
  if (localNames.has(ref)) return;
  const fromFile = nameToFile.get(ref);
  if (fromFile && fromFile !== fileName) {
    if (!imports.has(fromFile)) imports.set(fromFile, new Set());
    imports.get(fromFile)?.add(ref);
  }
}

/** Build the import set for a single file. */
function buildImportsForFile(
  fileName: string,
  entries: SplitLedgerEntry[],
  nameToFile: Map<string, string>
): Map<string, Set<string>> {
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
      addRefToImports(ref, fileName, localNames, nameToFile, imports);
    }
  }
  return imports;
}

/** Build per-file import map from file entries and nameToFile. */
function buildPerFileImports(
  fileEntries: Map<string, SplitLedgerEntry[]>,
  nameToFile: Map<string, string>
): Map<string, Map<string, Set<string>>> {
  const fileImports = new Map<string, Map<string, Set<string>>>();
  for (const [fileName, entries] of fileEntries) {
    fileImports.set(
      fileName,
      buildImportsForFile(fileName, entries, nameToFile)
    );
  }
  return fileImports;
}

/** Rebuild the import graph from current ledger assignments. */
function rebuildImportGraph(ledger: SplitLedger): {
  nameToFile: Map<string, string>;
  fileEntries: Map<string, SplitLedgerEntry[]>;
  fileImports: Map<string, Map<string, Set<string>>>;
} {
  const { nameToFile, fileEntries } = groupLedgerEntries(ledger);
  const fileImports = buildPerFileImports(fileEntries, nameToFile);
  return { nameToFile, fileEntries, fileImports };
}

/** Count files that import a declared name from shared.js. */
function countImportersOfSharedName(
  declName: string,
  fileImports: Map<string, Map<string, Set<string>>>,
  counts: Map<string, number>
): void {
  for (const [otherFile, otherImports] of fileImports) {
    if (otherFile === "shared.js") continue;
    if (otherImports.get("shared.js")?.has(declName)) {
      counts.set(otherFile, (counts.get(otherFile) ?? 0) + 1);
    }
  }
}

/** Count consumer files for a shared entry's declared names (or referenced names if no decls). */
function countSharedConsumers(
  _entry: SplitLedgerEntry,
  refs: Set<string>,
  declNames: string[],
  nameToFile: Map<string, string>,
  fileImports: Map<string, Map<string, Set<string>>>
): Map<string, number> {
  const consumerCounts = new Map<string, number>();

  for (const declName of declNames) {
    countImportersOfSharedName(declName, fileImports, consumerCounts);
  }

  if (declNames.length === 0) {
    for (const ref of refs) {
      const ownerFile = nameToFile.get(ref);
      if (ownerFile && ownerFile !== "shared.js") {
        consumerCounts.set(ownerFile, (consumerCounts.get(ownerFile) ?? 0) + 1);
      }
    }
  }

  return consumerCounts;
}

/** Try to move a single shared.js entry to its best consumer. Returns true if moved. */
function tryMoveSharedEntry(
  entry: SplitLedgerEntry,
  nameToFile: Map<string, string>,
  fileImports: Map<string, Map<string, Set<string>>>
): boolean {
  const refs = collectReferencedNames(entry.node);
  const declNames = extractDeclaredNames(entry.node);

  const hasExternalDeps = Array.from(refs).some((ref) => {
    const fromFile = nameToFile.get(ref);
    return fromFile !== undefined && fromFile !== "shared.js";
  });

  if (!hasExternalDeps) return false;

  const consumerCounts = countSharedConsumers(
    entry,
    refs,
    declNames,
    nameToFile,
    fileImports
  );

  if (consumerCounts.size > 0) {
    const bestFile = pickBestFile(consumerCounts);
    if (bestFile !== "shared.js") {
      entry.outputFile = bestFile;
      return true;
    }
  }

  return false;
}

/** Clean shared.js by iteratively moving entries with external deps to their consumers. */
function cleanSharedJs(ledger: SplitLedger): void {
  for (let iteration = 0; iteration < 10; iteration++) {
    const { nameToFile, fileEntries, fileImports } = rebuildImportGraph(ledger);
    const sharedImports = fileImports.get("shared.js");
    if (!sharedImports || sharedImports.size === 0) break;

    const sharedEntries = fileEntries.get("shared.js") || [];
    let moved = false;

    for (const entry of sharedEntries) {
      if (tryMoveSharedEntry(entry, nameToFile, fileImports)) {
        moved = true;
      }
    }

    if (!moved) break;
  }
}

/** Move all entries declaring a name from one file to another. */
function moveDeclaringEntries(
  ledger: SplitLedger,
  namesToMove: Set<string>,
  sourceFile: string,
  targetFile: string
): void {
  for (const name of namesToMove) {
    for (const entry of ledger.entries.values()) {
      if (entry.outputFile !== sourceFile) continue;
      const declared = extractDeclaredNames(entry.node);
      if (declared.includes(name)) {
        entry.outputFile = targetFile;
      }
    }
  }
}

/** Break a single two-file import cycle by moving minority-direction entries. */
function breakCyclePair(
  ledger: SplitLedger,
  fileA: string,
  fileB: string,
  importsA: Map<string, Set<string>>,
  importsB: Map<string, Set<string>>
): void {
  const namesAFromB = importsA.get(fileB) ?? new Set<string>();
  const namesBFromA = importsB.get(fileA) ?? new Set<string>();
  const moveFromB = namesAFromB.size <= namesBFromA.size;
  const namesToMove = moveFromB ? namesAFromB : namesBFromA;
  const sourceFile = moveFromB ? fileB : fileA;
  const targetFile = moveFromB ? fileA : fileB;
  moveDeclaringEntries(ledger, namesToMove, sourceFile, targetFile);
}

/** Break two-file import cycles by moving minority-direction entries. */
function breakTwoFileCycles(
  ledger: SplitLedger,
  fileImports: Map<string, Map<string, Set<string>>>
): void {
  const processed = new Set<string>();

  for (const [fileA, importsA] of fileImports) {
    for (const [fileB] of importsA) {
      const key = [fileA, fileB].sort().join("\u2194");
      if (processed.has(key)) continue;
      processed.add(key);

      const importsB = fileImports.get(fileB);
      if (!importsB?.has(fileA)) continue;

      breakCyclePair(ledger, fileA, fileB, importsA, importsB);
    }
  }
}

/**
 * Post-processing: ensure shared.js is a leaf (no imports) and break circular imports.
 */
function resolveImportCycles(ledger: SplitLedger): void {
  cleanSharedJs(ledger);

  const { fileImports } = rebuildImportGraph(ledger);
  breakTwoFileCycles(ledger, fileImports);
}

/**
 * Split a pre-parsed AST into multiple output files.
 * Designed for integration into the rename pipeline where the AST
 * has already been parsed and modified (renamed) in memory.
 *
 * @param ast - The (post-rename) AST with original byte positions intact
 * @param filePath - Original file path (used as source identifier in the ledger)
 * @param source - Original source text (used for module detection if no detection provided)
 * @param options - Split options including optional pre-computed detection
 * @returns Map of output filename to file content
 */
export function splitFromAst(
  ast: t.File,
  filePath: string,
  source: string,
  options?: SplitOptions
): Map<string, string> {
  const parsedFiles: ParsedFile[] = [{ ast, filePath, source }];
  const { plan } = buildSplitPlan(parsedFiles, options);
  return buildFileContents(plan, parsedFiles);
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
