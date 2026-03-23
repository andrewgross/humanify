import assert from "node:assert";
import { describe, it } from "node:test";
import type { LLMContext } from "../analysis/types.js";
import {
  GLOBAL_BUILTINS,
  isValidIdentifier,
  RESERVED_WORDS,
  resolveConflict,
  sanitizeIdentifier,
  validateSuggestion
} from "./validation.js";

describe("validation", () => {
  describe("isValidIdentifier", () => {
    it("accepts valid identifiers", () => {
      assert.strictEqual(isValidIdentifier("foo"), true);
      assert.strictEqual(isValidIdentifier("_foo"), true);
      assert.strictEqual(isValidIdentifier("$foo"), true);
      assert.strictEqual(isValidIdentifier("foo123"), true);
      assert.strictEqual(isValidIdentifier("camelCase"), true);
      assert.strictEqual(isValidIdentifier("PascalCase"), true);
      assert.strictEqual(isValidIdentifier("_privateVar"), true);
      assert.strictEqual(isValidIdentifier("$jquery"), true);
    });

    it("rejects invalid identifiers", () => {
      assert.strictEqual(isValidIdentifier(""), false);
      assert.strictEqual(isValidIdentifier("123foo"), false);
      assert.strictEqual(isValidIdentifier("foo-bar"), false);
      assert.strictEqual(isValidIdentifier("foo bar"), false);
      assert.strictEqual(isValidIdentifier("foo.bar"), false);
    });
  });

  describe("sanitizeIdentifier", () => {
    it("removes invalid characters", () => {
      assert.strictEqual(sanitizeIdentifier("foo-bar"), "foobar");
      assert.strictEqual(sanitizeIdentifier("foo.bar"), "foobar");
      assert.strictEqual(sanitizeIdentifier("foo bar"), "foobar");
      assert.strictEqual(sanitizeIdentifier("foo@bar#baz"), "foobarbaz");
    });

    it("prefixes identifiers starting with numbers", () => {
      assert.strictEqual(sanitizeIdentifier("123foo"), "_123foo");
      assert.strictEqual(sanitizeIdentifier("1"), "_1");
    });

    it("handles empty input", () => {
      assert.strictEqual(sanitizeIdentifier(""), "_unnamed");
      // "@#$" becomes "$" because $ is a valid identifier character
      assert.strictEqual(sanitizeIdentifier("@#$"), "$");
      // Only truly empty result should give _unnamed
      assert.strictEqual(sanitizeIdentifier("@#%"), "_unnamed");
    });

    it("appends underscore to reserved words", () => {
      assert.strictEqual(sanitizeIdentifier("if"), "if_");
      assert.strictEqual(sanitizeIdentifier("class"), "class_");
      assert.strictEqual(sanitizeIdentifier("function"), "function_");
      assert.strictEqual(sanitizeIdentifier("return"), "return_");
    });

    it("appends underscore to global built-in names", () => {
      assert.strictEqual(sanitizeIdentifier("Date"), "Date_");
      assert.strictEqual(sanitizeIdentifier("Math"), "Math_");
      assert.strictEqual(sanitizeIdentifier("JSON"), "JSON_");
      assert.strictEqual(sanitizeIdentifier("console"), "console_");
      assert.strictEqual(sanitizeIdentifier("Promise"), "Promise_");
    });

    it("preserves valid identifiers", () => {
      assert.strictEqual(sanitizeIdentifier("validName"), "validName");
      assert.strictEqual(sanitizeIdentifier("_private"), "_private");
      assert.strictEqual(sanitizeIdentifier("$dollar"), "$dollar");
    });
  });

  describe("resolveConflict", () => {
    it("tries semantic suffixes first", () => {
      const used = new Set(["name"]);
      const result = resolveConflict("name", used);
      assert.strictEqual(result, "nameVal");
    });

    it("uses numeric suffix when semantic suffixes exhausted", () => {
      const used = new Set([
        "name",
        "nameVal",
        "nameVar",
        "nameRef",
        "nameItem",
        "nameData",
        "nameResult",
        "nameValue"
      ]);
      const result = resolveConflict("name", used);
      assert.strictEqual(result, "name2");
    });

    it("increments numeric suffix as needed", () => {
      const used = new Set([
        "name",
        "nameVal",
        "nameVar",
        "nameRef",
        "nameItem",
        "nameData",
        "nameResult",
        "nameValue",
        "name2",
        "name3"
      ]);
      const result = resolveConflict("name", used);
      assert.strictEqual(result, "name4");
    });

    it("falls back to underscore prefix as last resort", () => {
      // Create a set with all suffixes and numbers 2-100
      const used = new Set(["name"]);
      for (const suffix of [
        "Val",
        "Var",
        "Ref",
        "Item",
        "Data",
        "Result",
        "Value"
      ]) {
        used.add(`name${suffix}`);
      }
      for (let i = 2; i <= 100; i++) {
        used.add(`name${i}`);
      }

      const result = resolveConflict("name", used);
      assert.strictEqual(result, "_name");
    });

    it("never produces triple underscore prefixes", () => {
      // Exhaust all strategies before underscore to ensure no ___ stacking
      const used = new Set(["name"]);
      for (const suffix of [
        "Val",
        "Var",
        "Ref",
        "Item",
        "Data",
        "Result",
        "Value"
      ]) {
        used.add(`name${suffix}`);
      }
      for (let i = 2; i <= 100; i++) {
        used.add(`name${i}`);
      }
      used.add("_name");
      used.add("name_");
      used.add("local_name");
      used.add("inner_name");

      const result = resolveConflict("name", used);
      assert.ok(
        !result.includes("___"),
        `Result "${result}" should not contain ___`
      );
      assert.ok(
        !used.has(result),
        `Result "${result}" should not be in used set`
      );
    });

    it("uses trailing underscore when leading underscore is taken", () => {
      const used = new Set(["name"]);
      for (const suffix of [
        "Val",
        "Var",
        "Ref",
        "Item",
        "Data",
        "Result",
        "Value"
      ]) {
        used.add(`name${suffix}`);
      }
      for (let i = 2; i <= 100; i++) {
        used.add(`name${i}`);
      }
      used.add("_name");

      const result = resolveConflict("name", used);
      assert.strictEqual(result, "name_");
    });
  });

  describe("validateSuggestion", () => {
    const makeContext = (usedIdentifiers: string[] = []): LLMContext => ({
      functionCode: "function test() {}",
      calleeSignatures: [],
      callsites: [],
      usedIdentifiers: new Set(usedIdentifiers)
    });

    it("accepts valid suggestions", () => {
      const result = validateSuggestion({ name: "validName" }, makeContext());
      assert.strictEqual(result.valid, true);
    });

    it("rejects invalid identifier syntax", () => {
      const result = validateSuggestion({ name: "123invalid" }, makeContext());
      assert.strictEqual(result.valid, false);
      assert.ok(result.reason?.includes("Invalid"));
    });

    it("rejects reserved words", () => {
      const result = validateSuggestion({ name: "class" }, makeContext());
      assert.strictEqual(result.valid, false);
      assert.ok(result.reason?.includes("reserved"));
    });

    it("rejects global built-in names that would shadow globals", () => {
      for (const name of [
        "Date",
        "Math",
        "JSON",
        "Array",
        "Object",
        "Map",
        "Set",
        "Promise",
        "Error",
        "console",
        "File",
        "Blob",
        "globalThis"
      ]) {
        const result = validateSuggestion({ name }, makeContext());
        assert.strictEqual(result.valid, false, `Should reject "${name}"`);
        assert.ok(
          result.reason?.includes("global"),
          `Reason for "${name}" should mention global: ${result.reason}`
        );
      }
    });

    it("rejects names already in use", () => {
      const result = validateSuggestion(
        { name: "existingVar" },
        makeContext(["existingVar", "otherVar"])
      );
      assert.strictEqual(result.valid, false);
      assert.ok(result.reason?.includes("already in use"));
    });

    it("rejects overly long names", () => {
      const longName = "a".repeat(51);
      const result = validateSuggestion({ name: longName }, makeContext());
      assert.strictEqual(result.valid, false);
      assert.ok(result.reason?.includes("too long"));
    });

    it("accepts names at the length limit", () => {
      const maxName = "a".repeat(50);
      const result = validateSuggestion({ name: maxName }, makeContext());
      assert.strictEqual(result.valid, true);
    });
  });

  describe("RESERVED_WORDS", () => {
    it("contains common JavaScript keywords", () => {
      const expected = [
        "if",
        "else",
        "for",
        "while",
        "function",
        "class",
        "const",
        "let",
        "var",
        "return",
        "await"
        // Note: "async" is not a reserved word, only "await" is
      ];
      for (const word of expected) {
        assert.ok(RESERVED_WORDS.has(word), `Should contain "${word}"`);
      }
    });

    it("contains literal values", () => {
      assert.ok(RESERVED_WORDS.has("null"));
      assert.ok(RESERVED_WORDS.has("true"));
      assert.ok(RESERVED_WORDS.has("false"));
      assert.ok(RESERVED_WORDS.has("undefined"));
    });
  });

  describe("GLOBAL_BUILTINS", () => {
    it("contains core JavaScript globals", () => {
      const expected = [
        "Date",
        "Math",
        "JSON",
        "Array",
        "Object",
        "Map",
        "Set",
        "Promise",
        "Error",
        "RegExp",
        "Symbol",
        "Proxy",
        "Reflect",
        "console",
        "globalThis"
      ];
      for (const name of expected) {
        assert.ok(GLOBAL_BUILTINS.has(name), `Should contain "${name}"`);
      }
    });

    it("contains Web/Node.js platform globals", () => {
      const expected = [
        "File",
        "Blob",
        "FormData",
        "URL",
        "URLSearchParams",
        "Buffer",
        "process",
        "setTimeout",
        "setInterval",
        "clearTimeout",
        "clearInterval",
        "AbortController"
      ];
      for (const name of expected) {
        assert.ok(GLOBAL_BUILTINS.has(name), `Should contain "${name}"`);
      }
    });
  });
});
