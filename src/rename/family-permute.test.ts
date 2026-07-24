import assert from "node:assert";
import { describe, it } from "node:test";
import {
  assignFamilyBucket,
  deriveLocalRenames,
  editedLineCount,
  reassignmentsOnly
} from "./family-permute.js";

describe("editedLineCount", () => {
  it("counts lines of a not present in b", () => {
    assert.strictEqual(editedLineCount("x\ny\nz", "x\nY\nz"), 1);
    assert.strictEqual(editedLineCount("same", "same"), 0);
  });
});

describe("assignFamilyBucket (exp036 idea 8b core)", () => {
  it("keeps byte-identical members put and reassigns the rest", () => {
    // Two same-shape members; one fresh already matches a prior, the
    // other drew a fresh name and must adopt the remaining prior.
    const prior = ["var alpha = load();", "var beta = load();"];
    const fresh = ["var alpha = load();", "var freshDraw = load();"];
    const a = assignFamilyBucket(fresh, prior);
    // fresh[0] already clean -> prior[0]; fresh[1] -> prior[1].
    assert.deepStrictEqual(
      a.map((x) => [x.freshIndex, x.priorIndex, x.alreadyClean]),
      [
        [0, 0, true],
        [1, 1, false]
      ]
    );
    assert.deepStrictEqual(
      reassignmentsOnly(a).map((x) => x.freshIndex),
      [1]
    );
  });

  it("is a no-op when every member is already clean", () => {
    const both = ["var a = f();", "var b = f();", "var c = f();"];
    const a = assignFamilyBucket(both, both);
    assert.ok(a.every((x) => x.alreadyClean));
    assert.strictEqual(reassignmentsOnly(a).length, 0);
  });

  it("recovers a swap: fresh members hold each other's prior names", () => {
    // The rotation case: prior [alpha, beta]; fresh rendered them as
    // [beta, alpha]. Byte-match consumes both crosswise -> both clean,
    // 0 reassignments (the diff is already zero once paired right).
    const prior = ["var alpha = k();", "var beta = k();"];
    const fresh = ["var beta = k();", "var alpha = k();"];
    const a = assignFamilyBucket(fresh, prior);
    assert.deepStrictEqual(
      a.map((x) => [x.freshIndex, x.priorIndex]),
      [
        [0, 1],
        [1, 0]
      ]
    );
    assert.ok(a.every((x) => x.alreadyClean));
  });

  it("picks the least-churn permutation among clean bijections", () => {
    // Neither fresh matches byte-for-byte; both are name-only diffs.
    // fresh[0] is closer to prior[0] (1 edited line) than prior[1].
    const prior = [
      "var config = build();\nconfig.ready = true;",
      "var registry = build();\nregistry.count = 0;"
    ];
    const fresh = [
      "var cfg = build();\ncfg.ready = true;",
      "var reg = build();\nreg.count = 0;"
    ];
    const a = assignFamilyBucket(fresh, prior);
    assert.deepStrictEqual(
      a.map((x) => x.priorIndex),
      [0, 1],
      "each fresh pairs with its closest prior, not crosswise"
    );
  });

  it("is deterministic and order-stable across repeated runs", () => {
    const prior = ["var a = z();", "var b = z();", "var c = z();"];
    const fresh = ["var p = z();", "var q = z();", "var r = z();"];
    const first = assignFamilyBucket(fresh, prior);
    const second = assignFamilyBucket(fresh, prior);
    assert.deepStrictEqual(first, second);
    // Every fresh assigned exactly one distinct prior (a bijection).
    const priors = new Set(first.map((x) => x.priorIndex));
    assert.strictEqual(priors.size, 3);
  });

  it("rejects unequal counts (membership churn is not this tier's job)", () => {
    assert.throws(() => assignFamilyBucket(["var a=f();"], []));
  });
});

describe("deriveLocalRenames (the safe slot-mapping owner gate)", () => {
  const locals = (...n: string[]) => new Set(n);

  it("maps local-binding differences to prior names", () => {
    const m = deriveLocalRenames(
      "var freshA = load(), freshB = init();",
      "var priorA = load(), priorB = init();",
      locals("freshA", "freshB")
    );
    assert.deepStrictEqual(
      m && [...m],
      [
        ["freshA", "priorA"],
        ["freshB", "priorB"]
      ]
    );
  });

  it("returns null when a PROPERTY name differs (non-rename change)", () => {
    // obj.foo vs obj.bar — same statement hash (property masked) but a
    // real API difference; renaming a local can't zero it, so refuse.
    const m = deriveLocalRenames(
      "var x = obj.foo();",
      "var y = obj.bar();",
      locals("x")
    );
    assert.strictEqual(m, null);
  });

  it("returns null when a FREE identifier differs (semantic break risk)", () => {
    // `helperA` vs `helperB` are not statement-local — renaming them
    // would rebind an outer reference. Refuse.
    const m = deriveLocalRenames(
      "var x = helperA(1);",
      "var x = helperB(1);",
      locals("x")
    );
    assert.strictEqual(m, null);
  });

  it("returns null on inconsistent mapping or structural misalignment", () => {
    // Same local name would need two different prior names.
    assert.strictEqual(
      deriveLocalRenames(
        "var a = f(a, a);",
        "var b = f(b, c);",
        locals("a")
      ),
      null
    );
    // Different separator structure (extra arg) — not permute-equivalent.
    assert.strictEqual(
      deriveLocalRenames("var a = f(1);", "var b = f(1, 2);", locals("a")),
      null
    );
  });

  it("empty map when already identical (no rename needed)", () => {
    const m = deriveLocalRenames("var a = f();", "var a = f();", locals("a"));
    assert.deepStrictEqual(m && [...m], []);
  });
});
