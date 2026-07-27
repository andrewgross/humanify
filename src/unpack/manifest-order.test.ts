import assert from "node:assert";
import { describe, it } from "node:test";
import type { BunModulesManifestEntry } from "./adapters/bun.js";
import {
  MANIFEST_PRIOR_ORDER_OFF_ENV,
  annotateHashOrdinals,
  orderByPriorManifest
} from "./manifest-order.js";

/** Terse entry builder — only the fields the ordering actually reads. */
function e(
  fileName: string,
  name: string,
  structuralHash: string
): BunModulesManifestEntry {
  return { fileName, name, nameSource: "carry-over", structuralHash };
}

describe("annotateHashOrdinals", () => {
  it("leaves a singleton structuralHash group without the field", () => {
    const out = annotateHashOrdinals([e("vendor/a.js", "a", "h1")]);
    assert.deepEqual(out, [e("vendor/a.js", "a", "h1")]);
    assert.ok(!("hashOrdinal" in (out[0] ?? {})));
  });

  it("numbers the members of a >=2-member group in bundle order", () => {
    const out = annotateHashOrdinals([
      e("vendor/a.js", "a", "dup"),
      e("vendor/solo.js", "solo", "h2"),
      e("vendor/b.js", "b", "dup")
    ]);
    assert.equal(out[0]?.hashOrdinal, 0);
    assert.equal(out[1]?.hashOrdinal, undefined);
    assert.equal(out[2]?.hashOrdinal, 1);
  });

  it("writes NO field at all when the kill switch is set", () => {
    // The kill switch has to revert the whole of exp047, not just the ordering:
    // it is what produces the pre-exp047 CONTROL leg of a same-session A/B, so a
    // control that still carried `hashOrdinal` would not be a control.
    process.env[MANIFEST_PRIOR_ORDER_OFF_ENV] = "1";
    try {
      const out = annotateHashOrdinals([
        e("vendor/a.js", "a", "dup"),
        e("vendor/b.js", "b", "dup")
      ]);
      assert.deepEqual(out, [
        e("vendor/a.js", "a", "dup"),
        e("vendor/b.js", "b", "dup")
      ]);
      for (const row of out) assert.ok(!("hashOrdinal" in row));
    } finally {
      delete process.env[MANIFEST_PRIOR_ORDER_OFF_ENV];
    }
  });

  it("is inert across a release that only reorders the bundle", () => {
    // The whole reason this field is `hashOrdinal` and not `bundleIndex`:
    // exp047 measured `bundleIndex` at 7,056 manifest churn lines against a
    // 6,407 baseline -- worse than doing nothing -- because it records the
    // churn. A group's internal ordinals survive a global reshuffle.
    const before = annotateHashOrdinals([
      e("vendor/a.js", "a", "dup"),
      e("vendor/z.js", "z", "h9"),
      e("vendor/b.js", "b", "dup")
    ]);
    const after = annotateHashOrdinals([
      e("vendor/z.js", "z", "h9"),
      e("vendor/a.js", "a", "dup"),
      e("vendor/b.js", "b", "dup")
    ]);
    const ordinalOf = (rows: BunModulesManifestEntry[], f: string) =>
      rows.find((r) => r.fileName === f)?.hashOrdinal;
    for (const f of ["vendor/a.js", "vendor/b.js", "vendor/z.js"]) {
      assert.equal(ordinalOf(before, f), ordinalOf(after, f), f);
    }
  });
});

describe("orderByPriorManifest", () => {
  it("emits an unchanged entry set in the prior release's order", () => {
    const prior = [
      e("vendor/a.js", "a", "h1"),
      e("vendor/b.js", "b", "h2"),
      e("vendor/c.js", "c", "h3")
    ];
    // Bun reshuffled the factories this build.
    const fresh = [
      e("vendor/c.js", "c", "h3"),
      e("vendor/a.js", "a", "h1"),
      e("vendor/b.js", "b", "h2")
    ];
    assert.deepEqual(
      orderByPriorManifest(fresh, prior).map((f) => f.fileName),
      ["vendor/a.js", "vendor/b.js", "vendor/c.js"]
    );
  });

  it("keeps a genuinely new entry LOCAL rather than appending it", () => {
    // Appending unmatched entries to the tail was measured at +494 lines on
    // 197->198 and +8 on the 118->119 canary: relocating an entry is never
    // cheaper than editing it in place.
    const prior = [e("vendor/a.js", "a", "h1"), e("vendor/b.js", "b", "h2")];
    const fresh = [
      e("vendor/a.js", "a", "h1"),
      e("vendor/new.js", "new", "hNEW"),
      e("vendor/b.js", "b", "h2")
    ];
    assert.deepEqual(
      orderByPriorManifest(fresh, prior).map((f) => f.fileName),
      ["vendor/a.js", "vendor/new.js", "vendor/b.js"]
    );
  });

  it("pairs a content-changed entry back into its own prior slot", () => {
    // A library whose content changed has a fresh structuralHash AND a fresh
    // `lib_<hash>` fileName, so neither key matches. It must still land where
    // its prior version sat, or the diff charges a relocation on top of the
    // real edit.
    const prior = [
      e("vendor/a.js", "a", "h1"),
      e("vendor/lib_old.js", "lib_old", "hOLD"),
      e("vendor/c.js", "c", "h3")
    ];
    const fresh = [
      e("vendor/c.js", "c", "h3"),
      e("vendor/a.js", "a", "h1"),
      e("vendor/lib_new.js", "lib_new", "hNEW")
    ];
    assert.deepEqual(
      orderByPriorManifest(fresh, prior).map((f) => f.fileName),
      ["vendor/a.js", "vendor/lib_new.js", "vendor/c.js"]
    );
  });

  it("falls back to the library name when the hash rotated but the name held", () => {
    const prior = [
      e("vendor/keep.js", "keep", "h1"),
      e("vendor/lodash.js", "lodash", "hOLD")
    ];
    const fresh = [
      e("vendor/lodash.js", "lodash", "hNEW"),
      e("vendor/keep.js", "keep", "h1")
    ];
    assert.deepEqual(
      orderByPriorManifest(fresh, prior).map((f) => f.fileName),
      ["vendor/keep.js", "vendor/lodash.js"]
    );
  });

  it("never drops or duplicates an entry", () => {
    const prior = [e("vendor/a.js", "a", "h1"), e("vendor/b.js", "b", "h2")];
    const fresh = [
      e("vendor/b.js", "b", "h2"),
      e("vendor/x.js", "x", "hX"),
      e("vendor/a.js", "a", "h1"),
      e("vendor/y.js", "y", "hY")
    ];
    const out = orderByPriorManifest(fresh, prior);
    assert.equal(out.length, fresh.length);
    assert.deepEqual(
      [...out].map((f) => f.fileName).sort(),
      [...fresh].map((f) => f.fileName).sort()
    );
  });

  it("is a fixed point on its own output (the self-hop invariant)", () => {
    const prior = [
      e("vendor/a.js", "a", "h1"),
      e("vendor/b.js", "b", "h2"),
      e("vendor/c.js", "c", "h3")
    ];
    const once = orderByPriorManifest([...prior].reverse(), prior);
    const twice = orderByPriorManifest(once, once);
    assert.deepEqual(
      twice.map((f) => f.fileName),
      once.map((f) => f.fileName)
    );
  });

  it("returns bundle order untouched with no prior manifest", () => {
    const fresh = [e("vendor/b.js", "b", "h2"), e("vendor/a.js", "a", "h1")];
    assert.deepEqual(
      orderByPriorManifest(fresh, undefined).map((f) => f.fileName),
      ["vendor/b.js", "vendor/a.js"]
    );
  });

  it("returns bundle order untouched when the kill switch is set", () => {
    const prior = [e("vendor/a.js", "a", "h1"), e("vendor/b.js", "b", "h2")];
    const fresh = [e("vendor/b.js", "b", "h2"), e("vendor/a.js", "a", "h1")];
    process.env[MANIFEST_PRIOR_ORDER_OFF_ENV] = "1";
    try {
      assert.deepEqual(
        orderByPriorManifest(fresh, prior).map((f) => f.fileName),
        ["vendor/b.js", "vendor/a.js"]
      );
    } finally {
      delete process.env[MANIFEST_PRIOR_ORDER_OFF_ENV];
    }
  });
});
