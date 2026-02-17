import { describe, it, mock, beforeEach } from "node:test";
import assert from "node:assert";
import { isLibraryPath, extractLibraryNameFromPath } from "./detector.js";
import { detectLibraryFromComments } from "./comment-patterns.js";

describe("isLibraryPath", () => {
  it("detects node_modules paths", () => {
    assert.strictEqual(isLibraryPath("node_modules/react/index.js"), true);
    assert.strictEqual(isLibraryPath("./node_modules/lodash/lodash.js"), true);
    assert.strictEqual(isLibraryPath("src/node_modules/internal/util.js"), true);
  });

  it("detects known runtime libraries", () => {
    assert.strictEqual(isLibraryPath("@babel/runtime/helpers/classCallCheck.js"), true);
    assert.strictEqual(isLibraryPath("core-js/modules/es.array.map.js"), true);
    assert.strictEqual(isLibraryPath("regenerator-runtime/runtime.js"), true);
    assert.strictEqual(isLibraryPath("tslib/tslib.es6.js"), true);
    assert.strictEqual(isLibraryPath("webpack/runtime/define-property-getters"), true);
  });

  it("does not flag application code", () => {
    assert.strictEqual(isLibraryPath("src/utils/helpers.js"), false);
    assert.strictEqual(isLibraryPath("app/components/Button.js"), false);
    assert.strictEqual(isLibraryPath("index.js"), false);
    assert.strictEqual(isLibraryPath("0.js"), false);
  });
});

describe("extractLibraryNameFromPath", () => {
  it("extracts package name from node_modules path", () => {
    assert.strictEqual(
      extractLibraryNameFromPath("node_modules/react/index.js"),
      "react"
    );
    assert.strictEqual(
      extractLibraryNameFromPath("node_modules/react-dom/cjs/react-dom.production.min.js"),
      "react-dom"
    );
  });

  it("extracts scoped package names", () => {
    assert.strictEqual(
      extractLibraryNameFromPath("node_modules/@babel/runtime/helpers/classCallCheck.js"),
      "@babel/runtime"
    );
    assert.strictEqual(
      extractLibraryNameFromPath("@babel/runtime/helpers/interopRequireDefault.js"),
      "@babel/runtime"
    );
  });

  it("falls back to first path segment", () => {
    assert.strictEqual(extractLibraryNameFromPath("lodash/map.js"), "lodash");
    assert.strictEqual(extractLibraryNameFromPath("core-js/modules/es.array.map.js"), "core-js");
  });
});

describe("detectLibraryFromComments", () => {
  it("detects /*! library v1.2.3 */ banners", () => {
    assert.strictEqual(
      detectLibraryFromComments("/*! React v18.2.0 */\nvar React = ..."),
      "react"
    );
    assert.strictEqual(
      detectLibraryFromComments("/*! lodash v4.17.21 */"),
      "lodash"
    );
  });

  it("detects /*! library - v1.2.3 */ banners", () => {
    assert.strictEqual(
      detectLibraryFromComments("/*! moment - v2.29.4 */"),
      "moment"
    );
  });

  it("detects @license banners", () => {
    assert.strictEqual(
      detectLibraryFromComments("/** @license React */"),
      "react"
    );
    assert.strictEqual(
      detectLibraryFromComments("/* @license Redux */\nvar store = ..."),
      "redux"
    );
  });

  it("detects @module banners", () => {
    assert.strictEqual(
      detectLibraryFromComments("/** @module lodash */"),
      "lodash"
    );
  });

  it("detects * library vX.Y.Z inside block comments", () => {
    assert.strictEqual(
      detectLibraryFromComments("/**\n * axios v1.6.0\n */"),
      "axios"
    );
  });

  it("returns undefined for code without banners", () => {
    assert.strictEqual(
      detectLibraryFromComments("function foo() { return 42; }"),
      undefined
    );
    assert.strictEqual(
      detectLibraryFromComments("var x = 1; var y = 2;"),
      undefined
    );
  });

  it("only scans first 1KB", () => {
    const padding = "x".repeat(2000);
    assert.strictEqual(
      detectLibraryFromComments(padding + "/*! React v18.2.0 */"),
      undefined
    );
  });

  it("strips trailing punctuation from library names", () => {
    assert.strictEqual(
      detectLibraryFromComments("/*! jQuery, v3.6.0 */"),
      "jquery"
    );
  });
});
