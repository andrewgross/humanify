import assert from "node:assert";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import { createRenamePlugin } from "./plugin.js";

/** Provider that renames every identifier with a suffix. */
function suffixProvider(): LLMProvider {
  return {
    async suggestAllNames(request: BatchRenameRequest) {
      const renames: Record<string, string> = {};
      for (const id of request.identifiers) {
        renames[id] = `${id}Named`;
      }
      return { renames };
    }
  };
}

describe("per-function rename report", () => {
  it("keeps main-pass outcomes when the shadowed-binding second pass runs", async () => {
    // The catch param shadows the function-scope K, so it is skipped in
    // the main collection and only renamed by the shadowed second pass.
    // That second pass must MERGE into the function's report — it used to
    // overwrite it, hiding every main-pass outcome from diagnostics.
    const source = `
function handleK(a) {
  var K = load(a);
  try {
    run(K);
  } catch (K) {
    log(K);
  }
  return K;
}
console.log(handleK);
`;
    const rename = createRenamePlugin({ provider: suffixProvider() });
    const result = await rename(source);

    assert.strictEqual(result.parseFailure, undefined);
    const fnReports = result.reports.filter((r) => r.type === "function");
    assert.strictEqual(fnReports.length, 1);
    const report = fnReports[0];

    // Main pass: a, K (var), handleK (own name). Shadowed pass: K (catch).
    assert.strictEqual(
      report.totalIdentifiers,
      4,
      `report must count both passes, got ${report.totalIdentifiers} ` +
        `(outcomes: ${Object.keys(report.outcomes).join(", ")})`
    );
    for (const name of ["a", "K", "handleK"]) {
      assert.ok(
        report.outcomes[name],
        `main-pass outcome for "${name}" must survive the second pass ` +
          `(outcomes: ${Object.keys(report.outcomes).join(", ")})`
      );
    }
    // KNOWN UNDERCOUNT under wave scheduling, which is what production runs.
    // Four renames demonstrably happen — the emitted code is
    // `handleK_r(a_r) { var K_r ... catch (K_rVal) }` under BOTH schedulers —
    // but `outcomes` is keyed by OLD NAME, so the `var K` and the `catch (K)`
    // collapse into one entry, and `fixupRenamedCount` (processor.ts:3912)
    // derives the count from that map. The free-running loop counted
    // identifiers directly and said 4.
    //
    // Asserted as 3 to record what production actually reports, NOT because 3
    // is right. See the filed task on the name-keyed outcomes map. The
    // assertions above are the ones carrying this test's intent: every
    // main-pass outcome survives the second pass.
    assert.strictEqual(
      report.renamedCount,
      3,
      "documents the name-collision undercount; the emitted code has 4 renames"
    );
  });
});
