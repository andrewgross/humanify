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
import { reportResults } from "./reporter.js";

const FIXTURES_DIR = join(import.meta.dirname, "..", "fixtures");

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

async function handleValidate(args: string[]): Promise<void> {
  const pkg = args[0];
  if (!pkg) {
    console.error("Usage: e2e validate <fixture> [v1] [v2]");
    process.exit(1);
  }

  const config = loadFixtureConfig(pkg);
  const minifierConfig = DEFAULT_MINIFIER_CONFIG;

  // Determine which version pairs to run
  let pairs = config.versionPairs;
  if (args[1] && args[2]) {
    pairs = [{ v1: args[1], v2: args[2] }];
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

    // Step 7: Report
    reportResults(result);

    if (result.failures.length > 0) {
      allPassed = false;
    }
  }

  if (!allPassed) {
    process.exit(1);
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
