import assert from "node:assert";
import { beforeEach, describe, it } from "node:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import type { WebcrackFile } from "../plugins/webcrack.js";
import { detectLibraryFromComments } from "./comment-patterns.js";
import {
  detectLibraries,
  extractLibraryNameFromPath,
  isLibraryPath
} from "./detector.js";

describe("isLibraryPath", () => {
  it("detects node_modules paths", () => {
    assert.strictEqual(isLibraryPath("node_modules/react/index.js"), true);
    assert.strictEqual(isLibraryPath("./node_modules/lodash/lodash.js"), true);
    assert.strictEqual(
      isLibraryPath("src/node_modules/internal/util.js"),
      true
    );
  });

  it("detects known runtime libraries", () => {
    assert.strictEqual(
      isLibraryPath("@babel/runtime/helpers/classCallCheck.js"),
      true
    );
    assert.strictEqual(isLibraryPath("core-js/modules/es.array.map.js"), true);
    assert.strictEqual(isLibraryPath("regenerator-runtime/runtime.js"), true);
    assert.strictEqual(isLibraryPath("tslib/tslib.es6.js"), true);
    assert.strictEqual(
      isLibraryPath("webpack/runtime/define-property-getters"),
      true
    );
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
      extractLibraryNameFromPath(
        "node_modules/react-dom/cjs/react-dom.production.min.js"
      ),
      "react-dom"
    );
  });

  it("extracts scoped package names", () => {
    assert.strictEqual(
      extractLibraryNameFromPath(
        "node_modules/@babel/runtime/helpers/classCallCheck.js"
      ),
      "@babel/runtime"
    );
    assert.strictEqual(
      extractLibraryNameFromPath(
        "@babel/runtime/helpers/interopRequireDefault.js"
      ),
      "@babel/runtime"
    );
  });

  it("falls back to first path segment", () => {
    assert.strictEqual(extractLibraryNameFromPath("lodash/map.js"), "lodash");
    assert.strictEqual(
      extractLibraryNameFromPath("core-js/modules/es.array.map.js"),
      "core-js"
    );
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
      detectLibraryFromComments(`${padding}/*! React v18.2.0 */`),
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

describe("detectLibraries — mixed file detection (Layer 3)", () => {
  let tmpDir: string;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "humanify-test-"));
  });

  async function writeFile(name: string, content: string): Promise<string> {
    const filePath = path.join(tmpDir, name);
    await fs.writeFile(filePath, content);
    return filePath;
  }

  it("detects mixed files with interleaved banners", async () => {
    // Banners must be past 1KB so Layer 2 (header scan) doesn't catch them
    const appPadding = `var appCode = ${JSON.stringify("x".repeat(1100))};\n`;
    const mixedCode = [
      appPadding,
      "/*! React v18.2.0 */",
      "function reactInternal() { return 2; }",
      "/*! zustand v4.0.0 */",
      "function zustandStore() { return 3; }"
    ].join("\n");

    const filePath = await writeFile("mixed.js", mixedCode);
    const files: WebcrackFile[] = [{ path: filePath }];
    const result = await detectLibraries(files);

    // File should NOT be in libraryFiles (it has app code too)
    assert.strictEqual(result.libraryFiles.has(filePath), false);
    // File should be in novelFiles (it will be processed)
    assert.ok(result.novelFiles.includes(filePath));
    // File should be in mixedFiles
    assert.ok(result.mixedFiles.has(filePath));

    const mixed = result.mixedFiles.get(filePath);
    assert.ok(mixed != null, "mixed file entry should exist");
    assert.strictEqual(mixed.regions.length, 2);
    assert.deepStrictEqual(mixed.libraryNames.sort(), ["react", "zustand"]);
  });

  it("does not flag files without banners as mixed", async () => {
    const appCode = "function app() { return 1; }";
    const filePath = await writeFile("app.js", appCode);
    const files: WebcrackFile[] = [{ path: filePath }];
    const result = await detectLibraries(files);

    assert.strictEqual(result.mixedFiles.size, 0);
    assert.ok(result.novelFiles.includes(filePath));
  });

  it("Layer 2 takes priority over Layer 3 for single-banner files", async () => {
    // A file with a banner in the first 1KB is classified as a whole library file
    const code = "/*! React v18.2.0 */\nfunction a() { return 1; }";
    const filePath = await writeFile("react.js", code);
    const files: WebcrackFile[] = [{ path: filePath }];
    const result = await detectLibraries(files);

    // Should be detected as a full library file by Layer 2, not as mixed
    assert.ok(result.libraryFiles.has(filePath));
    assert.strictEqual(result.mixedFiles.has(filePath), false);
  });
});
