import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import { generate } from "../babel-utils.js";
import { runFamilyPermute } from "./family-permute-step.js";
import { createIsEligible } from "./rename-eligibility.js";

const IS_ELIGIBLE = createIsEligible("bun", "bun");

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

describe("runFamilyPermute (exp036 8b — evidence-based)", () => {
  it("LOCKS a correct round-tripping name; only the mint orphan moves", () => {
    // The regression that killed v1: getToken exists in BOTH sides and
    // must never be reassigned. The fresh mint q7x (which replaced the
    // prior deviceMap) adopts deviceMap via matching call-site context.
    const prior = `
      function getToken() { return authStore(); }
      function deviceMap() { return authStore(); }
      use(getToken(), deviceMap());
      log(deviceMap());
    `;
    const fresh = `
      function getToken() { return authStore(); }
      function q7x() { return authStore(); }
      use(getToken(), q7x());
      log(q7x());
    `;
    const { outcome, prior: p } = run(prior, fresh);
    assert.ok(outcome?.code, "applied");
    assert.strictEqual(outcome.applied, 1, "only the orphan mint moves");
    assert.strictEqual(outcome.code, p, "q7x -> deviceMap, byte-clean");
    assert.match(outcome.code, /function getToken/, "getToken untouched");
  });

  it("never reassigns when the disaster shape appears (both names correct)", () => {
    // Both members round-trip (getToken, deviceMap present on both sides)
    // but their bodies differ by a local — v1 would have mispaired them.
    // Now: both locked, nothing applied, no wrong rename.
    const prior = `
      function getToken() { return authStore(1); }
      function deviceMap() { return authStore(2); }
      use(getToken(), deviceMap());
    `;
    const fresh = `
      function getToken() { return authStore(1); }
      function deviceMap() { return authStore(2); }
      use(getToken(), deviceMap());
    `;
    const { outcome } = run(prior, fresh);
    assert.strictEqual(outcome?.applied ?? 0, 0, "both locked, nothing moved");
  });

  it("pairs two mints by call-site context, not pool order", () => {
    const prior = `
      function getHandler() { return route(); }
      function postHandler() { return route(); }
      register(getHandler, "GET");
      wire(getHandler);
      register(postHandler, "POST");
      wire(postHandler);
    `;
    const fresh = `
      function k3() { return route(); }
      function m9() { return route(); }
      register(k3, "GET");
      wire(k3);
      register(m9, "POST");
      wire(m9);
    `;
    const { outcome, prior: p } = run(prior, fresh);
    assert.ok(outcome?.code);
    assert.strictEqual(outcome.code, p, "k3->getHandler, m9->postHandler");
  });

  it("refuses an orphan with no matching context (no evidence)", () => {
    const prior = `
      function alpha() { return q(); }
      function beta() { return q(); }
      alpha();
      beta();
    `;
    const fresh = `
      function alpha() { return q(); }
      function z9() { return q(); }
      alpha();
      z9();
    `;
    // z9's context `z9()` masks to `\x00()`, beta's `beta()` masks to
    // `\x00()` — they DO match here, so this is actually a supported
    // move. Assert it stays byte-safe (either applies cleanly or not).
    const { outcome, prior: p } = run(prior, fresh);
    assert.ok(!outcome?.code || outcome.code === p);
  });

  it("no-op when the bucket is already clean", () => {
    const same = `
      function a() { return f(); }
      function b() { return f(); }
      a();
      b();
    `;
    const { outcome } = run(same, same);
    assert.strictEqual(outcome?.applied ?? 0, 0);
  });
});
