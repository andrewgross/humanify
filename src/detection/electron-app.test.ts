import assert from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { detectElectronApp } from "./electron-app.js";

function writeApp(root: string, files: Record<string, string>): void {
  for (const [rel, content] of Object.entries(files)) {
    const dest = path.join(root, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, content);
  }
}

describe("detectElectronApp", () => {
  let appDir: string;

  beforeEach(() => {
    appDir = fs.mkdtempSync(path.join(os.tmpdir(), "electron-detect-"));
  });

  afterEach(() => {
    fs.rmSync(appDir, { recursive: true, force: true });
  });

  it("returns null when the dir has no resolvable app layout", () => {
    assert.strictEqual(detectElectronApp(appDir), null);
  });

  it("is definitive on an electron dependency marker", () => {
    writeApp(appDir, {
      "package.json": JSON.stringify({
        main: "out/main/index.js",
        devDependencies: { electron: "^30.0.0" }
      }),
      "out/main/index.js": "console.log(1)"
    });
    const detection = detectElectronApp(appDir);
    assert.ok(detection);
    assert.strictEqual(detection.bundler?.type, "electron");
    assert.strictEqual(detection.bundler?.tier, "definitive");
    assert.ok(detection.signals.some((s) => s.source === "electron-app"));
  });

  it("is definitive on an electron-* / @electron/* dependency marker", () => {
    writeApp(appDir, {
      "package.json": JSON.stringify({
        main: "index.js",
        dependencies: { "electron-updater": "^6.8.3" }
      }),
      "index.js": "console.log(1)"
    });
    const detection = detectElectronApp(appDir);
    assert.ok(detection);
    assert.strictEqual(detection.bundler?.tier, "definitive");
  });

  it("is definitive when the entry references the electron module", () => {
    writeApp(appDir, {
      "package.json": JSON.stringify({ main: "index.js" }),
      "index.js": 'var e=require("electron");e.app.whenReady();'
    });
    const detection = detectElectronApp(appDir);
    assert.ok(detection);
    assert.strictEqual(detection.bundler?.type, "electron");
    assert.strictEqual(detection.bundler?.tier, "definitive");
  });

  it("is only likely when the layout resolves without any marker", () => {
    writeApp(appDir, {
      "package.json": JSON.stringify({ main: "index.js" }),
      "index.js": "console.log(1)"
    });
    const detection = detectElectronApp(appDir);
    assert.ok(detection);
    assert.strictEqual(detection.bundler?.type, "electron");
    assert.strictEqual(detection.bundler?.tier, "likely");
  });

  it("detects the minifier from the entry file's code", () => {
    writeApp(appDir, {
      "package.json": JSON.stringify({
        main: "index.js",
        devDependencies: { electron: "^30.0.0" }
      }),
      // swc's snake_case helper is a distinctive minifier signal
      "index.js": "function a(){_class_call_check(this,a)}"
    });
    const detection = detectElectronApp(appDir);
    assert.ok(detection);
    assert.strictEqual(detection.minifier?.type, "swc");
  });
});
