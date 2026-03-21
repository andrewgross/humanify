import assert from "node:assert";
import { describe, it } from "node:test";
import { createSkipSet } from "./skip-list.js";

describe("createSkipSet", () => {
  describe("universal names", () => {
    it("includes universal names in all combinations", () => {
      const universals = [
        "exports",
        "require",
        "module",
        "__filename",
        "__dirname"
      ];

      for (const combo of [
        createSkipSet(),
        createSkipSet("webpack", "terser"),
        createSkipSet("esbuild", "esbuild"),
        createSkipSet("unknown", "swc"),
        createSkipSet("rollup", "unknown"),
        createSkipSet("bun", "bun")
      ]) {
        for (const name of universals) {
          assert.ok(combo.has(name), `Expected "${name}" in skip set`);
        }
      }
    });
  });

  describe("webpack names", () => {
    it("includes webpack helpers when bundler is webpack", () => {
      const set = createSkipSet("webpack", "terser");
      assert.ok(set.has("__webpack_require__"));
      assert.ok(set.has("__webpack_modules__"));
      assert.ok(set.has("__webpack_exports__"));
      assert.ok(set.has("__webpack_module_cache__"));
    });

    it("does not include webpack helpers for esbuild bundler", () => {
      const set = createSkipSet("esbuild", "esbuild");
      assert.ok(!set.has("__webpack_require__"));
      assert.ok(!set.has("__webpack_modules__"));
    });
  });

  describe("esbuild names", () => {
    it("includes esbuild helpers when bundler is esbuild", () => {
      const set = createSkipSet("esbuild", "esbuild");
      assert.ok(set.has("__commonJS"));
      assert.ok(set.has("__toESM"));
      assert.ok(set.has("__toCommonJS"));
      assert.ok(set.has("__export"));
      assert.ok(set.has("__require"));
      assert.ok(set.has("__name"));
      assert.ok(set.has("__publicField"));
    });

    it("does not include esbuild helpers for webpack bundler", () => {
      const set = createSkipSet("webpack", "terser");
      assert.ok(!set.has("__commonJS"));
      assert.ok(!set.has("__toESM"));
    });
  });

  describe("swc names", () => {
    it("includes swc helpers when minifier is swc", () => {
      const set = createSkipSet("unknown", "swc");
      assert.ok(set.has("_interop_require_default"));
      assert.ok(set.has("_class_call_check"));
      assert.ok(set.has("_create_class"));
    });

    it("does not include swc helpers for terser minifier", () => {
      const set = createSkipSet("webpack", "terser");
      assert.ok(!set.has("_interop_require_default"));
      assert.ok(!set.has("_class_call_check"));
    });
  });

  describe("names that should NOT be in any skip set", () => {
    it("does not contain common short names", () => {
      for (const combo of [
        createSkipSet(),
        createSkipSet("webpack", "terser"),
        createSkipSet("esbuild", "esbuild"),
        createSkipSet("unknown", "swc")
      ]) {
        assert.ok(!combo.has("get"), `"get" should not be in skip set`);
        assert.ok(!combo.has("set"), `"set" should not be in skip set`);
        assert.ok(!combo.has("map"), `"map" should not be in skip set`);
      }
    });

    it("does not contain private member names", () => {
      for (const combo of [
        createSkipSet(),
        createSkipSet("webpack", "terser"),
        createSkipSet("esbuild", "esbuild")
      ]) {
        assert.ok(!combo.has("#Z"), `"#Z" should not be in skip set`);
        assert.ok(!combo.has("#ab"), `"#ab" should not be in skip set`);
      }
    });
  });

  describe("caching", () => {
    it("returns the same Set reference for the same arguments", () => {
      const a = createSkipSet("webpack", "terser");
      const b = createSkipSet("webpack", "terser");
      assert.strictEqual(a, b);
    });

    it("returns different Set references for different arguments", () => {
      const a = createSkipSet("webpack", "terser");
      const b = createSkipSet("esbuild", "esbuild");
      assert.notStrictEqual(a, b);
    });

    it("caches undefined arguments consistently", () => {
      const a = createSkipSet();
      const b = createSkipSet();
      assert.strictEqual(a, b);
    });
  });
});
