import { describe, it } from "node:test";
import assert from "node:assert";
import { looksMinified } from "./minified-heuristic.js";

describe("looksMinified", () => {
  describe("1-char names", () => {
    it("flags single letters as minified", () => {
      assert.strictEqual(looksMinified("a"), true);
      assert.strictEqual(looksMinified("x"), true);
      assert.strictEqual(looksMinified("Z"), true);
    });

    it("flags _ and $ as minified", () => {
      assert.strictEqual(looksMinified("_"), true);
      assert.strictEqual(looksMinified("$"), true);
    });
  });

  describe("2-char names", () => {
    it("flags unknown 2-char names as minified", () => {
      assert.strictEqual(looksMinified("Ab"), true);
      assert.strictEqual(looksMinified("xY"), true);
      assert.strictEqual(looksMinified("zz"), true);
    });

    it("preserves common 2-char names", () => {
      assert.strictEqual(looksMinified("id"), false);
      assert.strictEqual(looksMinified("fn"), false);
      assert.strictEqual(looksMinified("cb"), false);
      assert.strictEqual(looksMinified("el"), false);
      assert.strictEqual(looksMinified("db"), false);
      assert.strictEqual(looksMinified("io"), false);
      assert.strictEqual(looksMinified("fs"), false);
      assert.strictEqual(looksMinified("os"), false);
      assert.strictEqual(looksMinified("ok"), false);
      assert.strictEqual(looksMinified("on"), false);
      assert.strictEqual(looksMinified("is"), false);
    });
  });

  describe("3-char names", () => {
    it("preserves common 3-char names", () => {
      assert.strictEqual(looksMinified("get"), false);
      assert.strictEqual(looksMinified("set"), false);
      assert.strictEqual(looksMinified("map"), false);
      assert.strictEqual(looksMinified("run"), false);
      assert.strictEqual(looksMinified("key"), false);
      assert.strictEqual(looksMinified("val"), false);
      assert.strictEqual(looksMinified("ref"), false);
      assert.strictEqual(looksMinified("err"), false);
      assert.strictEqual(looksMinified("msg"), false);
      assert.strictEqual(looksMinified("req"), false);
      assert.strictEqual(looksMinified("res"), false);
      assert.strictEqual(looksMinified("src"), false);
      assert.strictEqual(looksMinified("buf"), false);
      assert.strictEqual(looksMinified("len"), false);
      assert.strictEqual(looksMinified("idx"), false);
      assert.strictEqual(looksMinified("url"), false);
      assert.strictEqual(looksMinified("api"), false);
      assert.strictEqual(looksMinified("app"), false);
      assert.strictEqual(looksMinified("env"), false);
      assert.strictEqual(looksMinified("log"), false);
    });

    it("flags Bun-style 3-char minified names", () => {
      assert.strictEqual(looksMinified("rlA"), true);  // lowercase-uppercase pattern
      assert.strictEqual(looksMinified("oGD"), true);  // lowercase-uppercase pattern
      assert.strictEqual(looksMinified("T5D"), true);  // has digit
      assert.strictEqual(looksMinified("$aT"), true);  // $ prefix + uppercase
      assert.strictEqual(looksMinified("HaT"), true);  // ends with uppercase after lowercase
      assert.strictEqual(looksMinified("xRT"), true);  // lowercase-uppercase pattern
    });

    it("preserves normal 3-char names with regular casing", () => {
      assert.strictEqual(looksMinified("foo"), false);
      assert.strictEqual(looksMinified("bar"), false);
      assert.strictEqual(looksMinified("baz"), false);
    });
  });

  describe("4-char names", () => {
    it("flags names with digits", () => {
      assert.strictEqual(looksMinified("q5aT"), true);
      assert.strictEqual(looksMinified("a2b3"), true);
      assert.strictEqual(looksMinified("x1yZ"), true);
    });

    it("flags names with unusual casing", () => {
      assert.strictEqual(looksMinified("xRTd"), true); // lowercase-uppercase-uppercase
      assert.strictEqual(looksMinified("aBC"), true);   // single lower then 2+ upper
    });

    it("preserves normal 4-char names", () => {
      assert.strictEqual(looksMinified("name"), false);
      assert.strictEqual(looksMinified("data"), false);
      assert.strictEqual(looksMinified("list"), false);
      assert.strictEqual(looksMinified("item"), false);
      assert.strictEqual(looksMinified("node"), false);
      assert.strictEqual(looksMinified("push"), false);
      assert.strictEqual(looksMinified("sort"), false);
      assert.strictEqual(looksMinified("keys"), false);
      assert.strictEqual(looksMinified("type"), false);
      assert.strictEqual(looksMinified("path"), false);
    });

    it("preserves camelCase 4-char names", () => {
      assert.strictEqual(looksMinified("getX"), false);
      assert.strictEqual(looksMinified("setY"), false);
    });
  });

  describe("5+ char names", () => {
    it("never flags names 5+ chars as minified", () => {
      assert.strictEqual(looksMinified("value"), false);
      assert.strictEqual(looksMinified("exports"), false);
      assert.strictEqual(looksMinified("require"), false);
      assert.strictEqual(looksMinified("module"), false);
      assert.strictEqual(looksMinified("Object"), false);
      assert.strictEqual(looksMinified("xRTdE"), false); // even weird casing
      assert.strictEqual(looksMinified("a1b2c"), false); // even with digits
    });
  });

  describe("edge cases", () => {
    it("handles empty string", () => {
      assert.strictEqual(looksMinified(""), false);
    });
  });
});
