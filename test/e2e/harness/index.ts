import { existsSync, readdirSync, readFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { join, basename } from "node:path";
import { setupFixture, loadFixtureConfig, getBuildDir } from "./setup.js";
import {
  minifyFixtureVersion,
  DEFAULT_MINIFIER_CONFIG,
  MINIFIER_CONFIGS,
  getMinifierConfig,
  type MinifierConfig
} from "./minify.js";
import { buildGroundTruth, type GroundTruth } from "./ground-truth.js";
import {
  buildFingerprintData,
  linkMinifiedToSource,
  validate
} from "./validate.js";
import { matchFunctions } from "../../../src/analysis/fingerprint-index.js";
import type { FunctionNode } from "../../../src/analysis/types.js";
import {
  reportResults,
  reportResultsCI,
  reportAggregateSummary,
  type AggregateEntry
} from "./reporter.js";
import { generateDebugArtifacts, getOutputDir } from "./debug.js";
import {
  saveSnapshot,
  compareToSnapshot,
  reportSnapshotComparison
} from "./snapshot.js";
import { extractFunctionCode } from "./code-extractor.js";
import { handleHumanify } from "./humanify.js";

const FIXTURES_DIR = join(import.meta.dirname, "..", "fixtures");

interface ValidateOptions {
  updateSnapshot: boolean;
  ci: boolean;
  verbose: boolean;
  minifier: string | undefined;
  allMinifiers: boolean;
  showDiff: boolean;
}

async function main() {
  const args = process.argv.slice(2);
  const command = args[0];

  switch (command) {
    case "setup":
      await handleSetup(args.slice(1));
      break;
    case "validate":
      await handleValidate(args.slice(1));
      break;
    case "humanify":
      await handleHumanify(args.slice(1));
      break;
    case "debug":
      await handleDebug(args.slice(1));
      break;
    case "list":
      handleList();
      break;
    default:
      printUsage();
      process.exit(1);
  }
}

function printUsage(): void {
  console.log("Usage:");
  console.log("  e2e setup <fixture>              Set up a test fixture");
  console.log("  e2e validate <fixture> [v1] [v2]  Run validation");
  console.log("    Options:");
  console.log("      --update-snapshot            Update stored snapshots");
  console.log(
    "      --ci                         CI mode: compare against snapshots, fail on drift"
  );
  console.log(
    "      --verbose                    Show detailed failure output"
  );
  console.log(
    "      --minifier <id>              Use specific minifier (default: terser-default)"
  );
  console.log(
    "      --all-minifiers              Run with all available minifiers"
  );
  console.log(
    "      --show-diff                  Show source code diff before validation"
  );
  console.log(
    "  e2e humanify <fixture> [version]   Run LLM rename pipeline on minified fixture"
  );
  console.log("    Options:");
  console.log(
    "      --update-snapshot            Save output metrics as baseline"
  );
  console.log(
    "      --ci                         Compare against saved baseline, fail on drift"
  );
  console.log(
    "      -v                           Show timestamped progress and renamed output"
  );
  console.log(
    "      -vv                          Full debug: LLM prompts/responses, stack traces"
  );
  console.log(
    "      --minifier <id>              Use specific minifier (default: terser-default)"
  );
  console.log(
    "      --all-minifiers              Run with all available minifiers"
  );
  console.log("    Env vars:");
  console.log("      HUMANIFY_TEST_BASE_URL       OpenAI-compatible endpoint");
  console.log("      HUMANIFY_TEST_MODEL          Model identifier");
  console.log("      HUMANIFY_TEST_API_KEY        API key (optional)");
  console.log(
    "  e2e debug <fixture> <v1> <v2> --function <name>  Investigate specific function"
  );
  console.log("  e2e list                          List available fixtures");
  console.log("");
  console.log("Available minifiers:");
  for (const config of MINIFIER_CONFIGS) {
    console.log(`  ${config.id} (${config.tool})`);
  }
}

async function handleSetup(args: string[]): Promise<void> {
  const pkg = args[0];
  if (!pkg) {
    console.error("Usage: e2e setup <fixture>");
    process.exit(1);
  }
  await setupFixture(pkg);
}

const BOOLEAN_FLAGS: Record<string, keyof ValidateOptions> = {
  "--update-snapshot": "updateSnapshot",
  "--ci": "ci",
  "--verbose": "verbose",
  "--all-minifiers": "allMinifiers",
  "--show-diff": "showDiff"
};

function parseValidateOptions(args: string[]): {
  positional: string[];
  options: ValidateOptions;
} {
  const positional: string[] = [];
  const options: ValidateOptions = {
    updateSnapshot: false,
    ci: false,
    verbose: false,
    minifier: undefined,
    allMinifiers: false,
    showDiff: false
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    const boolKey = BOOLEAN_FLAGS[arg];
    if (boolKey) {
      (options as unknown as Record<string, unknown>)[boolKey] = true;
    } else if (arg === "--minifier" && args[i + 1]) {
      options.minifier = args[++i];
    } else if (!arg.startsWith("--")) {
      positional.push(arg);
    }
  }

  return { positional, options };
}

/**
 * Resolve the list of minifier configs from CLI options.
 */
function resolveMinifierConfigs(options: ValidateOptions): MinifierConfig[] {
  if (options.allMinifiers) return MINIFIER_CONFIGS;
  if (options.minifier) {
    const minConfig = getMinifierConfig(options.minifier);
    if (!minConfig) {
      console.error(`Unknown minifier: ${options.minifier}`);
      console.error("Available minifiers:");
      for (const c of MINIFIER_CONFIGS) {
        console.error(`  ${c.id}`);
      }
      process.exit(1);
    }
    return [minConfig];
  }
  return [DEFAULT_MINIFIER_CONFIG];
}

/**
 * Build source file descriptors from entry points for a given version.
 */
function buildSourceFileDescriptors(
  config: ReturnType<typeof loadFixtureConfig>,
  pkg: string,
  version: string
): Array<{ path: string; relative: string }> {
  const buildDir = getBuildDir(pkg, version);
  return config.entryPoints.map((e) => {
    const jsEntry = basename(e).replace(/\.ts$/, ".js");
    return { path: join(buildDir, "build", jsEntry), relative: jsEntry };
  });
}

/**
 * Report results for a single validation run, handling CI vs interactive mode.
 * Returns true if the run passed.
 */
function reportValidationResults(
  result: ReturnType<typeof validate>,
  options: ValidateOptions,
  debugDir: string
): boolean {
  if (options.ci) {
    const { passed, summary } = reportResultsCI(result);
    console.log(summary);
    const comparison = compareToSnapshot(result);
    const snapshotPassed = reportSnapshotComparison(comparison);
    return passed && snapshotPassed;
  }

  reportResults(result, { debugDir, verbose: options.verbose });
  if (options.updateSnapshot) {
    const snapshotPath = saveSnapshot(result);
    console.log(`Snapshot updated: ${snapshotPath}`);
  }
  return result.failures.length === 0;
}

/**
 * Run validation for a single pair + minifier combination.
 */
async function validateSinglePair(
  pkg: string,
  pair: {
    v1: string;
    v2: string;
    expectMatchDespiteModification?: Array<{
      function: string;
      reason: string;
    }>;
  },
  config: ReturnType<typeof loadFixtureConfig>,
  minifierConfig: MinifierConfig,
  options: ValidateOptions
): Promise<{ passed: boolean; entry: AggregateEntry }> {
  console.log(
    `\nValidating ${pkg} ${pair.v1} \u2192 ${pair.v2} (${minifierConfig.id})...`
  );

  const v1MinResults = await minifyFixtureVersion(
    pkg,
    pair.v1,
    config,
    minifierConfig
  );
  const v2MinResults = await minifyFixtureVersion(
    pkg,
    pair.v2,
    config,
    minifierConfig
  );

  if (v1MinResults.length === 0 || v2MinResults.length === 0) {
    console.error("Minification produced no results");
    process.exit(1);
  }

  const v1Min = v1MinResults[0];
  const v2Min = v2MinResults[0];

  const v1SourceFiles = buildSourceFileDescriptors(config, pkg, pair.v1);
  const v2SourceFiles = buildSourceFileDescriptors(config, pkg, pair.v2);

  const groundTruth = buildGroundTruth(v1SourceFiles, v2SourceFiles);
  console.log(
    `Ground truth: ${groundTruth.v1Functions.length} v1 fns, ${groundTruth.v2Functions.length} v2 fns, ${groundTruth.correspondence.length} correspondences`
  );

  if (options.showDiff) {
    showGroundTruthDiff(groundTruth, v1SourceFiles, v2SourceFiles);
  }

  const v1Data = buildFingerprintData(v1Min.code, v1Min.minifiedPath);
  const v2Data = buildFingerprintData(v2Min.code, v2Min.minifiedPath);
  console.log(
    `Fingerprints: ${v1Data.index.fingerprints.size} v1, ${v2Data.index.fingerprints.size} v2`
  );

  const matchResult = matchFunctions(v1Data.index, v2Data.index);
  console.log(
    `Matches: ${matchResult.matches.size} matched, ${matchResult.ambiguous.size} ambiguous, ${matchResult.unmatched.length} unmatched`
  );

  const v1LinkResult = await linkMinifiedToSource(
    v1Data.functions,
    groundTruth.v1Functions,
    v1Min.sourceMap
  );
  const v2LinkResult = await linkMinifiedToSource(
    v2Data.functions,
    groundTruth.v2Functions,
    v2Min.sourceMap
  );
  console.log(
    `Links: ${v1LinkResult.links.size} v1, ${v2LinkResult.links.size} v2`
  );

  const result = validate(
    pkg,
    pair.v1,
    pair.v2,
    minifierConfig.id,
    groundTruth,
    v1Data.index,
    v2Data.index,
    matchResult,
    v1LinkResult,
    v2LinkResult,
    pair.expectMatchDespiteModification
  );

  const v1SourceCode = extractFunctionCode(
    groundTruth.v1Functions,
    v1SourceFiles.map((f) => f.path)
  );
  const v2SourceCode = extractFunctionCode(
    groundTruth.v2Functions,
    v2SourceFiles.map((f) => f.path)
  );
  const v1MinifiedCode = extractMinifiedFunctionCode(
    v1Data.functions,
    v1Min.code
  );
  const v2MinifiedCode = extractMinifiedFunctionCode(
    v2Data.functions,
    v2Min.code
  );

  const outputDir = getOutputDir(pkg, pair.v1, pair.v2, minifierConfig.id);
  const debugDir = join(outputDir, "debug");

  generateDebugArtifacts({
    fixture: pkg,
    v1: pair.v1,
    v2: pair.v2,
    minifierConfig: minifierConfig.id,
    groundTruth,
    v1Index: v1Data.index,
    v2Index: v2Data.index,
    matchResult,
    v1Links: v1LinkResult.links,
    v2Links: v2LinkResult.links,
    v1SourceCode,
    v2SourceCode,
    v1MinifiedCode,
    v2MinifiedCode,
    result
  });

  const passed = reportValidationResults(result, options, debugDir);

  return {
    passed,
    entry: {
      pair: `${pair.v1} \u2192 ${pair.v2}`,
      minifier: minifierConfig.id,
      accuracy: result.overallAccuracy,
      passed: result.failures.length === 0
    }
  };
}

async function handleValidate(args: string[]): Promise<void> {
  const { positional, options } = parseValidateOptions(args);
  const pkg = positional[0];

  if (!pkg) {
    console.error(
      "Usage: e2e validate <fixture> [v1] [v2] [--update-snapshot] [--ci] [--verbose] [--minifier <id>] [--all-minifiers]"
    );
    process.exit(1);
  }

  const config = loadFixtureConfig(pkg);
  const minifierConfigs = resolveMinifierConfigs(options);

  let pairs: typeof config.versionPairs = config.versionPairs;
  if (positional[1] && positional[2]) {
    const configPair = config.versionPairs.find(
      (p) => p.v1 === positional[1] && p.v2 === positional[2]
    );
    pairs = [configPair ?? { v1: positional[1], v2: positional[2] }];
  }

  let allPassed = true;
  const aggregateEntries: AggregateEntry[] = [];

  for (const minifierConfig of minifierConfigs) {
    for (const pair of pairs) {
      const { passed, entry } = await validateSinglePair(
        pkg,
        pair,
        config,
        minifierConfig,
        options
      );
      if (!passed) allPassed = false;
      aggregateEntries.push(entry);
    }
  }

  if (minifierConfigs.length > 1) {
    reportAggregateSummary(pkg, aggregateEntries);
  }

  if (!allPassed) {
    process.exit(1);
  }
}

/**
 * Parse debug command arguments.
 */
function parseDebugArgs(args: string[]): {
  positional: string[];
  functionName: string | undefined;
  minifierId: string | undefined;
} {
  const positional: string[] = [];
  let functionName: string | undefined;
  let minifierId: string | undefined;

  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--function" && args[i + 1]) {
      functionName = args[++i];
    } else if (args[i] === "--minifier" && args[i + 1]) {
      minifierId = args[++i];
    } else if (!args[i].startsWith("--")) {
      positional.push(args[i]);
    }
  }

  return { positional, functionName, minifierId };
}

/**
 * Print version function details (location, arity, body hash).
 */
function printVersionFunction(
  label: string,
  fn:
    | {
        location: { startLine: number; endLine: number };
        arity: number;
        bodyHash: string;
      }
    | undefined
): void {
  if (!fn) return;
  console.log(`${label} Function:`);
  console.log(
    `  Location: lines ${fn.location.startLine}-${fn.location.endLine}`
  );
  console.log(`  Arity: ${fn.arity}`);
  console.log(`  Body hash: ${fn.bodyHash}`);
  console.log("");
}

/**
 * Print failure artifacts for a function.
 */
function printFailureArtifacts(
  failuresDir: string,
  functionName: string
): void {
  if (!existsSync(failuresDir)) return;

  const failureDirs = readdirSync(failuresDir, { withFileTypes: true })
    .filter((d) => d.isDirectory() && d.name.startsWith(`${functionName}-`))
    .map((d) => d.name);

  if (failureDirs.length === 0) {
    console.log("No failures recorded for this function.");
    return;
  }

  console.log("Failure artifacts found:");
  for (const dir of failureDirs) {
    console.log(`  ${join(failuresDir, dir)}`);
    const summaryPath = join(failuresDir, dir, "summary.txt");
    if (existsSync(summaryPath)) {
      console.log("");
      console.log(readFileSync(summaryPath, "utf-8"));
    }
  }
}

async function handleDebug(args: string[]): Promise<void> {
  const { positional, functionName, minifierId } = parseDebugArgs(args);
  const [pkg, v1, v2] = positional;

  if (!pkg || !v1 || !v2 || !functionName) {
    console.error(
      "Usage: e2e debug <fixture> <v1> <v2> --function <name> [--minifier <id>]"
    );
    process.exit(1);
  }

  const minifierConfig = minifierId
    ? getMinifierConfig(minifierId)
    : DEFAULT_MINIFIER_CONFIG;
  if (!minifierConfig) {
    console.error(`Unknown minifier: ${minifierId}`);
    process.exit(1);
  }

  const outputDir = getOutputDir(pkg, v1, v2, minifierConfig.id);
  const debugDir = join(outputDir, "debug");

  if (!existsSync(debugDir)) {
    console.error(`Debug artifacts not found at ${debugDir}`);
    console.error("Run 'e2e validate' first to generate debug artifacts.");
    process.exit(1);
  }

  const groundTruthPath = join(debugDir, "ground-truth.json");
  if (!existsSync(groundTruthPath)) {
    console.error("Ground truth file not found. Run 'e2e validate' first.");
    process.exit(1);
  }
  const groundTruth = JSON.parse(readFileSync(groundTruthPath, "utf-8"));

  const correspondence = groundTruth.correspondence.find(
    (c: { sourceName: string }) => c.sourceName === functionName
  );

  if (!correspondence) {
    console.error(`Function '${functionName}' not found in ground truth.`);
    console.error("Available functions:");
    for (const c of groundTruth.correspondence) {
      console.error(`  ${c.sourceName} (${c.changeType})`);
    }
    process.exit(1);
  }

  console.log("");
  console.log(`Function: ${functionName}`);
  console.log(`Change type: ${correspondence.changeType}`);
  console.log(`Source file: ${correspondence.sourceFile}`);
  console.log("");

  const v1Fn = groundTruth.v1Functions.find(
    (f: { name: string }) => f.name === functionName
  );
  const v2Fn = groundTruth.v2Functions.find(
    (f: { name: string }) => f.name === functionName
  );
  printVersionFunction("V1", v1Fn);
  printVersionFunction("V2", v2Fn);

  printFailureArtifacts(join(debugDir, "failures"), functionName);

  const v1FingerprintsPath = join(debugDir, "v1-fingerprints.json");
  const v2FingerprintsPath = join(debugDir, "v2-fingerprints.json");

  if (existsSync(v1FingerprintsPath) && existsSync(v2FingerprintsPath)) {
    console.log("");
    console.log("Fingerprint data stored in:");
    console.log(`  V1: ${v1FingerprintsPath}`);
    console.log(`  V2: ${v2FingerprintsPath}`);
  }
}

type LocNode = FunctionNode;

/**
 * Extract a multi-line code range from source lines.
 */
function extractMultiLineRange(
  lines: string[],
  loc: {
    start: { line: number; column: number };
    end: { line: number; column: number };
  }
): string {
  const extracted: string[] = [];
  for (let i = loc.start.line - 1; i < loc.end.line; i++) {
    if (i === loc.start.line - 1) {
      extracted.push(lines[i].slice(loc.start.column));
    } else if (i === loc.end.line - 1) {
      extracted.push(lines[i].slice(0, loc.end.column));
    } else {
      extracted.push(lines[i]);
    }
  }
  return extracted.join("\n");
}

/**
 * Extract code snippets for minified functions.
 */
function extractMinifiedFunctionCode(
  functions: Map<string, LocNode>,
  code: string
): Map<string, string> {
  const lines = code.split("\n");
  const result = new Map<string, string>();

  for (const [sessionId, fn] of functions) {
    const loc = fn.path.node.loc;
    if (!loc) continue;

    if (loc.start.line === loc.end.line) {
      const line = lines[loc.start.line - 1] || "";
      result.set(sessionId, line.slice(loc.start.column, loc.end.column));
    } else {
      result.set(sessionId, extractMultiLineRange(lines, loc));
    }
  }

  return result;
}

/**
 * Print details for a single fixture.
 */
function printFixtureInfo(name: string): void {
  const configPath = join(FIXTURES_DIR, name, "fixture.config.json");
  if (!existsSync(configPath)) {
    console.log(`  ${name} (no config)`);
    return;
  }
  const config = JSON.parse(readFileSync(configPath, "utf-8"));
  const setupDone = existsSync(join(FIXTURES_DIR, name, "source"));
  const status = setupDone ? "ready" : "needs setup";
  console.log(`  ${name} (${status})`);
  for (const pair of config.versionPairs) {
    console.log(
      `    ${pair.v1} \u2192 ${pair.v2}${pair.description ? ` (${pair.description})` : ""}`
    );
  }
}

function handleList(): void {
  if (!existsSync(FIXTURES_DIR)) {
    console.log("No fixtures directory found.");
    return;
  }

  const fixtures = readdirSync(FIXTURES_DIR, { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => d.name);

  if (fixtures.length === 0) {
    console.log("No fixtures found.");
    return;
  }

  console.log("Available fixtures:");
  for (const name of fixtures) {
    printFixtureInfo(name);
  }
}

/**
 * Show a unified diff of source files between versions.
 */
function showGroundTruthDiff(
  _groundTruth: GroundTruth,
  v1Files: Array<{ path: string; relative: string }>,
  v2Files: Array<{ path: string; relative: string }>
): void {
  console.log("");
  console.log("┌─────────────────────────────────────────┐");
  console.log("│  Source Diff (v1 → v2)                  │");
  console.log("└─────────────────────────────────────────┘");
  console.log("");

  // Run diff for each pair of files
  for (let i = 0; i < v1Files.length; i++) {
    const v1File = v1Files[i];
    const v2File = v2Files[i];

    try {
      // diff returns exit code 1 when files differ, so we need to handle that
      const output = execSync(
        `diff -u "${v1File.path}" "${v2File.path}" || true`,
        { encoding: "utf-8", maxBuffer: 1024 * 1024 }
      );

      if (output.trim()) {
        // Replace the full paths with version labels in the header
        const labeled = output
          .replace(
            new RegExp(v1File.path.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "g"),
            `v1/${v1File.relative}`
          )
          .replace(
            new RegExp(v2File.path.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "g"),
            `v2/${v2File.relative}`
          );
        console.log(labeled);
      } else {
        console.log(`No differences in ${v1File.relative}`);
      }
    } catch (err) {
      console.log(`Could not diff ${v1File.relative}: ${err}`);
    }
  }

  console.log("");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
