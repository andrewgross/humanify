/**
 * Ablation study: measure the marginal value of each resolution level.
 *
 * For each fixture × version pair × minifier, runs matching at:
 *   R0 only:       exactHash match, no disambiguation
 *   R0 + R1:       add blurred callee shapes
 *   R0 + R1 + R2:  add exact callee hashes + two-hop (current default)
 *
 * Usage:
 *   npx tsx experiments/010-ablation/run.ts [fixture]
 */

import { basename, join } from "node:path";
import {
  loadFixtureConfig,
  getBuildDir
} from "../../test/e2e/harness/setup.js";
import {
  MINIFIER_CONFIGS,
  minifyFixtureVersion
} from "../../test/e2e/harness/minify.js";
import { buildGroundTruth } from "../../test/e2e/harness/ground-truth.js";
import {
  buildFingerprintData,
  linkMinifiedToSource,
  validate
} from "../../test/e2e/harness/validate.js";
import {
  matchFunctions,
  type MatchOptions
} from "../../src/analysis/fingerprint-index.js";
import type { ResolutionStats } from "../../src/analysis/types.js";

const FIXTURES = ["nanoid", "mitt", "zustand", "preact"];
const RESOLUTION_LEVELS = [0, 1, 2] as const;

interface AblationRow {
  fixture: string;
  v1: string;
  v2: string;
  minifier: string;
  maxResolution: 0 | 1 | 2;
  v1Fingerprints: number;
  v2Fingerprints: number;
  matched: number;
  ambiguous: number;
  unmatched: number;
  overallAccuracy: number;
  precision: number;
  recall: number;
  resolutionStats: ResolutionStats;
}

async function runAblationForPair(
  pkg: string,
  v1: string,
  v2: string,
  minifierId: string
): Promise<AblationRow[]> {
  const config = loadFixtureConfig(pkg);
  const pair = config.versionPairs.find((p) => p.v1 === v1 && p.v2 === v2);
  const minifierConfig = MINIFIER_CONFIGS.find((c) => c.id === minifierId)!;

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
  const v1Min = v1MinResults[0];
  const v2Min = v2MinResults[0];

  const v1BuildDir = getBuildDir(pkg, v1);
  const v2BuildDir = getBuildDir(pkg, v2);

  const v1SourceFiles = config.entryPoints.map((e) => {
    const jsEntry = basename(e).replace(/\.ts$/, ".js");
    return { path: join(v1BuildDir, "build", jsEntry), relative: jsEntry };
  });
  const v2SourceFiles = config.entryPoints.map((e) => {
    const jsEntry = basename(e).replace(/\.ts$/, ".js");
    return { path: join(v2BuildDir, "build", jsEntry), relative: jsEntry };
  });

  const groundTruth = buildGroundTruth(v1SourceFiles, v2SourceFiles);
  const v1Data = buildFingerprintData(v1Min.code, v1Min.minifiedPath);
  const v2Data = buildFingerprintData(v2Min.code, v2Min.minifiedPath);
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

  const rows: AblationRow[] = [];

  for (const maxRes of RESOLUTION_LEVELS) {
    const options: MatchOptions = { maxResolution: maxRes };
    const matchResult = matchFunctions(v1Data.index, v2Data.index, options);

    const result = validate(
      pkg,
      v1,
      v2,
      minifierId,
      groundTruth,
      v1Data.index,
      v2Data.index,
      matchResult,
      v1LinkResult,
      v2LinkResult,
      pair?.expectMatchDespiteModification
    );

    rows.push({
      fixture: pkg,
      v1,
      v2,
      minifier: minifierId,
      maxResolution: maxRes,
      v1Fingerprints: result.v1FingerprintCount,
      v2Fingerprints: result.v2FingerprintCount,
      matched: matchResult.matches.size,
      ambiguous: matchResult.ambiguous.size,
      unmatched: matchResult.unmatched.length,
      overallAccuracy: result.overallAccuracy,
      precision: result.precision,
      recall: result.recall,
      resolutionStats: matchResult.resolutionStats
    });
  }

  return rows;
}

function printTable(rows: AblationRow[]): void {
  // Group by fixture + pair
  const groups = new Map<string, AblationRow[]>();
  for (const row of rows) {
    const key = `${row.fixture} ${row.v1}→${row.v2} (${row.minifier})`;
    const group = groups.get(key) ?? [];
    group.push(row);
    groups.set(key, group);
  }

  const pct = (n: number) => `${(n * 100).toFixed(1)}%`.padEnd(8);
  const pad = (s: string | number, w: number) =>
    String(s).padEnd(w);

  console.log("");
  console.log("=".repeat(110));
  console.log("  ABLATION STUDY: Resolution Level Impact");
  console.log("=".repeat(110));

  for (const [key, group] of groups) {
    console.log(`\n  ${key}`);
    console.log(
      `  ${pad("MaxRes", 12)}${pad("Match", 8)}${pad("Ambig", 8)}${pad("Unmatch", 9)}${pad("Accuracy", 10)}${pad("Precision", 10)}${pad("Recall", 10)}${pad("exact", 8)}${pad("calleeS", 9)}${pad("calleeH", 9)}${pad("twoHop", 8)}`
    );
    console.log("  " + "-".repeat(105));

    for (const row of group) {
      const rs = row.resolutionStats;
      const label =
        row.maxResolution === 0
          ? "exact only"
          : row.maxResolution === 1
            ? "+shapes"
            : "+hashes";
      console.log(
        `  ${pad(label, 12)}${pad(row.matched, 8)}${pad(row.ambiguous, 8)}${pad(row.unmatched, 9)}${pct(row.overallAccuracy)}  ${pct(row.precision)}  ${pct(row.recall)}  ${pad(rs.exactHashUnique, 8)}${pad(rs.calleeShapesResolved, 9)}${pad(rs.calleeHashesResolved, 9)}${pad(rs.twoHopShapesResolved, 8)}`
      );
    }
  }

  console.log("\n" + "=".repeat(120));

  // Summary: did any resolution level change outcomes?
  let anyDiff = false;
  for (const [, group] of groups) {
    const r0 = group.find((r) => r.maxResolution === 0)!;
    const r2 = group.find((r) => r.maxResolution === 2)!;
    if (r0.overallAccuracy !== r2.overallAccuracy) {
      anyDiff = true;
    }
  }

  if (anyDiff) {
    console.log(
      "\n  Resolution cascade DOES affect outcomes on some fixtures."
    );
  } else {
    console.log(
      "\n  Resolution cascade has NO effect on current fixtures — R0 resolves everything."
    );
    console.log(
      "  Larger fixtures with more hash collisions are needed to exercise R1/R2."
    );
  }
}

async function main(): Promise<void> {
  const filterFixture = process.argv[2];
  const fixtures = filterFixture
    ? FIXTURES.filter((f) => f === filterFixture)
    : FIXTURES;

  if (fixtures.length === 0) {
    console.error(`Unknown fixture: ${filterFixture}`);
    console.error(`Available: ${FIXTURES.join(", ")}`);
    process.exit(1);
  }

  const allRows: AblationRow[] = [];

  for (const pkg of fixtures) {
    const config = loadFixtureConfig(pkg);

    for (const pair of config.versionPairs) {
      // Use terser-default only for ablation (minifier choice shouldn't affect resolution distribution)
      const rows = await runAblationForPair(
        pkg,
        pair.v1,
        pair.v2,
        "terser-default"
      );
      allRows.push(...rows);
    }
  }

  printTable(allRows);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
