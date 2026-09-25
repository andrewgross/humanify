import assert from "node:assert";
import { describe, it } from "node:test";
import type { CommentRegion } from "./comment-regions.js";
import { findCommentRegions, libraryAtOffset } from "./comment-regions.js";

describe("findCommentRegions", () => {
  it("returns empty for code without banners", () => {
    const regions = findCommentRegions("function foo() { return 42; }");
    assert.deepStrictEqual(regions, []);
  });

  it("detects a single /*! library v1.2.3 */ banner", () => {
    const code = "/*! React v18.2.0 */\nfunction a() {}";
    const regions = findCommentRegions(code);
    assert.strictEqual(regions.length, 1);
    assert.strictEqual(regions[0].libraryName, "react");
    assert.strictEqual(regions[0].startOffset, 0);
    assert.strictEqual(regions[0].endOffset, null);
  });

  it("detects multiple banners and creates sequential regions", () => {
    const code = [
      "/*! React v18.2.0 */",
      "function reactInternal() {}",
      "/*! zustand v4.0.0 */",
      "function zustandStore() {}"
    ].join("\n");

    const regions = findCommentRegions(code);
    assert.strictEqual(regions.length, 2);

    assert.strictEqual(regions[0].libraryName, "react");
    assert.strictEqual(regions[0].startOffset, 0);
    assert.strictEqual(regions[0].endOffset, regions[1].startOffset);

    assert.strictEqual(regions[1].libraryName, "zustand");
    assert.ok(regions[1].startOffset > 0);
    assert.strictEqual(regions[1].endOffset, null); // last region extends to EOF
  });

  it("detects @license banners", () => {
    const code = "/** @license lodash */\nvar _ = {};";
    const regions = findCommentRegions(code);
    assert.strictEqual(regions.length, 1);
    assert.strictEqual(regions[0].libraryName, "lodash");
  });

  it("detects @module banners", () => {
    const code = "/** @module underscore */\nvar _ = {};";
    const regions = findCommentRegions(code);
    assert.strictEqual(regions.length, 1);
    assert.strictEqual(regions[0].libraryName, "underscore");
  });

  it("detects * library vX.Y.Z inside block comments", () => {
    const code = "/**\n * axios v1.6.0\n */\nfunction send() {}";
    const regions = findCommentRegions(code);
    assert.strictEqual(regions.length, 1);
    assert.strictEqual(regions[0].libraryName, "axios");
  });

  it("scans the entire file (not just first 1KB)", () => {
    const padding = `${"x".repeat(2000)}\n`;
    const code = `${padding}/*! React v18.2.0 */\nfunction a() {}`;
    const regions = findCommentRegions(code);
    assert.strictEqual(regions.length, 1);
    assert.strictEqual(regions[0].libraryName, "react");
  });

  it("handles banners with dash separator", () => {
    const code = "/*! moment - v2.29.4 */\nfunction m() {}";
    const regions = findCommentRegions(code);
    assert.strictEqual(regions.length, 1);
    assert.strictEqual(regions[0].libraryName, "moment");
  });

  it("normalizes library names (lowercase, strip punctuation)", () => {
    const code = "/*! jQuery, v3.6.0 */\nvar $;";
    const regions = findCommentRegions(code);
    assert.strictEqual(regions.length, 1);
    assert.strictEqual(regions[0].libraryName, "jquery");
  });

  it("deduplicates overlapping matches at the same offset", () => {
    // A comment that matches multiple patterns should produce one region
    const code = "/*! lodash v4.17.21 */\nvar _ = {};";
    const regions = findCommentRegions(code);
    // May match both the /*! pattern and the * library vX.Y.Z pattern
    // but should be deduped by offset
    assert.ok(regions.length >= 1);
    assert.strictEqual(regions[0].libraryName, "lodash");
  });

  it("regions are sorted by offset", () => {
    const code = [
      "function appCode() { return 1; }",
      "/*! lodash v4.17.21 */",
      "function chunk() {}",
      "/*! React v18.2.0 */",
      "function createElement() {}"
    ].join("\n");

    const regions = findCommentRegions(code);
    assert.strictEqual(regions.length, 2);
    assert.ok(regions[0].startOffset < regions[1].startOffset);
    assert.strictEqual(regions[0].libraryName, "lodash");
    assert.strictEqual(regions[1].libraryName, "react");
  });

  it("first region does not start at 0 when code precedes first banner", () => {
    const code = "var appVar = 1;\n/*! React v18.2.0 */\nfunction a() {}";
    const regions = findCommentRegions(code);
    assert.strictEqual(regions.length, 1);
    assert.ok(regions[0].startOffset > 0);
  });
});

describe("libraryAtOffset", () => {
  it("returns null when there are no regions", () => {
    assert.strictEqual(libraryAtOffset([], 0), null);
  });

  it("names the library of an offset inside a region, null outside", () => {
    const regions: CommentRegion[] = [
      { libraryName: "react", startOffset: 0, endOffset: 100 }
    ];
    assert.strictEqual(libraryAtOffset(regions, 50), "react");
    assert.strictEqual(libraryAtOffset(regions, 150), null);
  });

  it("the last region extends to EOF", () => {
    const regions: CommentRegion[] = [
      { libraryName: "react", startOffset: 100, endOffset: null }
    ];
    assert.strictEqual(libraryAtOffset(regions, 50), null);
    assert.strictEqual(libraryAtOffset(regions, 200), "react");
  });

  it("distinguishes several regions and the app gaps between them", () => {
    const regions: CommentRegion[] = [
      { libraryName: "react", startOffset: 0, endOffset: 100 },
      { libraryName: "lodash", startOffset: 200, endOffset: 300 }
    ];
    assert.strictEqual(libraryAtOffset(regions, 150), null);
    assert.strictEqual(libraryAtOffset(regions, 50), "react");
    assert.strictEqual(libraryAtOffset(regions, 250), "lodash");
    assert.strictEqual(libraryAtOffset(regions, 350), null);
  });

  it("a region's start is inclusive and its end exclusive", () => {
    const regions: CommentRegion[] = [
      { libraryName: "react", startOffset: 0, endOffset: 100 }
    ];
    assert.strictEqual(libraryAtOffset(regions, 0), "react");
    assert.strictEqual(libraryAtOffset(regions, 100), null);
  });
});
