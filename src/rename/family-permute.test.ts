import assert from "node:assert";
import { describe, it } from "node:test";
import { assignByContext } from "./family-permute.js";

describe("assignByContext (evidence-based, exp036 8b redesign)", () => {
  const M = (name: string, ...contexts: string[]) => ({ name, contexts });

  it("LOCKS a name present on both sides — never reassigns it", () => {
    // The getClaudeCodeOAuthToken disaster: a correct name that
    // round-trips must be left alone even though the pool has other
    // members. Only the genuine orphan (mint q7x -> dead deviceActionMap)
    // may move, and only with context support.
    const fresh = [
      M("getClaudeCodeOAuthToken", "let t = \x00();", "\x00().accessToken"),
      M("q7x", "let d = \x00();", "\x00().deviceMap")
    ];
    const prior = [
      M("getClaudeCodeOAuthToken", "let t = \x00();", "\x00().accessToken"),
      M("deviceActionMap", "let d = \x00();", "\x00().deviceMap")
    ];
    const out = assignByContext(fresh, prior);
    assert.strictEqual(out.length, 1, "only the orphan moves");
    assert.deepStrictEqual(
      { from: out[0].fromName, to: out[0].toName },
      { from: "q7x", to: "deviceActionMap" }
    );
    assert.ok(
      !out.some((a) => a.fromName === "getClaudeCodeOAuthToken"),
      "the correct round-tripping name is never touched"
    );
  });

  it("pairs orphans by usage context, not pool order", () => {
    // Two fresh mints, two dead prior names; context decides which is
    // which (source/pool order would cross them up).
    const fresh = [M("k3", "route(\x00, GET);"), M("m9", "route(\x00, POST);")];
    const prior = [
      M("postHandler", "route(\x00, POST);"),
      M("getHandler", "route(\x00, GET);")
    ];
    const out = assignByContext(fresh, prior);
    const map = new Map(out.map((a) => [a.fromName, a.toName]));
    assert.strictEqual(map.get("k3"), "getHandler", "k3 used with GET");
    assert.strictEqual(map.get("m9"), "postHandler", "m9 used with POST");
  });

  it("refuses an orphan with zero context support (no evidence)", () => {
    const fresh = [M("q7x", "\x00.unrelated()")];
    const prior = [M("deadName", "\x00.somethingElse()")];
    assert.deepStrictEqual(assignByContext(fresh, prior), []);
  });

  it("respects the eligibility gate on the fresh name", () => {
    const fresh = [M("__helper", "use(\x00)")];
    const prior = [M("deadName", "use(\x00)")];
    assert.deepStrictEqual(
      assignByContext(fresh, prior, (n) => n !== "__helper"),
      []
    );
  });

  it("no orphans on either side => no moves", () => {
    const both = [M("a", "x(\x00)"), M("b", "y(\x00)")];
    assert.deepStrictEqual(assignByContext(both, both), []);
  });
});
