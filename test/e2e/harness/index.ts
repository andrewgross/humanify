import { existsSync, readdirSync, readFileSync } from "fs";
import { join, basename } from "path";
import {
  setupFixture,
  loadFixtureConfig,
  getSourceDir,
  getBuildDir,
  getFixtureDir,
} from "./setup.js";
import {
  minifyFixtureVersion,
  DEFAULT_MINIFIER_CONFIG,
} from "./minify.js";
import { buildGroundTruth } from "./ground-truth.js";
import {
  buildFingerprintData,
  linkMinifiedToSource,
  validate,
} from "./validate.js";
import { matchFunctions } from "../../../src/analysis/fingerprint-index.js";
import { reportResults, reportResultsCI } from "./reporter.js";
import { generateDebugArtifacts, getOutputDir, type DebugContext } from "./debug.js";
import { saveSnapshot, compareToSnapshot, reportSnapshotComparison } from "./snapshot.js";
import { extractFunctionCode } from "./code-extractor.js";

const FIXTURES_DIR = join(import.meta.dirname, "..", "fixtures");

interface ValidateOptions {
  updateSnapshot: boolean;
  ci: boolean;
  verbose: boolean;
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
  console.log("      --ci                         CI mode: compare against snapshots, fail on drift");
  console.log("      --verbose                    Show detailed failure output");
  console.log("  e2e debug <fixture> <v1> <v2> --function <name>  Investigate specific function");
  console.log("  e2e list                          List available fixtures");
}

async function handleSetup(args: string[]): Promise<void> {
  const pkg = args[0];
  if (!pkg) {
    console.error("Usage: e2e setup <fixture>");
    process.exit(1);
  }
  await setupFixture(pkg);
}

function parseValidateOptions(args: string[]): { positional: string[]; options: ValidateOptions } {
  const positional: string[] = [];
  const options: ValidateOptions = {
    updateSnapshot: false,
    ci: false,
    verbose: false,
  };

  for (const arg of args) {
    if (arg === "--update-snapshot") {
      options.updateSnapshot = true;
    } else if (arg === "--ci") {
      options.ci = true;
    } else if (arg === "--verbose") {
      options.verbose = true;
    } else if (!arg.startsWith("--")) {
      positional.push(arg);
    }
  }

  return { positional, options };
}

async function handleValidate(args: string[]): Promise<void> {
  const { positional, options } = parseValidateOptions(args);
  const pkg = positional[0];

  if (!pkg) {
    console.error("Usage: e2e validate <fixture> [v1] [v2] [--update-snapshot] [--ci] [--verbose]");
    process.exit(1);
  }

  const config = loadFixtureConfig(pkg);
  const minifierConfig = DEFAULT_MINIFIER_CONFIG;

  // Determine which version pairs to run
  let pairs = config.versionPairs;
  if (positional[1] && positional[2]) {
    pairs = [{ v1: positional[1], v2: positional[2] }];
  }

  let allPassed = true;

  for (const pair of pairs) {
    console.log(`\nValidating ${pkg} ${pair.v1} → ${pair.v2}...`);

    // Step 1: Minify both versions
    const v1MinResults = await minifyFixtureVersion(pkg, pair.v1, config, minifierConfig);
    const v2MinResults = await minifyFixtureVersion(pkg, pair.v2, config, minifierConfig);

    if (v1MinResults.length === 0 || v2MinResults.length === 0) {
      console.error("Minification produced no results");
      process.exit(1);
    }

    const v1Min = v1MinResults[0];
    const v2Min = v2MinResults[0];

    // Step 2: Extract ground truth from compiled JS (not TypeScript source)
    // This ensures line numbers align with what the source map points back to.
    // The compiled JS is the input to minification, so the source map maps
    // minified positions → compiled JS positions.
    const v1BuildDir = getBuildDir(pkg, pair.v1);
    const v2BuildDir = getBuildDir(pkg, pair.v2);

    const v1SourceFiles = config.entryPoints.map((e) => {
      const jsEntry = basename(e).replace(/\.ts$/, ".js");
      return {
        path: join(v1BuildDir, "build", jsEntry),
        relative: jsEntry,
      };
    });
    const v2SourceFiles = config.entryPoints.map((e) => {
      const jsEntry = basename(e).replace(/\.ts$/, ".js");
      return {
        path: join(v2BuildDir, "build", jsEntry),
        relative: jsEntry,
      };
    });

    const groundTruth = buildGroundTruth(v1SourceFiles, v2SourceFiles);
    console.log(
      `Ground truth: ${groundTruth.v1Functions.length} v1 fns, ${groundTruth.v2Functions.length} v2 fns, ${groundTruth.correspondence.length} correspondences`
    );

    // Step 3: Build fingerprint indexes from minified code
    const v1Data = buildFingerprintData(v1Min.code, v1Min.minifiedPath);
    const v2Data = buildFingerprintData(v2Min.code, v2Min.minifiedPath);

    console.log(
      `Fingerprints: ${v1Data.index.fingerprints.size} v1, ${v2Data.index.fingerprints.size} v2`
    );

    // Step 4: Match functions across versions
    const matchResult = matchFunctions(v1Data.index, v2Data.index);
    console.log(
      `Matches: ${matchResult.matches.size} matched, ${matchResult.ambiguous.size} ambiguous, ${matchResult.unmatched.length} unmatched`
    );

    // Step 5: Link minified functions to source via source maps
    const v1Links = await linkMinifiedToSource(
      v1Data.functions,
      groundTruth.v1Functions,
      v1Min.sourceMap
    );
    const v2Links = await linkMinifiedToSource(
      v2Data.functions,
      groundTruth.v2Functions,
      v2Min.sourceMap
    );
    console.log(`Links: ${v1Links.size} v1, ${v2Links.size} v2`);

    // Step 6: Validate
    const result = validate(
      pkg,
      pair.v1,
      pair.v2,
      minifierConfig.id,
      groundTruth,
      v1Data.index,
      v2Data.index,
      matchResult,
      v1Links,
      v2Links
    );

    // Step 7: Extract code for debug context
    const v1SourceCode = extractFunctionCode(
      groundTruth.v1Functions,
      v1SourceFiles.map(f => f.path)
    );
    const v2SourceCode = extractFunctionCode(
      groundTruth.v2Functions,
      v2SourceFiles.map(f => f.path)
    );
    const v1MinifiedCode = extractMinifiedFunctionCode(v1Data.functions as any, v1Min.code);
    const v2MinifiedCode = extractMinifiedFunctionCode(v2Data.functions as any, v2Min.code);

    // Step 8: Generate debug artifacts
    const outputDir = getOutputDir(pkg, pair.v1, pair.v2, minifierConfig.id);
    const debugDir = join(outputDir, "debug");

    const debugContext: DebugContext = {
      fixture: pkg,
      v1: pair.v1,
      v2: pair.v2,
      minifierConfig: minifierConfig.id,
      groundTruth,
      v1Index: v1Data.index,
      v2Index: v2Data.index,
      matchResult,
      v1Links,
      v2Links,
      v1SourceCode,
      v2SourceCode,
      v1MinifiedCode,
      v2MinifiedCode,
      result,
    };

    generateDebugArtifacts(debugContext);

    // Step 9: Report results
    if (options.ci) {
      // CI mode: compare against snapshot
      const { passed, summary } = reportResultsCI(result);
      console.log(summary);

      const comparison = compareToSnapshot(result);
      const snapshotPassed = reportSnapshotComparison(comparison);

      if (!passed || !snapshotPassed) {
        allPassed = false;
      }
    } else {
      // Interactive mode: show detailed report
      reportResults(result, { debugDir, verbose: options.verbose });

      // Update snapshot if requested
      if (options.updateSnapshot) {
        const snapshotPath = saveSnapshot(result);
        console.log(`Snapshot updated: ${snapshotPath}`);
      }

      if (result.failures.length > 0) {
        allPassed = false;
      }
    }
  }

  if (!allPassed) {
    process.exit(1);
  }
}

async function handleDebug(args: string[]): Promise<void> {
  // Parse arguments
  const positional: string[] = [];
  let functionName: string | undefined;

  for (let i = 0; i < args.length; i++) {
    if (args[i] === "--function" && args[i + 1]) {
      functionName = args[++i];
    } else if (!args[i].startsWith("--")) {
      positional.push(args[i]);
    }
  }

  const [pkg, v1, v2] = positional;

  if (!pkg || !v1 || !v2 || !functionName) {
    console.error("Usage: e2e debug <fixture> <v1> <v2> --function <name>");
    process.exit(1);
  }

  const outputDir = getOutputDir(pkg, v1, v2, DEFAULT_MINIFIER_CONFIG.id);
  const debugDir = join(outputDir, "debug");

  // Check if debug artifacts exist
  if (!existsSync(debugDir)) {
    console.error(`Debug artifacts not found at ${debugDir}`);
    console.error("Run 'e2e validate' first to generate debug artifacts.");
    process.exit(1);
  }

  // Load ground truth
  const groundTruthPath = join(debugDir, "ground-truth.json");
  if (!existsSync(groundTruthPath)) {
    console.error("Ground truth file not found. Run 'e2e validate' first.");
    process.exit(1);
  }
  const groundTruth = JSON.parse(readFileSync(groundTruthPath, "utf-8"));

  // Find the function in ground truth
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

  // Print function details
  console.log("");
  console.log(`Function: ${functionName}`);
  console.log(`Change type: ${correspondence.changeType}`);
  console.log(`Source file: ${correspondence.sourceFile}`);
  console.log("");

  // Find in v1 and v2 functions
  const v1Fn = groundTruth.v1Functions.find(
    (f: { name: string }) => f.name === functionName
  );
  const v2Fn = groundTruth.v2Functions.find(
    (f: { name: string }) => f.name === functionName
  );

  if (v1Fn) {
    console.log("V1 Function:");
    console.log(`  Location: lines ${v1Fn.location.startLine}-${v1Fn.location.endLine}`);
    console.log(`  Arity: ${v1Fn.arity}`);
    console.log(`  Body hash: ${v1Fn.bodyHash}`);
    console.log("");
  }

  if (v2Fn) {
    console.log("V2 Function:");
    console.log(`  Location: lines ${v2Fn.location.startLine}-${v2Fn.location.endLine}`);
    console.log(`  Arity: ${v2Fn.arity}`);
    console.log(`  Body hash: ${v2Fn.bodyHash}`);
    console.log("");
  }

  // Check for failure artifacts
  const failuresDir = join(debugDir, "failures");
  if (existsSync(failuresDir)) {
    const failureDirs = readdirSync(failuresDir, { withFileTypes: true })
      .filter(d => d.isDirectory() && d.name.startsWith(`${functionName}-`))
      .map(d => d.name);

    if (failureDirs.length > 0) {
      console.log("Failure artifacts found:");
      for (const dir of failureDirs) {
        console.log(`  ${join(failuresDir, dir)}`);

        const summaryPath = join(failuresDir, dir, "summary.txt");
        if (existsSync(summaryPath)) {
          console.log("");
          console.log(readFileSync(summaryPath, "utf-8"));
        }
      }
    } else {
      console.log("No failures recorded for this function.");
    }
  }

  // Load and display fingerprints
  const v1FingerprintsPath = join(debugDir, "v1-fingerprints.json");
  const v2FingerprintsPath = join(debugDir, "v2-fingerprints.json");

  if (existsSync(v1FingerprintsPath) && existsSync(v2FingerprintsPath)) {
    const v1Fingerprints = JSON.parse(readFileSync(v1FingerprintsPath, "utf-8"));
    const v2Fingerprints = JSON.parse(readFileSync(v2FingerprintsPath, "utf-8"));

    console.log("");
    console.log("Fingerprint data stored in:");
    console.log(`  V1: ${v1FingerprintsPath}`);
    console.log(`  V2: ${v2FingerprintsPath}`);
  }
}

/**
 * Extract code snippets for minified functions.
 */
function extractMinifiedFunctionCode(
  functions: Map<string, { path: { node: { loc?: { start: { line: number; column: number }; end: { line: number; column: number } } | null } } }>,
  code: string
): Map<string, string> {
  const lines = code.split("\n");
  const result = new Map<string, string>();

  for (const [sessionId, fn] of functions) {
    const loc = fn.path.node.loc;
    if (!loc) continue;

    // For minified code, it's usually all on one line, so extract by column
    if (loc.start.line === loc.end.line) {
      const line = lines[loc.start.line - 1] || "";
      result.set(sessionId, line.slice(loc.start.column, loc.end.column));
    } else {
      // Multi-line: extract full range
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
      result.set(sessionId, extracted.join("\n"));
    }
  }

  return result;
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
    const configPath = join(FIXTURES_DIR, name, "fixture.config.json");
    if (existsSync(configPath)) {
      const config = JSON.parse(readFileSync(configPath, "utf-8"));
      const setupDone = existsSync(join(FIXTURES_DIR, name, "source"));
      const status = setupDone ? "ready" : "needs setup";
      console.log(`  ${name} (${status})`);
      for (const pair of config.versionPairs) {
        console.log(`    ${pair.v1} → ${pair.v2}${pair.description ? ` (${pair.description})` : ""}`);
      }
    } else {
      console.log(`  ${name} (no config)`);
    }
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
