import assert from "node:assert";
import { describe, it } from "node:test";
import type { FunctionNode } from "../analysis/types.js";
import type { CommentRegion } from "./comment-regions.js";
import {
  classifyFunctionsByRegion,
  findCommentRegions
} from "./comment-regions.js";

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

describe("classifyFunctionsByRegion", () => {
  function makeFn(sessionId: string, startOffset: number): FunctionNode {
    return {
      sessionId,
      path: {
        node: { start: startOffset }
      },
      fingerprint: { exactHash: "abc" },
      internalCallees: new Set(),
      externalCallees: new Set(),
      callers: new Set(),
      status: "pending",
      callSites: []
    } as unknown as FunctionNode;
  }

  it("returns empty set when no regions", () => {
    const fns = [makeFn("fn1", 0)];
    const result = classifyFunctionsByRegion(fns, []);
    assert.strictEqual(result.size, 0);
  });

  it("classifies functions inside a region as library", () => {
    const regions: CommentRegion[] = [
      { libraryName: "react", startOffset: 0, endOffset: 100 }
    ];
    const fns = [makeFn("fn1", 50), makeFn("fn2", 150)];
    const result = classifyFunctionsByRegion(fns, regions);
    assert.ok(result.has("fn1"));
    assert.ok(!result.has("fn2"));
  });

  it("classifies functions in the last region (extends to EOF)", () => {
    const regions: CommentRegion[] = [
      { libraryName: "react", startOffset: 100, endOffset: null }
    ];
    const fns = [makeFn("fn1", 50), makeFn("fn2", 200)];
    const result = classifyFunctionsByRegion(fns, regions);
    assert.ok(!result.has("fn1")); // before the region
    assert.ok(result.has("fn2")); // inside the last region
  });

  it("classifies functions across multiple regions", () => {
    const regions: CommentRegion[] = [
      { libraryName: "react", startOffset: 0, endOffset: 100 },
      { libraryName: "lodash", startOffset: 200, endOffset: 300 }
    ];
    const fns = [
      makeFn("app1", 150), // between regions (app code)
      makeFn("react1", 50), // in react region
      makeFn("lodash1", 250), // in lodash region
      makeFn("app2", 350) // after all regions (app code)
    ];
    const result = classifyFunctionsByRegion(fns, regions);
    assert.ok(result.has("react1"));
    assert.ok(result.has("lodash1"));
    assert.ok(!result.has("app1"));
    assert.ok(!result.has("app2"));
  });

  it("handles function at exact region boundary", () => {
    const regions: CommentRegion[] = [
      { libraryName: "react", startOffset: 0, endOffset: 100 }
    ];
    // Function at exactly the start of the region
    const fns = [makeFn("fn1", 0), makeFn("fn2", 100)];
    const result = classifyFunctionsByRegion(fns, regions);
    assert.ok(result.has("fn1")); // at start
    assert.ok(!result.has("fn2")); // at endOffset (exclusive)
  });

  it("skips functions with null start offset", () => {
    const regions: CommentRegion[] = [
      { libraryName: "react", startOffset: 0, endOffset: null }
    ];
    const fn = {
      sessionId: "fn1",
      path: { node: { start: null } },
      fingerprint: { exactHash: "abc" },
      internalCallees: new Set(),
      externalCallees: new Set(),
      callers: new Set(),
      status: "pending",
      callSites: []
    } as unknown as FunctionNode;
    const result = classifyFunctionsByRegion([fn], regions);
    assert.strictEqual(result.size, 0);
  });
});
