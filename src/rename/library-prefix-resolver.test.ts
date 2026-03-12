import assert from "node:assert";
import { describe, it } from "node:test";
import {
  LibraryPrefixResolver,
  sanitizeLibraryName
} from "./library-prefix-resolver.js";

describe("sanitizeLibraryName", () => {
  it("converts hyphens to underscores", () => {
    assert.strictEqual(sanitizeLibraryName("react-dom"), "react_dom");
  });

  it("strips @ scope prefix and converts slash", () => {
    assert.strictEqual(sanitizeLibraryName("@babel/runtime"), "babel_runtime");
  });

  it("handles scoped packages with hyphens", () => {
    assert.strictEqual(
      sanitizeLibraryName("@emotion/styled-base"),
      "emotion_styled_base"
    );
  });

  it("passes through simple names", () => {
    assert.strictEqual(sanitizeLibraryName("react"), "react");
  });

  it("handles core-js style names", () => {
    assert.strictEqual(sanitizeLibraryName("core-js"), "core_js");
  });

  it("handles names with dots", () => {
    assert.strictEqual(sanitizeLibraryName("lodash.merge"), "lodash_merge");
  });

  it("handles names that start with digits after sanitization", () => {
    // Ensure result is a valid identifier prefix
    const result = sanitizeLibraryName("3d-viewer");
    assert.ok(
      /^[a-z_]/.test(result),
      `Result "${result}" should start with a letter or underscore`
    );
  });

  it("lowercases all characters", () => {
    assert.strictEqual(sanitizeLibraryName("React"), "react");
  });
});

describe("LibraryPrefixResolver", () => {
  it("prefixes all identifiers with the library name", () => {
    const resolver = new LibraryPrefixResolver("react_dom");
    const result = resolver.resolveNames(["Xuo", "Vp3", "qR"]);
    assert.deepStrictEqual(result, {
      Xuo: "react_dom_Xuo",
      Vp3: "react_dom_Vp3",
      qR: "react_dom_qR"
    });
  });

  it("has strategy set to library-prefix", () => {
    const resolver = new LibraryPrefixResolver("lodash");
    assert.strictEqual(resolver.strategy, "library-prefix");
  });

  it("handles empty identifier list", () => {
    const resolver = new LibraryPrefixResolver("react");
    const result = resolver.resolveNames([]);
    assert.deepStrictEqual(result, {});
  });
});
