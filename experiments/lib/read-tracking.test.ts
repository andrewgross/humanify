import assert from "node:assert";
import { describe, it } from "node:test";
import { leafPaths, trackReads, unreadLeaves } from "./read-tracking.js";

describe("read tracking", () => {
  it("enumerates leaves, treating arrays as leaves", () => {
    assert.deepStrictEqual(
      leafPaths({ a: 1, b: { c: "x", d: [1, 2] } }).sort(),
      ["a", "b.c", "b.d"]
    );
  });

  it("records what a consumer actually touched", () => {
    const rec = { provenance: { commit: "abc", dirty: false }, pair: "p" };
    const t = trackReads(rec);
    const consumer = (m: typeof rec) => m.provenance.commit;
    assert.strictEqual(consumer(t.proxy), "abc");
    assert.ok(t.read.has("provenance.commit"));
    assert.ok(!t.read.has("provenance.dirty"));
  });

  it("reaching for a container does NOT count as reading its leaves", () => {
    // The bands bug lived exactly here: the file was loaded and one field
    // consulted, while provenance.commit went unread and the guard passed.
    const rec = { provenance: { commit: "abc", provisional: false } };
    const t = trackReads(rec);
    const consumer = (m: typeof rec) => m.provenance.provisional;
    consumer(t.proxy);
    assert.deepStrictEqual(unreadLeaves(rec, t), ["provenance.commit"]);
  });

  it("declared archival fields are not reported", () => {
    const rec = { node: "v24", pair: "p" };
    const t = trackReads(rec);
    ((m: typeof rec) => m.pair)(t.proxy);
    assert.deepStrictEqual(unreadLeaves(rec, t, ["node"]), []);
  });

  it("reports nothing when every leaf is consulted", () => {
    const rec = { a: 1, b: { c: 2 } };
    const t = trackReads(rec);
    ((m: typeof rec) => [m.a, m.b.c])(t.proxy);
    assert.deepStrictEqual(unreadLeaves(rec, t), []);
  });
});
