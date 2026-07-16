import assert from "node:assert";
import { describe, it } from "node:test";
import { uniqueCaseInsensitiveName } from "./unique-name.js";

describe("uniqueCaseInsensitiveName", () => {
  it("returns the stem unchanged when unused", () => {
    const used = new Set<string>();
    assert.strictEqual(uniqueCaseInsensitiveName("foo", used, ".js"), "foo.js");
  });

  it("suffixes -2, -3 on repeat, before the extension", () => {
    const used = new Set<string>();
    assert.strictEqual(uniqueCaseInsensitiveName("foo", used, ".js"), "foo.js");
    assert.strictEqual(
      uniqueCaseInsensitiveName("foo", used, ".js"),
      "foo-2.js"
    );
    assert.strictEqual(
      uniqueCaseInsensitiveName("foo", used, ".js"),
      "foo-3.js"
    );
  });

  it("folds case: names differing only in case do not collide on disk", () => {
    const used = new Set<string>();
    // The bug the shared helper fixes: a case-sensitive uniquify would return
    // Ab.js and aB.js, which collapse to one file on macOS/Windows.
    assert.strictEqual(uniqueCaseInsensitiveName("Ab", used, ".js"), "Ab.js");
    assert.strictEqual(uniqueCaseInsensitiveName("aB", used, ".js"), "aB-2.js");
    assert.strictEqual(uniqueCaseInsensitiveName("AB", used, ".js"), "AB-3.js");
    // First-writer keeps its exact casing; the rest are suffixed.
    assert.strictEqual(new Set([...used]).size, 3);
  });

  it("works with no extension (the caller appends one later)", () => {
    const used = new Set<string>();
    assert.strictEqual(
      uniqueCaseInsensitiveName("axios@1.0.0", used),
      "axios@1.0.0"
    );
    assert.strictEqual(
      uniqueCaseInsensitiveName("Axios@1.0.0", used),
      "Axios@1.0.0-2"
    );
  });
});
