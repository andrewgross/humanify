import assert from "node:assert";
import { describe, it } from "node:test";
import { createIsEligible } from "./rename-eligibility.js";

describe("createIsEligible", () => {
  describe("short names are eligible", () => {
    it("marks single-char names as eligible", () => {
      const isEligible = createIsEligible();
      assert.strictEqual(isEligible("a"), true);
      assert.strictEqual(isEligible("x"), true);
      assert.strictEqual(isEligible("Z"), true);
      assert.strictEqual(isEligible("_"), true);
      assert.strictEqual(isEligible("$"), true);
    });

    it("marks common short names as eligible (inverted from old heuristic)", () => {
      const isEligible = createIsEligible();
      assert.strictEqual(isEligible("get"), true);
      assert.strictEqual(isEligible("set"), true);
      assert.strictEqual(isEligible("map"), true);
      assert.strictEqual(isEligible("val"), true);
      assert.strictEqual(isEligible("id"), true);
      assert.strictEqual(isEligible("fn"), true);
    });

    it("marks minified-looking short names as eligible", () => {
      const isEligible = createIsEligible();
      assert.strictEqual(isEligible("Ab"), true);
      assert.strictEqual(isEligible("xRT"), true);
    });
  });

  describe("private class members are eligible", () => {
    it("marks private members as eligible without special handling", () => {
      const isEligible = createIsEligible();
      assert.strictEqual(isEligible("#Z"), true);
      assert.strictEqual(isEligible("#ab"), true);
      assert.strictEqual(isEligible("#xRT"), true);
    });
  });

  describe("longer descriptive names are eligible", () => {
    it("marks normal long names as eligible", () => {
      const isEligible = createIsEligible();
      assert.strictEqual(isEligible("counter"), true);
      assert.strictEqual(isEligible("value"), true);
      assert.strictEqual(isEligible("handleClick"), true);
    });
  });

  describe("universal skip names are not eligible", () => {
    it("skips module system identifiers", () => {
      const isEligible = createIsEligible();
      assert.strictEqual(isEligible("exports"), false);
      assert.strictEqual(isEligible("require"), false);
      assert.strictEqual(isEligible("module"), false);
      assert.strictEqual(isEligible("__filename"), false);
      assert.strictEqual(isEligible("__dirname"), false);
    });
  });

  describe("bundler-specific skips", () => {
    it("skips webpack helpers with webpack bundler", () => {
      const isEligible = createIsEligible("webpack", "terser");
      assert.strictEqual(isEligible("__webpack_require__"), false);
      assert.strictEqual(isEligible("__webpack_modules__"), false);
    });

    it("skips esbuild helpers with esbuild bundler", () => {
      const isEligible = createIsEligible("esbuild", "esbuild");
      assert.strictEqual(isEligible("__toESM"), false);
      assert.strictEqual(isEligible("__commonJS"), false);
    });

    it("skips swc helpers with swc minifier", () => {
      const isEligible = createIsEligible("unknown", "swc");
      assert.strictEqual(isEligible("_interop_require_default"), false);
      assert.strictEqual(isEligible("_class_call_check"), false);
    });
  });

  describe("pattern-based skips", () => {
    it("skips WORD-LIKE double-underscore names (bundler helpers)", () => {
      const isEligible = createIsEligible();
      assert.strictEqual(isEligible("__someNewHelper"), false);
      assert.strictEqual(isEligible("__customRuntime"), false);
      // Real helper words stay reserved regardless of enumeration.
      assert.strictEqual(isEligible("__esm"), false);
      assert.strictEqual(isEligible("__createBinding"), false);
      assert.strictEqual(isEligible("__exportStar"), false);
    });

    it("frees SHORT minted double-underscore bindings (exp036 idea 6)", () => {
      // The minifier emits `__c`, `__t`, `__s` for app bindings; the
      // blanket rule wrongly reserved them (measured: 22 such bindings
      // on 216, vs 0 real helpers of this shape — Bun minifies its
      // helpers to single letters like Q/b, never __-prefixed). A short
      // minted dunder is a naming gap, not a reservation. The boot gate
      // is the safety net if the oracle ever misses a real helper.
      const isEligible = createIsEligible();
      for (const name of ["__c", "__t", "__s", "__ab", "__x9"]) {
        assert.strictEqual(
          isEligible(name),
          true,
          `${name} should be renameable`
        );
      }
    });

    it("skips SWC helper pattern names", () => {
      const isEligible = createIsEligible();
      assert.strictEqual(isEligible("_some_helper_fn"), false);
      assert.strictEqual(isEligible("_interop_require"), false);
    });

    it("does not skip single-underscore simple names", () => {
      const isEligible = createIsEligible();
      // Single underscore with a simple name is not SWC pattern
      assert.strictEqual(isEligible("_private"), true);
      assert.strictEqual(isEligible("_init"), true);
    });
  });

  describe("empty string", () => {
    it("treats empty string as not eligible", () => {
      const isEligible = createIsEligible();
      assert.strictEqual(isEligible(""), false);
    });
  });
});
