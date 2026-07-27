import assert from "node:assert";
import { describe, it } from "node:test";
import { assignBucket } from "./family-permute.js";

describe("assignBucket (context-strict swap-correction, exp036 8b)", () => {
  const M = (name: string, ...contexts: string[]) => ({ name, contexts });

  it("LOCKS a correctly-placed name present on both sides", () => {
    const fresh = [
      M("getClaudeCodeOAuthToken", "let t = \x00();", "\x00().accessToken"),
      M("q7x", "let d = \x00();", "\x00().deviceMap")
    ];
    const prior = [
      M("getClaudeCodeOAuthToken", "let t = \x00();", "\x00().accessToken"),
      M("deviceActionMap", "let d = \x00();", "\x00().deviceMap")
    ];
    const out = assignBucket(fresh, prior);
    assert.strictEqual(out.length, 1, "only the orphan moves");
    assert.deepStrictEqual(
      { from: out[0].fromName, to: out[0].toName },
      { from: "q7x", to: "deviceActionMap" }
    );
  });

  it("adopts a dead prior name for a mint orphan by usage context", () => {
    const fresh = [
      M("k3", "route(\x00, GET);", "wire(\x00);"),
      M("m9", "route(\x00, POST);", "hook(\x00);")
    ];
    const prior = [
      M("postHandler", "route(\x00, POST);", "hook(\x00);"),
      M("getHandler", "route(\x00, GET);", "wire(\x00);")
    ];
    const map = new Map(
      assignBucket(fresh, prior).map((a) => [a.fromName, a.toName])
    );
    assert.strictEqual(map.get("k3"), "getHandler");
    assert.strictEqual(map.get("m9"), "postHandler");
  });

  it("SWAP-CORRECTS two locked names the matcher placed on the wrong members", () => {
    const fresh = [
      M("pbkdf2IterationCount", "for (\x00 of flags)", "flags[\x00]"),
      M("flagCharIndex", "pbkdf2(pw, salt, \x00)", "rounds = \x00")
    ];
    const prior = [
      M("flagCharIndex", "for (\x00 of flags)", "flags[\x00]"),
      M("pbkdf2IterationCount", "pbkdf2(pw, salt, \x00)", "rounds = \x00")
    ];
    const map = new Map(
      assignBucket(fresh, prior).map((a) => [a.fromName, a.toName])
    );
    assert.strictEqual(map.get("pbkdf2IterationCount"), "flagCharIndex");
    assert.strictEqual(map.get("flagCharIndex"), "pbkdf2IterationCount");
  });

  it("does NOT move a merely-ambiguous member (own name is as good as any)", () => {
    // Identical usage context — no strict improvement, so the pass leaves
    // them exactly as rendered. Forcing a positional assignment here is the
    // +50,606 disaster: position does not correspond across versions.
    const fresh = [M("alpha", "use(\x00)"), M("beta", "use(\x00)")];
    const prior = [M("beta", "use(\x00)"), M("alpha", "use(\x00)")];
    assert.deepStrictEqual(assignBucket(fresh, prior), []);
  });

  it("never targets a minted-looking prior leftover", () => {
    const fresh = [M("q7x", "spawn(\x00)")];
    const prior = [M("__s", "spawn(\x00)")];
    assert.deepStrictEqual(assignBucket(fresh, prior), []);
  });

  it("refuses a mint orphan with zero context support (no evidence)", () => {
    const fresh = [M("q7x", "\x00.unrelated()")];
    const prior = [M("deadName", "\x00.somethingElse()")];
    assert.deepStrictEqual(assignBucket(fresh, prior), []);
  });

  it("respects the eligibility gate on the fresh name", () => {
    const fresh = [M("__helper", "use(\x00)")];
    const prior = [M("deadName", "use(\x00)")];
    assert.deepStrictEqual(
      assignBucket(fresh, prior, (n) => n !== "__helper"),
      []
    );
  });

  it("is DETERMINISTIC — identical input yields identical output", () => {
    const fresh = [
      M("z9", "handle(\x00, A)", "reg(\x00)"),
      M("k3", "handle(\x00, B)", "reg(\x00)")
    ];
    const prior = [
      M("beta", "handle(\x00, B)", "reg(\x00)"),
      M("alpha", "handle(\x00, A)", "reg(\x00)")
    ];
    assert.deepStrictEqual(
      assignBucket(fresh, prior),
      assignBucket(fresh, prior)
    );
  });
});
