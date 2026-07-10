import assert from "node:assert";
import { describe, it } from "node:test";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import { createIsEligible } from "./rename-eligibility.js";
import { runDeferredSweep } from "./sweep-step.js";

const IS_ELIGIBLE = createIsEligible(undefined, undefined);
const GEN_OPTS = { compact: false } as const;
const OPTS = { concurrency: 2, genOpts: GEN_OPTS };

function providerReturning(renames: Record<string, string>): LLMProvider {
  return {
    async suggestAllNames(_request: BatchRenameRequest) {
      return { renames };
    }
  };
}

describe("runDeferredSweep (pipeline step)", () => {
  it("names a minted survivor and returns the regenerated output", async () => {
    const code = `function processItem(H2) {\n  return H2 + 1;\n}`;
    const outcome = await runDeferredSweep(
      code,
      providerReturning({ H2: "itemCount" }),
      IS_ELIGIBLE,
      OPTS
    );
    assert.ok(outcome);
    assert.strictEqual(outcome.named, 1);
    assert.ok(outcome.code, "applied renames must ship a regenerated output");
    assert.match(outcome.code, /processItem\(itemCount\)/);
    assert.ok(outcome.ast, "the regenerated AST must be returned");
  });

  it("returns stats without code when the LLM declines every target", async () => {
    const code = `function processItem(H2) {\n  return H2 + 1;\n}`;
    const outcome = await runDeferredSweep(
      code,
      providerReturning({}),
      IS_ELIGIBLE,
      OPTS
    );
    assert.ok(outcome);
    assert.strictEqual(outcome.named, 0);
    assert.strictEqual(outcome.skipped, 1);
    assert.strictEqual(outcome.code, undefined);
  });

  it("contains an unparseable input instead of throwing (optional pass)", async () => {
    const outcome = await runDeferredSweep(
      "((((((( not javascript",
      providerReturning({}),
      IS_ELIGIBLE,
      OPTS
    );
    assert.strictEqual(outcome, undefined);
  });

  it("contains a provider crash as a skip, never a throw", async () => {
    const crashing: LLMProvider = {
      async suggestAllNames() {
        throw new Error("LLM box unreachable");
      }
    };
    const code = `function processItem(H2) {\n  return H2 + 1;\n}`;
    const outcome = await runDeferredSweep(code, crashing, IS_ELIGIBLE, OPTS);
    // sweepMintedNames contains per-group failures itself; the step must
    // surface them as skips with no replacement output.
    assert.ok(outcome);
    assert.strictEqual(outcome.named, 0);
    assert.strictEqual(outcome.code, undefined);
  });
});
