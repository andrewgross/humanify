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
    // Four renames demonstrably happen — the emitted code is
    // `handleK_r(a_r) { var K_r ... catch (K_rVal) }`. The main pass names
    // a/K/handleK and the shadowed pass names the catch `K`, and BOTH are real
    // renames that must be counted.
    //
    // This asserted 3 while `outcomes` was keyed by bare old name: the two `K`
    // bindings collapsed into one entry, and fixupRenamedCount recomputed the
    // count from that collapsed map — undoing mergeRenameReports' correct sum.
    // coverage.ts counts the same map, so its per-status totals disagreed with
    // its own `total`.
    assert.strictEqual(
      report.renamedCount,
      4,
      "both K bindings are renamed; a name collision must not lose one"
    );
    assert.strictEqual(
      Object.keys(report.outcomes).length,
      4,
      "the shadowed K needs its own outcome entry, or every consumer that " +
        "counts outcomes undercounts by exactly the shadowed pass"
    );
  });
});
