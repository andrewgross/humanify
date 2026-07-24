import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import { generate } from "../babel-utils.js";
import { runFamilyPermute } from "./family-permute-step.js";
import { createIsEligible } from "./rename-eligibility.js";

const IS_ELIGIBLE = createIsEligible("bun", "bun");

/** Normalize a fixture through the generator so apply-mode assertions
 * can demand byte-equality with the prior text (both legs are
 * babel-generator output in production). Statements are TOP-LEVEL
 * (program body) — that is the axis this pass groups on. */
function canon(code: string): string {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  assert.ok(ast);
  return generate(ast as never, { compact: false }).code;
}

function run(priorRaw: string, freshRaw: string) {
  const prior = canon(priorRaw);
  const fresh = canon(freshRaw);
  const outcome = runFamilyPermute(fresh, prior, IS_ELIGIBLE, {
    compact: false
  });
  return { outcome, prior, fresh };
}

describe("runFamilyPermute (exp036 idea 8b post-render pass)", () => {
  it("reassigns a fresh-minted family member to the free prior name", () => {
    // Two same-hash members `var X = createStore();`. The fresh render
    // gave the second a mint `q7x`; the free prior name is secondStore.
    // The echo in the log line must follow the rename.
    const prior = `
      var firstStore = createStore();
      var secondStore = createStore();
      log(firstStore, secondStore);
    `;
    const fresh = `
      var firstStore = createStore();
      var q7x = createStore();
      log(firstStore, q7x);
    `;
    const { outcome, prior: p } = run(prior, fresh);
    assert.ok(outcome?.code, "pass applied and produced code");
    assert.strictEqual(outcome.applied, 1);
    assert.strictEqual(outcome.code, p, "q7x -> secondStore, byte-clean");
  });

  it("reassigns several minted members in one bucket (batch permutation)", () => {
    // Three same-hash members; two drew mints. Both adopt their free
    // prior names through the temp-name applier without collision, and
    // the echo line follows.
    const prior = `
      var storeAlpha = init();
      var storeBeta = init();
      var storeGamma = init();
      use(storeAlpha, storeBeta, storeGamma);
    `;
    const fresh = `
      var storeAlpha = init();
      var q7 = init();
      var k3 = init();
      use(storeAlpha, q7, k3);
    `;
    const { outcome, prior: p } = run(prior, fresh);
    assert.ok(outcome?.code);
    assert.strictEqual(outcome.applied, 2, "q7->storeBeta, k3->storeGamma");
    assert.strictEqual(outcome.code, p, "byte-clean including the echo");
  });

  it("refuses when a bucket member differs in a PROPERTY (real change)", () => {
    // Same statement hash (property masked) but obj.foo vs obj.bar is a
    // genuine change — the owner gate voids the bucket, nothing wrong
    // is applied.
    const prior = `
      var a = obj.foo();
      var b = obj.foo();
      use(a, b);
    `;
    const fresh = `
      var a = obj.bar();
      var q7x = obj.foo();
      use(a, q7x);
    `;
    const { outcome } = run(prior, fresh);
    assert.ok(
      !outcome?.code || outcome.applied === 0,
      "never applies a rename across a real property change"
    );
  });

  it("no-op when the bucket is already clean", () => {
    const same = `
      var a = f();
      var b = f();
      use(a, b);
    `;
    const { outcome } = run(same, same);
    assert.ok(!outcome?.code, "nothing to do");
    assert.strictEqual(outcome?.applied ?? 0, 0);
  });

  it("leaves unequal-count buckets alone (membership churn)", () => {
    const prior = `
      var a = g();
      var b = g();
      use(a, b);
    `;
    const fresh = `
      var x = g();
      var y = g();
      var z = g();
      use(x, y, z);
    `;
    const { outcome } = run(prior, fresh);
    assert.strictEqual(outcome?.applied ?? 0, 0);
  });
});
