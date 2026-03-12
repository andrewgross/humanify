import { describe, it } from "node:test";
import { runValidation } from "./harness/runner.js";
import { assertSnapshotMatch } from "./harness/test-helpers.js";
import { loadFixtureConfig } from "./harness/setup.js";
import { MINIFIER_CONFIGS } from "./harness/minify.js";

const config = loadFixtureConfig("mitt");

describe("mitt fingerprint validation", () => {
  for (const pair of config.versionPairs) {
    for (const minifier of MINIFIER_CONFIGS) {
      it(`${pair.v1} → ${pair.v2} (${minifier.id})`, async () => {
        const result = await runValidation(
          "mitt",
          pair.v1,
          pair.v2,
          minifier.id
        );
        assertSnapshotMatch(result);
      });
    }
  }
});
