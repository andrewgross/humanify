import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import { generate } from "../babel-utils.js";
import { createIsEligible } from "./rename-eligibility.js";
import { runPriorDiffReconciliation } from "./reconcile-step.js";

const IS_ELIGIBLE = createIsEligible(undefined, undefined);
const GEN_OPTS = { compact: false } as const;

function canon(code: string): string {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  assert.ok(ast);
  return generate(ast, GEN_OPTS).code;
}

describe("runPriorDiffReconciliation (pipeline step)", () => {
  it("applies rename-noise renames and converges to the prior text", () => {
    const prior = canon(`
      function setup() {
        var completionState = loadState();
        console.log("step");
        return completionState;
      }
    `);
    const code = canon(`
      function setup() {
        var Tj_ = loadState();
        console.log("step");
        return Tj_;
      }
    `);
    const outcome = runPriorDiffReconciliation(
      code,
      prior,
      IS_ELIGIBLE,
      GEN_OPTS
    );
    assert.ok(outcome);
    assert.strictEqual(outcome.stats.renames, 1);
    assert.strictEqual(outcome.code, prior);
    assert.ok(outcome.ast, "reconciled AST must be returned for downstream");
  });

  it("reports zero stats and no code when there is nothing to reconcile", () => {
    const text = canon(`console.log("same");`);
    const outcome = runPriorDiffReconciliation(
      text,
      text,
      IS_ELIGIBLE,
      GEN_OPTS
    );
    assert.ok(outcome);
    assert.deepStrictEqual(outcome.stats, { renames: 0, skipped: 0 });
    assert.strictEqual(outcome.code, undefined);
  });

  it("contains internal failures instead of throwing (optional pass must not kill the run)", () => {
    // Unparseable "generated" code: the step must degrade to undefined —
    // upstream validation owns reporting it — never propagate a throw
    // that would abort a completed multi-hour run.
    const outcome = runPriorDiffReconciliation(
      "((((((( not javascript",
      canon(`console.log("prior");`),
      IS_ELIGIBLE,
      GEN_OPTS
    );
    assert.strictEqual(outcome, undefined);
  });

  it("reports the applied rename pairs for diagnostics", () => {
    const prior = canon(`
      function setup() {
        var completionState = loadState();
        return completionState;
      }
    `);
    const code = canon(`
      function setup() {
        var Tj_ = loadState();
        return Tj_;
      }
    `);
    const outcome = runPriorDiffReconciliation(
      code,
      prior,
      IS_ELIGIBLE,
      GEN_OPTS
    );
    assert.ok(outcome);
    assert.deepStrictEqual(
      outcome.renames.map((r) => ({ from: r.fromName, to: r.toName })),
      [{ from: "Tj_", to: "completionState" }]
    );
  });

  it("never touches genuine changes (arg-count case)", () => {
    const prior = canon(`
      log = pathLib.join(getTempDirectory(), "claude", name);
    `);
    const code = canon(`
      log = pathLib.join(getTempDirPath(), name);
    `);
    const outcome = runPriorDiffReconciliation(
      code,
      prior,
      IS_ELIGIBLE,
      GEN_OPTS
    );
    assert.ok(outcome);
    assert.strictEqual(outcome.stats.renames, 0);
    assert.strictEqual(outcome.code, undefined);
  });
});
