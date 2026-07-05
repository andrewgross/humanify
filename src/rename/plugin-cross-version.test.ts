import assert from "node:assert";
import fs from "node:fs";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import { createRenamePlugin } from "./plugin.js";

/**
 * Cross-version transfer, end to end at the plugin level: humanify v1 of a
 * committed minified fixture, then humanify v2 with v1's output as the
 * prior version. This is the --prior-version workflow in miniature.
 */

const FIXTURES = new URL(
  "../../test/e2e/fixtures/disambiguation/minified/",
  import.meta.url
);

function readFixture(version: string, minifier: string): string {
  return fs.readFileSync(
    new URL(`${version}/${minifier}.js`, FIXTURES),
    "utf-8"
  );
}

/** Provider that counts calls and tags names with a run-specific suffix. */
function countingProvider(suffix: string): {
  provider: LLMProvider;
  calls: () => number;
} {
  let count = 0;
  const provider: LLMProvider = {
    async suggestAllNames(request: BatchRenameRequest) {
      count++;
      const renames: Record<string, string> = {};
      for (const id of request.identifiers) {
        renames[id] = `${id}${suffix}`;
      }
      return { renames };
    }
  };
  return { provider, calls: () => count };
}

describe("cross-version prior-version transfer (bun fixture pair)", () => {
  it("reuses v1 names in v2, reduces LLM calls, and produces parseable output", async () => {
    const v1Code = readFixture("v1.0.0", "bun-default");
    const v2Code = readFixture("v1.1.0", "bun-default");

    // Run A: fresh humanify of v1
    const runA = countingProvider("Renamed");
    const renameV1 = createRenamePlugin({ provider: runA.provider });
    const resultV1 = await renameV1(v1Code);

    assert.strictEqual(
      resultV1.parseFailure,
      undefined,
      `v1 output must parse: ${resultV1.parseFailure?.message}`
    );
    assert.ok(runA.calls() > 0, "fresh run should make LLM calls");

    // Run B: humanify v2 with v1's humanified output as prior version.
    // A different suffix means any "Renamed" identifier in v2's output
    // can only have come from the prior-version transfer.
    const runB = countingProvider("Fresh");
    const renameV2 = createRenamePlugin({
      provider: runB.provider,
      priorVersionCode: resultV1.code
    });
    const resultV2 = await renameV2(v2Code);

    assert.strictEqual(
      resultV2.parseFailure,
      undefined,
      `v2 output must parse: ${resultV2.parseFailure?.message}\n${resultV2.parseFailure?.excerpt ?? ""}`
    );

    const transferred = resultV2.code.match(/[A-Za-z0-9_$]*Renamed/g) ?? [];
    assert.ok(
      transferred.length > 0,
      `v2 output should reuse names invented in v1, got:\n${resultV2.code}`
    );
    assert.ok(
      runB.calls() < runA.calls(),
      `prior-version run should need fewer LLM calls (fresh=${runA.calls()}, cached=${runB.calls()})`
    );
  });

  it("is stable when v2 equals v1 (identical input reuses names wholesale)", async () => {
    const v1Code = readFixture("v1.0.0", "bun-default");

    const runA = countingProvider("Renamed");
    const renameV1 = createRenamePlugin({ provider: runA.provider });
    const resultV1 = await renameV1(v1Code);

    const runB = countingProvider("Fresh");
    const renameAgain = createRenamePlugin({
      provider: runB.provider,
      priorVersionCode: resultV1.code
    });
    const resultV2 = await renameAgain(v1Code);

    assert.strictEqual(resultV2.parseFailure, undefined);
    assert.ok(
      !resultV2.code.includes("Fresh"),
      `identical input should transfer every name without fresh LLM naming, got fresh names in:\n${resultV2.code}`
    );
  });
});
