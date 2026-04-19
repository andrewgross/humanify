import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { loadFixtureConfig } from "./harness/setup.js";
import { minifyFixtureVersion, MINIFIER_CONFIGS } from "./harness/minify.js";
import { buildFingerprintData } from "./harness/validate.js";
import {
  matchFunctions,
  getMatchStats
} from "../../src/analysis/fingerprint-index.js";

const config = loadFixtureConfig("disambiguation");
const pair = config.versionPairs[0];

describe("disambiguation: direct matching", () => {
  for (const minifier of MINIFIER_CONFIGS) {
    it(`${pair.v1} → ${pair.v2} (${minifier.id})`, async () => {
      // Minify both versions
      const [v1Result] = await minifyFixtureVersion(
        "disambiguation",
        pair.v1,
        config,
        minifier
      );
      const [v2Result] = await minifyFixtureVersion(
        "disambiguation",
        pair.v2,
        config,
        minifier
      );

      // Build fingerprint indices
      const v1Data = buildFingerprintData(v1Result.code, v1Result.minifiedPath);
      const v2Data = buildFingerprintData(v2Result.code, v2Result.minifiedPath);

      // Match functions across versions
      const result = matchFunctions(v1Data.index, v2Data.index);
      const stats = getMatchStats(result);

      // Sanity: all v1 functions are accounted for
      assert.equal(
        stats.total,
        v1Data.index.fingerprints.size,
        "all v1 functions should be accounted for in match result"
      );

      // The perturbation changes updateFromInput's structure, so it (and
      // possibly its parent createStore) won't match. Allow 1-2 unmatched.
      assert.ok(
        result.unmatched.length >= 1 && result.unmatched.length <= 2,
        `expected 1-2 unmatched functions (the perturbed one + possibly parent), got ${result.unmatched.length}`
      );

      if (minifier.tool === "swc") {
        // SWC inlines trivial functions, eliminating call edges — but
        // memberKey disambiguation resolves them via property keys.
        assert.ok(
          result.resolutionStats.memberKeyResolved > 0,
          "SWC: memberKey should resolve functions that lost call edges to inlining"
        );
        assert.equal(
          result.ambiguous.size,
          0,
          `SWC: expected no ambiguous matches (memberKey resolves inlined twins), got ${result.ambiguous.size}`
        );
      } else {
        // terser, esbuild, bun: preserve call sites, callerShapes should fire
        assert.ok(
          result.resolutionStats.callerShapesResolved > 0,
          `${minifier.id}: expected callerShapes resolutions > 0, got ${result.resolutionStats.callerShapesResolved}`
        );
        assert.equal(
          result.ambiguous.size,
          0,
          `${minifier.id}: expected no ambiguous matches, got ${result.ambiguous.size}`
        );
      }

      const rs = result.resolutionStats;
      console.log(
        `  ${minifier.id}: matched=${stats.matched} ambiguous=${stats.ambiguous} unmatched=${stats.unmatched} ` +
          `exactHash=${rs.exactHashUnique} memberKey=${rs.memberKeyResolved} calleeShapes=${rs.calleeShapesResolved} ` +
          `callerShapes=${rs.callerShapesResolved} calleeHashes=${rs.calleeHashesResolved}`
      );
    });
  }
});
