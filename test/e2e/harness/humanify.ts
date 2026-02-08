/**
 * E2E humanify command: runs minified fixtures through the actual LLM rename
 * pipeline and validates output quality.
 *
 * Environment variables:
 *   HUMANIFY_TEST_BASE_URL   - OpenAI-compatible endpoint (e.g. http://localhost:8080/v1)
 *   HUMANIFY_TEST_MODEL      - Model identifier (e.g. qwen2.5-coder-32b)
 *   HUMANIFY_TEST_API_KEY    - API key (e.g. "dummy" for local servers)
 */

import { existsSync, readFileSync, writeFileSync, mkdirSync } from "fs";
import { join, basename } from "path";
import { createHash } from "crypto";
import { parseSync } from "@babel/core";
import * as babelTraverse from "@babel/traverse";
import {
  loadFixtureConfig,
  getBuildDir,
  type FixtureConfig,
} from "./setup.js";
import {
  minifyFixtureVersion,
  DEFAULT_MINIFIER_CONFIG,
  MINIFIER_CONFIGS,
  getMinifierConfig,
  type MinifierConfig,
} from "./minify.js";
import { buildGroundTruth, type GroundTruth } from "./ground-truth.js";
import { OpenAICompatibleProvider } from "../../../src/llm/openai-compatible.js";
import { createRenamePlugin } from "../../../src/plugins/rename.js";
import type { LLMProvider } from "../../../src/llm/types.js";

const traverse: typeof babelTraverse.default =
  typeof babelTraverse.default === "function"
    ? babelTraverse.default
    : (babelTraverse.default as any).default;

// ─── Types ───────────────────────────────────────────────────────────────────

interface LLMConfig {
  available: boolean;
  baseUrl: string;
  model: string;
  apiKey: string;
}

interface HumanifyOptions {
  updateSnapshot: boolean;
  ci: boolean;
  verbose: boolean;
  minifier: string | undefined;
  allMinifiers: boolean;
}

interface HumanifyResult {
  fixture: string;
  version: string;
  minifierConfig: string;
  timestamp: string;
  inputFunctions: number;
  outputFunctions: number;
  identifiersRenamed: number;
  avgNameLength: number;
  durationMs: number;
  syntaxValid: boolean;
  structurePreserved: boolean;
  nameRecoveryScore: number | null;
  outputHash: string;
}

// ─── Environment Check ──────────────────────────────────────────────────────

function checkLLMConfig(): LLMConfig {
  const baseUrl = process.env.HUMANIFY_TEST_BASE_URL || "";
  const model = process.env.HUMANIFY_TEST_MODEL || "";
  const apiKey = process.env.HUMANIFY_TEST_API_KEY || "";

  return {
    available: Boolean(baseUrl && model),
    baseUrl,
    model,
    apiKey: apiKey || "dummy",
  };
}

function createTestProvider(config: LLMConfig): LLMProvider {
  return new OpenAICompatibleProvider({
    endpoint: config.baseUrl,
    apiKey: config.apiKey,
    model: config.model,
    timeout: 120000,
  });
}

// ─── Pipeline ───────────────────────────────────────────────────────────────

async function humanifyFile(
  minifiedCode: string,
  provider: LLMProvider
): Promise<{ output: string; durationMs: number }> {
  const rename = createRenamePlugin({
    provider,
    concurrency: 10,
    onProgress: (msg) => process.stdout.write(`\r  ${msg}`),
  });

  const startTime = Date.now();
  const output = await rename(minifiedCode);
  const durationMs = Date.now() - startTime;

  // Clear progress line
  process.stdout.write("\r" + " ".repeat(80) + "\r");

  return { output, durationMs };
}

// ─── Validation ─────────────────────────────────────────────────────────────

function countFunctions(code: string): number {
  try {
    const ast = parseSync(code, { sourceType: "unambiguous" });
    if (!ast) return 0;

    let count = 0;
    traverse(ast, {
      Function() {
        count++;
      },
    });
    return count;
  } catch {
    return -1;
  }
}

function validateOutput(
  input: string,
  output: string
): {
  syntaxValid: boolean;
  inputFunctions: number;
  outputFunctions: number;
  structurePreserved: boolean;
} {
  const inputFunctions = countFunctions(input);

  // Check syntax validity
  let syntaxValid = true;
  let outputFunctions = 0;
  try {
    const ast = parseSync(output, { sourceType: "unambiguous" });
    if (!ast) {
      syntaxValid = false;
    } else {
      traverse(ast, {
        Function() {
          outputFunctions++;
        },
      });
    }
  } catch {
    syntaxValid = false;
  }

  return {
    syntaxValid,
    inputFunctions,
    outputFunctions,
    structurePreserved: syntaxValid && inputFunctions === outputFunctions,
  };
}

// ─── Metrics ────────────────────────────────────────────────────────────────

function collectIdentifiers(code: string): string[] {
  try {
    const ast = parseSync(code, { sourceType: "unambiguous" });
    if (!ast) return [];

    const ids = new Set<string>();
    traverse(ast, {
      Identifier(path: any) {
        ids.add(path.node.name);
      },
    });
    return Array.from(ids);
  } catch {
    return [];
  }
}

function calculateMetrics(
  input: string,
  output: string
): { identifiersRenamed: number; avgNameLength: number } {
  const inputIds = new Set(collectIdentifiers(input));
  const outputIds = collectIdentifiers(output);

  // Identifiers in output that weren't in input = newly introduced names
  const newIds = outputIds.filter((id) => !inputIds.has(id));
  const identifiersRenamed = newIds.length;

  const avgNameLength =
    newIds.length > 0
      ? newIds.reduce((sum, id) => sum + id.length, 0) / newIds.length
      : 0;

  return { identifiersRenamed, avgNameLength: Math.round(avgNameLength * 10) / 10 };
}

// ─── Name Recovery ──────────────────────────────────────────────────────────

function normalizeForComparison(name: string): string {
  // Convert camelCase/snake_case to lowercase tokens
  return name
    .replace(/([a-z])([A-Z])/g, "$1_$2")
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "");
}

function fuzzyNameMatch(candidate: string, reference: string): number {
  const normCandidate = normalizeForComparison(candidate);
  const normReference = normalizeForComparison(reference);

  if (normCandidate === normReference) return 1.0;
  if (normCandidate.includes(normReference) || normReference.includes(normCandidate)) return 0.7;

  // Check if they share significant substrings
  const minLen = Math.min(normCandidate.length, normReference.length);
  if (minLen < 3) return 0;

  let commonLen = 0;
  for (let len = minLen; len >= 3; len--) {
    for (let i = 0; i <= normReference.length - len; i++) {
      const sub = normReference.substring(i, i + len);
      if (normCandidate.includes(sub)) {
        commonLen = len;
        break;
      }
    }
    if (commonLen > 0) break;
  }

  return commonLen / Math.max(normCandidate.length, normReference.length);
}

function evaluateNameRecovery(
  output: string,
  groundTruth: GroundTruth
): number {
  const outputIds = new Set(collectIdentifiers(output));
  const sourceNames = [
    ...groundTruth.v1Functions.map((f) => f.name),
  ];

  if (sourceNames.length === 0) return 0;

  let totalScore = 0;
  let matchedCount = 0;

  for (const sourceName of sourceNames) {
    let bestScore = 0;
    for (const outputId of outputIds) {
      const score = fuzzyNameMatch(outputId, sourceName);
      if (score > bestScore) bestScore = score;
    }
    if (bestScore > 0.3) {
      totalScore += bestScore;
      matchedCount++;
    }
  }

  return sourceNames.length > 0
    ? Math.round((totalScore / sourceNames.length) * 100) / 100
    : 0;
}

// ─── Snapshots ──────────────────────────────────────────────────────────────

function getHumanifySnapshotDir(fixture: string): string {
  return join(import.meta.dirname, "..", "snapshots", "humanify", fixture);
}

function getHumanifySnapshotPath(
  fixture: string,
  version: string,
  minifierConfig: string
): string {
  return join(
    getHumanifySnapshotDir(fixture),
    `${version}-${minifierConfig}.snapshot.json`
  );
}

function saveHumanifySnapshot(result: HumanifyResult): string {
  const snapshotPath = getHumanifySnapshotPath(
    result.fixture,
    result.version,
    result.minifierConfig
  );
  mkdirSync(join(snapshotPath, ".."), { recursive: true });
  writeFileSync(snapshotPath, JSON.stringify(result, null, 2));
  return snapshotPath;
}

function compareHumanifySnapshot(result: HumanifyResult): {
  passed: boolean;
  diffs: string[];
  snapshotPath: string;
} {
  const snapshotPath = getHumanifySnapshotPath(
    result.fixture,
    result.version,
    result.minifierConfig
  );

  if (!existsSync(snapshotPath)) {
    return { passed: false, diffs: ["No snapshot found. Run with --update-snapshot first."], snapshotPath };
  }

  const baseline: HumanifyResult = JSON.parse(readFileSync(snapshotPath, "utf-8"));
  const diffs: string[] = [];

  // Must-pass: structural checks should not regress
  if (baseline.syntaxValid && !result.syntaxValid) {
    diffs.push("REGRESSION: output syntax was valid, now invalid");
  }
  if (baseline.structurePreserved && !result.structurePreserved) {
    diffs.push("REGRESSION: structure was preserved, now broken");
  }

  // Function count should remain stable
  if (result.outputFunctions !== baseline.outputFunctions) {
    diffs.push(
      `Function count changed: ${baseline.outputFunctions} → ${result.outputFunctions}`
    );
  }

  // Identifiers renamed should not drop significantly (>50% regression)
  if (
    baseline.identifiersRenamed > 0 &&
    result.identifiersRenamed < baseline.identifiersRenamed * 0.5
  ) {
    diffs.push(
      `Identifiers renamed regressed: ${baseline.identifiersRenamed} → ${result.identifiersRenamed}`
    );
  }

  // Average name length should not drop significantly
  if (
    baseline.avgNameLength > 0 &&
    result.avgNameLength < baseline.avgNameLength * 0.5
  ) {
    diffs.push(
      `Avg name length regressed: ${baseline.avgNameLength} → ${result.avgNameLength}`
    );
  }

  // Name recovery score should not drop significantly
  if (
    baseline.nameRecoveryScore !== null &&
    result.nameRecoveryScore !== null &&
    baseline.nameRecoveryScore > 0 &&
    result.nameRecoveryScore < baseline.nameRecoveryScore * 0.7
  ) {
    diffs.push(
      `Name recovery score regressed: ${baseline.nameRecoveryScore} → ${result.nameRecoveryScore}`
    );
  }

  return { passed: diffs.length === 0, diffs, snapshotPath };
}

// ─── Reporting ──────────────────────────────────────────────────────────────

function reportResult(result: HumanifyResult, verbose: boolean): void {
  console.log("");
  console.log("┌─────────────────────────────────────────┐");
  console.log(`│  Humanify: ${result.fixture} v${result.version} (${result.minifierConfig})`);
  console.log("└─────────────────────────────────────────┘");
  console.log("");

  // Must-pass checks
  const syntaxIcon = result.syntaxValid ? "\x1b[32m✓\x1b[0m" : "\x1b[31m✗\x1b[0m";
  const structIcon = result.structurePreserved ? "\x1b[32m✓\x1b[0m" : "\x1b[31m✗\x1b[0m";
  console.log(`  ${syntaxIcon} Syntax valid`);
  console.log(`  ${structIcon} Structure preserved (${result.inputFunctions} → ${result.outputFunctions} functions)`);
  console.log("");

  // Metrics
  console.log("  Metrics:");
  console.log(`    Identifiers renamed:  ${result.identifiersRenamed}`);
  console.log(`    Avg name length:      ${result.avgNameLength}`);
  console.log(`    Duration:             ${formatDuration(result.durationMs)}`);
  if (result.nameRecoveryScore !== null) {
    const pct = Math.round(result.nameRecoveryScore * 100);
    console.log(`    Name recovery score:  ${pct}%`);
  }
  console.log(`    Output hash:          ${result.outputHash.substring(0, 12)}...`);
  console.log("");
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60000);
  const secs = Math.round((ms % 60000) / 1000);
  return `${mins}m ${secs}s`;
}

// ─── Main Handler ───────────────────────────────────────────────────────────

function parseHumanifyOptions(args: string[]): {
  positional: string[];
  options: HumanifyOptions;
} {
  const positional: string[] = [];
  const options: HumanifyOptions = {
    updateSnapshot: false,
    ci: false,
    verbose: false,
    minifier: undefined,
    allMinifiers: false,
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === "--update-snapshot") {
      options.updateSnapshot = true;
    } else if (arg === "--ci") {
      options.ci = true;
    } else if (arg === "--verbose") {
      options.verbose = true;
    } else if (arg === "--minifier" && args[i + 1]) {
      options.minifier = args[++i];
    } else if (arg === "--all-minifiers") {
      options.allMinifiers = true;
    } else if (!arg.startsWith("--")) {
      positional.push(arg);
    }
  }

  return { positional, options };
}

export async function handleHumanify(args: string[]): Promise<void> {
  const { positional, options } = parseHumanifyOptions(args);
  const pkg = positional[0];
  const requestedVersion = positional[1];

  if (!pkg) {
    console.error(
      "Usage: e2e humanify <fixture> [version] [--update-snapshot] [--ci] [--verbose] [--minifier <id>] [--all-minifiers]"
    );
    process.exit(1);
  }

  // Check LLM availability
  const llmConfig = checkLLMConfig();
  if (!llmConfig.available) {
    console.error("LLM not configured. Set the following environment variables:");
    console.error("  HUMANIFY_TEST_BASE_URL  (e.g. http://localhost:8080/v1)");
    console.error("  HUMANIFY_TEST_MODEL     (e.g. qwen2.5-coder-32b)");
    console.error("  HUMANIFY_TEST_API_KEY   (optional, defaults to 'dummy')");
    process.exit(1);
  }

  console.log(`LLM: ${llmConfig.model} @ ${llmConfig.baseUrl}`);

  const config = loadFixtureConfig(pkg);
  const provider = createTestProvider(llmConfig);

  // Determine minifier configs
  let minifierConfigs: MinifierConfig[];
  if (options.allMinifiers) {
    minifierConfigs = MINIFIER_CONFIGS;
  } else if (options.minifier) {
    const mc = getMinifierConfig(options.minifier);
    if (!mc) {
      console.error(`Unknown minifier: ${options.minifier}`);
      console.error("Available: " + MINIFIER_CONFIGS.map((c) => c.id).join(", "));
      process.exit(1);
    }
    minifierConfigs = [mc];
  } else {
    minifierConfigs = [DEFAULT_MINIFIER_CONFIG];
  }

  // Collect all unique versions from version pairs
  const allVersions = new Set<string>();
  for (const pair of config.versionPairs) {
    allVersions.add(pair.v1);
    allVersions.add(pair.v2);
  }

  // Determine which versions to run
  let versions: string[];
  if (requestedVersion) {
    if (!allVersions.has(requestedVersion)) {
      console.error(`Version ${requestedVersion} not found in fixture config.`);
      console.error("Available: " + Array.from(allVersions).join(", "));
      process.exit(1);
    }
    versions = [requestedVersion];
  } else {
    // Default to the first v1 version (smallest/simplest)
    versions = [config.versionPairs[0].v1];
  }

  let allPassed = true;

  for (const version of versions) {
    for (const minifierConfig of minifierConfigs) {
      console.log(`\nHumanifying ${pkg} v${version} (${minifierConfig.id})...`);

      // Step 1: Minify the version
      const minResults = await minifyFixtureVersion(
        pkg,
        version,
        config,
        minifierConfig
      );
      if (minResults.length === 0) {
        console.error("Minification produced no results");
        process.exit(1);
      }
      const minResult = minResults[0];
      console.log(`  Minified: ${minResult.code.length} bytes`);

      // Step 2: Build ground truth for name recovery scoring
      const buildDir = getBuildDir(pkg, version);
      const sourceFiles = config.entryPoints.map((e) => {
        const jsEntry = basename(e).replace(/\.ts$/, ".js");
        return {
          path: join(buildDir, "build", jsEntry),
          relative: jsEntry,
        };
      });

      let groundTruth: GroundTruth | null = null;
      try {
        groundTruth = buildGroundTruth(sourceFiles, sourceFiles);
      } catch {
        // Ground truth may fail if source not set up; that's OK
      }

      // Step 3: Run humanify pipeline
      let output: string;
      let durationMs: number;
      try {
        const pipelineResult = await humanifyFile(minResult.code, provider);
        output = pipelineResult.output;
        durationMs = pipelineResult.durationMs;
      } catch (err: any) {
        console.error(`  Pipeline failed: ${err.message}`);
        allPassed = false;
        continue;
      }

      // Step 4: Validate output
      const validation = validateOutput(minResult.code, output);

      // Step 5: Calculate metrics
      const metrics = calculateMetrics(minResult.code, output);

      // Step 6: Name recovery score
      let nameRecoveryScore: number | null = null;
      if (groundTruth) {
        nameRecoveryScore = evaluateNameRecovery(output, groundTruth);
      }

      // Step 7: Build result
      const outputHash = createHash("sha256").update(output).digest("hex");
      const result: HumanifyResult = {
        fixture: pkg,
        version,
        minifierConfig: minifierConfig.id,
        timestamp: new Date().toISOString(),
        inputFunctions: validation.inputFunctions,
        outputFunctions: validation.outputFunctions,
        identifiersRenamed: metrics.identifiersRenamed,
        avgNameLength: metrics.avgNameLength,
        durationMs,
        syntaxValid: validation.syntaxValid,
        structurePreserved: validation.structurePreserved,
        nameRecoveryScore,
        outputHash,
      };

      // Step 8: Report
      reportResult(result, options.verbose);

      if (options.verbose) {
        console.log("─── Renamed Output ───");
        console.log(output);
        console.log("──────────────────────");
        console.log("");
      }

      // Step 9: Snapshot handling
      if (options.ci) {
        const comparison = compareHumanifySnapshot(result);
        if (comparison.passed) {
          console.log(`  Snapshot: \x1b[32mPASS\x1b[0m`);
        } else {
          console.log(`  Snapshot: \x1b[31mFAIL\x1b[0m`);
          for (const diff of comparison.diffs) {
            console.log(`    - ${diff}`);
          }
          allPassed = false;
        }
      } else if (options.updateSnapshot) {
        const snapshotPath = saveHumanifySnapshot(result);
        console.log(`  Snapshot saved: ${snapshotPath}`);
      }

      // Must-pass checks
      if (!validation.syntaxValid) {
        console.error("  \x1b[31mFAIL: Output has syntax errors\x1b[0m");
        allPassed = false;
      }
      if (!validation.structurePreserved) {
        console.error("  \x1b[31mFAIL: Function count changed\x1b[0m");
        allPassed = false;
      }
    }
  }

  if (!allPassed) {
    process.exit(1);
  }
}
