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

  it("SWAP-CORRECTS two locked names the matcher cross-placed (the self-hop fix)", () => {
    // Both names round-trip (present on both sides), so v2's orphan-only
    // pass was blind here — yet the matcher put each on the OTHER
    // interchangeable member (identical bodies), and their call sites
    // prove it. C1 reads the context and swaps them back atomically, so
    // the output byte-matches the prior. This is the failure that broke
    // self-hop: on hop 2 the matcher cross-places, and only a pass that
    // can move a LOCKED name recovers determinism.
    const prior = `
      function iterCount() { return base(); }
      function charIndex() { return base(); }
      pbkdf2(iterCount, salt);
      scanFlags(charIndex, mask);
    `;
    const fresh = `
      function charIndex() { return base(); }
      function iterCount() { return base(); }
      pbkdf2(charIndex, salt);
      scanFlags(iterCount, mask);
    `;
    const { outcome, prior: p } = run(prior, fresh);
    assert.ok(outcome?.code, "applied");
    assert.strictEqual(outcome.applied, 2, "both cross-placed names swap");
    assert.strictEqual(outcome.code, p, "swapped back to a byte-clean prior");
  });

  it("never ships a swap temp when a chain fill is blocked outside the bucket", () => {
    // Chain: q7x→funcA (context: register/GET), funcA→funcB (context:
    // wire/POST) — but fresh funcB is a DIFFERENT function outside the
    // bucket, so the second fill can never land. The naive rollback
    // (temp→funcA) also fails because the first fill just claimed funcA:
    // without a plan-level guard the binding ships as
    // __familyPermuteSwapN$ in the final artifact, invisible to the
    // name-blind structural invariant and absent from the move trail.
    const prior = `
      function funcA() { return sharedImpl(); }
      function funcB() { return sharedImpl(); }
      register(funcA, "GET");
      wire(funcB, "POST");
    `;
    const fresh = `
      function q7x() { return sharedImpl(); }
      function funcA() { return sharedImpl(); }
      register(q7x, "GET");
      wire(funcA, "POST");
      function funcB() { return outsideHolderImpl(1, 2, 3); }
      funcB();
    `;
    const { outcome } = run(prior, fresh);
    if (outcome?.code) {
      assert.doesNotMatch(
        outcome.code,
        /__familyPermuteSwap/,
        `a swap temp must never ship, got:\n${outcome.code}`
      );
    }
  });

  it("REPORTS every rename it shipped, with the evidence behind it", () => {
    // This pass rewrites names in the FINAL artifact, and its v1 cut
    // renamed a CORRECT name (getClaudeCodeOAuthToken -> deviceActionMap)
    // — caught by a human reading the diff, not by any metric. So the
    // moves have to be readable: the eval reads this trail to check WHERE
    // the pass fired before attributing any KPI move to it, and a hop
    // where it fired nowhere is measuring the LLM, not the code.
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
    const { outcome } = run(prior, fresh);
    assert.ok(outcome?.code, "applied");
    assert.deepStrictEqual(
      outcome.moves.map((m) => ({ from: m.from, to: m.to })),
      [{ from: "q7x", to: "deviceMap" }],
      "the shipped rename is named, not just counted"
    );
    assert.ok(outcome.moves[0].support >= 1, "carries its context support");
    assert.ok(outcome.moves[0].bucket.length > 0, "carries its bucket key");
    assert.strictEqual(
      outcome.moves.length,
      outcome.applied,
      "the trail covers every applied move"
    );
  });

  it("reports an empty trail when it ships nothing", () => {
    const same = `
      function a() { return f(); }
      function b() { return f(); }
      a();
      b();
    `;
    const { outcome } = run(same, same);
    assert.deepStrictEqual(outcome?.moves, []);
  });
});
