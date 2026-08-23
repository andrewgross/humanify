import assert from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { resolveElectronAppLayout } from "./electron-layout.js";

/** Write an app dir from a {relPath: content} map; dirs are created. */
function writeApp(root: string, files: Record<string, string>): void {
  for (const [rel, content] of Object.entries(files)) {
    const dest = path.join(root, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, content);
  }
}

function pkg(json: Record<string, unknown>): string {
  return JSON.stringify(json);
}

describe("resolveElectronAppLayout", () => {
  let appDir: string;

  beforeEach(() => {
    appDir = fs.mkdtempSync(path.join(os.tmpdir(), "electron-layout-"));
  });

  afterEach(() => {
    fs.rmSync(appDir, { recursive: true, force: true });
  });

  it("returns null when package.json is missing", () => {
    assert.strictEqual(resolveElectronAppLayout(appDir), null);
  });

  it("returns null when package.json is unparseable", () => {
    writeApp(appDir, { "package.json": "{not json" });
    assert.strictEqual(resolveElectronAppLayout(appDir), null);
  });

  it("resolves an explicit nested main entry", () => {
    writeApp(appDir, {
      "package.json": pkg({
        name: "@zcode/desktop",
        version: "3.8.1",
        main: "out/main/index.js"
      }),
      "out/main/index.js": "console.log(1)"
    });
    const layout = resolveElectronAppLayout(appDir);
    assert.ok(layout);
    assert.strictEqual(layout.name, "@zcode/desktop");
    assert.strictEqual(layout.version, "3.8.1");
    assert.strictEqual(layout.mainRel, path.join("out", "main", "index.js"));
    assert.strictEqual(layout.codeRootRel, "out");
    assert.strictEqual(layout.codeRootAbs, path.join(appDir, "out"));
  });

  it("defaults main to index.js when the field is absent", () => {
    writeApp(appDir, {
      "package.json": pkg({ name: "app" }),
      "index.js": "console.log(1)"
    });
    const layout = resolveElectronAppLayout(appDir);
    assert.ok(layout);
    assert.strictEqual(layout.mainRel, "index.js");
    assert.strictEqual(layout.codeRootRel, ".");
    assert.strictEqual(layout.codeRootAbs, appDir);
  });

  it("resolves a main entry without its extension (node-style)", () => {
    writeApp(appDir, {
      "package.json": pkg({ main: "out/main/index" }),
      "out/main/index.js": "x"
    });
    const layout = resolveElectronAppLayout(appDir);
    assert.ok(layout);
    assert.strictEqual(layout.mainRel, path.join("out", "main", "index.js"));
  });

  it("resolves a main entry pointing at a directory (node-style)", () => {
    writeApp(appDir, {
      "package.json": pkg({ main: "out/main" }),
      "out/main/index.js": "x"
    });
    const layout = resolveElectronAppLayout(appDir);
    assert.ok(layout);
    assert.strictEqual(layout.mainRel, path.join("out", "main", "index.js"));
    assert.strictEqual(layout.codeRootRel, "out");
  });

  it("normalizes a ./-prefixed main", () => {
    writeApp(appDir, {
      "package.json": pkg({ main: "./out/main.js" }),
      "out/main.js": "x"
    });
    const layout = resolveElectronAppLayout(appDir);
    assert.ok(layout);
    assert.strictEqual(layout.mainRel, path.join("out", "main.js"));
    assert.strictEqual(layout.codeRootRel, "out");
  });

  it("returns null when main does not resolve to a file", () => {
    writeApp(appDir, {
      "package.json": pkg({ main: "out/missing.js" })
    });
    assert.strictEqual(resolveElectronAppLayout(appDir), null);
  });

  it("returns null when main escapes the app dir", () => {
    writeApp(appDir, {
      "package.json": pkg({ main: "../evil.js" })
    });
    fs.writeFileSync(path.join(appDir, "..", "evil.js"), "x");
    try {
      assert.strictEqual(resolveElectronAppLayout(appDir), null);
    } finally {
      fs.rmSync(path.join(appDir, "..", "evil.js"), { force: true });
    }
  });

  it("merges dependencies and devDependencies into dependencyNames", () => {
    writeApp(appDir, {
      "package.json": pkg({
        main: "index.js",
        dependencies: { "electron-updater": "^6.0.0", react: "^19.0.0" },
        devDependencies: { electron: "^30.0.0" }
      }),
      "index.js": "x"
    });
    const layout = resolveElectronAppLayout(appDir);
    assert.ok(layout);
    assert.deepStrictEqual([...layout.dependencyNames].sort(), [
      "electron",
      "electron-updater",
      "react"
    ]);
  });
});
