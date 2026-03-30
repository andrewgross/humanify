import assert from "node:assert";
import { beforeEach, describe, it } from "node:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import type { BundlerAdapter } from "../../detection/types.js";
import type { WebcrackFile } from "../../plugins/webcrack.js";
import {
  DefaultLibraryDetector,
  extractLibraryNameFromPath,
  isLibraryPath
} from "./default.js";

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

describe("DefaultLibraryDetector", () => {
  const detector = new DefaultLibraryDetector();
  let tmpDir: string;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "humanify-test-"));
  });

  async function writeFile(name: string, content: string): Promise<string> {
    const filePath = path.join(tmpDir, name);
    await fs.writeFile(filePath, content);
    return filePath;
  }

  it("supports any adapter (fallback)", () => {
    assert.strictEqual(
      detector.supports({ name: "webcrack" } as BundlerAdapter),
      true
    );
    assert.strictEqual(
      detector.supports({ name: "bun" } as BundlerAdapter),
      true
    );
  });

  it("detects library from header banner", async () => {
    const code = "/*! React v18.2.0 */\nfunction a() { return 1; }";
    const filePath = await writeFile("react.js", code);
    const files: WebcrackFile[] = [{ path: filePath }];

    const result = await detector.detectLibraries(files);

    assert.ok(result.libraryFiles.has(filePath));
    assert.strictEqual(result.libraryFiles.get(filePath)?.libraryName, "react");
    assert.strictEqual(
      result.libraryFiles.get(filePath)?.detectedBy,
      "comment"
    );
  });

  it("detects mixed files with interleaved banners", async () => {
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
    const result = await detector.detectLibraries(files);

    assert.strictEqual(result.libraryFiles.has(filePath), false);
    assert.ok(result.novelFiles.includes(filePath));
    assert.ok(result.mixedFiles.has(filePath));

    const mixed = result.mixedFiles.get(filePath);
    assert.ok(mixed != null);
    assert.strictEqual(mixed.regions.length, 2);
    assert.deepStrictEqual(mixed.libraryNames.sort(), ["react", "zustand"]);
  });

  it("does not flag files without banners as mixed", async () => {
    const appCode = "function app() { return 1; }";
    const filePath = await writeFile("app.js", appCode);
    const files: WebcrackFile[] = [{ path: filePath }];
    const result = await detector.detectLibraries(files);

    assert.strictEqual(result.mixedFiles.size, 0);
    assert.ok(result.novelFiles.includes(filePath));
  });

  it("Layer 2 takes priority over Layer 3 for single-banner files", async () => {
    const code = "/*! React v18.2.0 */\nfunction a() { return 1; }";
    const filePath = await writeFile("react.js", code);
    const files: WebcrackFile[] = [{ path: filePath }];
    const result = await detector.detectLibraries(files);

    assert.ok(result.libraryFiles.has(filePath));
    assert.strictEqual(result.mixedFiles.has(filePath), false);
  });
});
