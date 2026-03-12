import { join, basename } from "node:path";
import { loadFixtureConfig, getBuildDir } from "./setup.js";
import { minifyFixtureVersion, getMinifierConfig } from "./minify.js";
import { buildGroundTruth } from "./ground-truth.js";
import {
  buildFingerprintData,
  linkMinifiedToSource,
  validate,
  type ValidationResult
} from "./validate.js";
import { matchFunctions } from "../../../src/analysis/fingerprint-index.js";

/**
 * Run the full validation pipeline for a single (fixture, v1, v2, minifier) combination.
 *
 * Returns the ValidationResult without any side effects (no console output,
 * no snapshots, no debug artifacts, no process.exit).
 */
export async function runValidation(
  pkg: string,
  v1: string,
  v2: string,
  minifierConfigId: string
): Promise<ValidationResult> {
  const config = loadFixtureConfig(pkg);

  // Find the matching version pair for overrides (e.g. expectMatchDespiteModification)
  const pair = config.versionPairs.find((p) => p.v1 === v1 && p.v2 === v2);

  const minifierConfig = getMinifierConfig(minifierConfigId);
  if (!minifierConfig) {
    throw new Error(`Unknown minifier config: ${minifierConfigId}`);
  }

  // Step 1: Minify both versions
  const v1MinResults = await minifyFixtureVersion(
    pkg,
    v1,
    config,
    minifierConfig
  );
  const v2MinResults = await minifyFixtureVersion(
    pkg,
    v2,
    config,
    minifierConfig
  );

  if (v1MinResults.length === 0 || v2MinResults.length === 0) {
    throw new Error("Minification produced no results");
  }

  const v1Min = v1MinResults[0];
  const v2Min = v2MinResults[0];

  // Step 2: Extract ground truth from compiled JS
  const v1BuildDir = getBuildDir(pkg, v1);
  const v2BuildDir = getBuildDir(pkg, v2);

  const v1SourceFiles = config.entryPoints.map((e) => {
    const jsEntry = basename(e).replace(/\.ts$/, ".js");
    return {
      path: join(v1BuildDir, "build", jsEntry),
      relative: jsEntry
    };
  });
  const v2SourceFiles = config.entryPoints.map((e) => {
    const jsEntry = basename(e).replace(/\.ts$/, ".js");
    return {
      path: join(v2BuildDir, "build", jsEntry),
      relative: jsEntry
    };
  });

  const groundTruth = buildGroundTruth(v1SourceFiles, v2SourceFiles);

  // Step 3: Build fingerprint indexes from minified code
  const v1Data = buildFingerprintData(v1Min.code, v1Min.minifiedPath);
  const v2Data = buildFingerprintData(v2Min.code, v2Min.minifiedPath);

  // Step 4: Match functions across versions
  const matchResult = matchFunctions(v1Data.index, v2Data.index);

  // Step 5: Link minified functions to source via source maps
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

  // Step 6: Validate
  const result = validate(
    pkg,
    v1,
    v2,
    minifierConfig.id,
    groundTruth,
    v1Data.index,
    v2Data.index,
    matchResult,
    v1LinkResult,
    v2LinkResult,
    pair?.expectMatchDespiteModification
  );

  return result;
}
