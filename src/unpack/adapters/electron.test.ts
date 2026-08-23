import assert from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import { type ElectronAppManifest, ElectronUnpackAdapter } from "./electron.js";

function writeApp(root: string, files: Record<string, string>): void {
  for (const [rel, content] of Object.entries(files)) {
    const dest = path.join(root, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, content);
  }
}

/** A minimal extracted electron-vite app: out/ app code, vendored deps. */
function writeZcodeShapedApp(appDir: string): void {
  writeApp(appDir, {
    "package.json": JSON.stringify({
      name: "fake-app",
      version: "1.0.0",
      main: "out/main/index.js",
      devDependencies: { electron: "^30.0.0" }
    }),
    "out/main/index.js": "console.log('main')",
    "out/preload/index.cjs": "console.log('preload')",
    "out/renderer/chunk-abc123.js": "console.log('chunk')",
    "out/renderer/style.css": "body{}",
    "scripts/build.js": "console.log('not app code')",
    "node_modules/react/package.json": JSON.stringify({
      name: "react",
      version: "19.2.4"
    }),
    "node_modules/react/index.js": "module.exports={}",
    "node_modules/@scope/util/package.json": JSON.stringify({
      name: "@scope/util",
      version: "2.0.0"
    }),
    "node_modules/@scope/util/index.js": "module.exports={}"
  });
}

describe("ElectronUnpackAdapter", () => {
  const adapter = new ElectronUnpackAdapter();
  let appDir: string;
  let outDir: string;

  beforeEach(() => {
    appDir = fs.mkdtempSync(path.join(os.tmpdir(), "electron-unpack-app-"));
    outDir = fs.mkdtempSync(path.join(os.tmpdir(), "electron-unpack-out-"));
  });

  afterEach(() => {
    fs.rmSync(appDir, { recursive: true, force: true });
    fs.rmSync(outDir, { recursive: true, force: true });
  });

  it("supports electron detection and nothing else", () => {
    assert.strictEqual(
      adapter.supports({
        bundler: { type: "electron", tier: "definitive" },
        signals: []
      }),
      true
    );
    assert.strictEqual(
      adapter.supports({
        bundler: { type: "bun", tier: "definitive" },
        signals: []
      }),
      false
    );
  });

  it("throws on a file input with a directory hint", async () => {
    await assert.rejects(
      adapter.unpack({ kind: "file", code: "console.log(1)" }, outDir),
      /directory/
    );
  });

  it("copies the app's JS under the code root, preserving layout", async () => {
    writeZcodeShapedApp(appDir);
    const result = await adapter.unpack(
      { kind: "directory", path: appDir },
      outDir
    );
    const rels = result.files.map((f) => path.relative(outDir, f.path)).sort();
    assert.deepStrictEqual(rels, [
      path.join("out", "main", "index.js"),
      path.join("out", "preload", "index.cjs"),
      path.join("out", "renderer", "chunk-abc123.js")
    ]);
    assert.strictEqual(
      fs.readFileSync(path.join(outDir, "out/main/index.js"), "utf-8"),
      "console.log('main')"
    );
  });

  it("copies nothing outside the code root: no css, node_modules, scripts", async () => {
    writeZcodeShapedApp(appDir);
    await adapter.unpack({ kind: "directory", path: appDir }, outDir);
    assert.ok(!fs.existsSync(path.join(outDir, "out/renderer/style.css")));
    assert.ok(!fs.existsSync(path.join(outDir, "node_modules")));
    assert.ok(!fs.existsSync(path.join(outDir, "scripts")));
  });

  it("handles a root-level main (code root '.') and still skips node_modules", async () => {
    writeApp(appDir, {
      "package.json": JSON.stringify({ name: "flat", main: "index.js" }),
      "index.js": "console.log(1)",
      "lib.js": "console.log(2)",
      "node_modules/x/package.json": JSON.stringify({ name: "x" }),
      "node_modules/x/index.js": "module.exports={}"
    });
    const result = await adapter.unpack(
      { kind: "directory", path: appDir },
      outDir
    );
    const rels = result.files.map((f) => path.relative(outDir, f.path)).sort();
    assert.deepStrictEqual(rels, ["index.js", "lib.js"]);
    assert.ok(!fs.existsSync(path.join(outDir, "node_modules")));
  });

  it("writes an app manifest with identity and the vendored package census", async () => {
    writeZcodeShapedApp(appDir);
    await adapter.unpack({ kind: "directory", path: appDir }, outDir);
    const manifest = JSON.parse(
      fs.readFileSync(
        path.join(outDir, ".humanify", "electron-app.json"),
        "utf-8"
      )
    ) as ElectronAppManifest;
    assert.strictEqual(manifest.adapter, "electron");
    assert.strictEqual(manifest.app.name, "fake-app");
    assert.strictEqual(manifest.app.version, "1.0.0");
    assert.strictEqual(manifest.app.main, "out/main/index.js");
    assert.strictEqual(manifest.codeRoot, "out");
    assert.strictEqual(manifest.fileCount, 3);
    assert.deepStrictEqual(manifest.vendored, [
      { name: "@scope/util", version: "2.0.0" },
      { name: "react", version: "19.2.4" }
    ]);
  });

  it("returns files in a deterministic sorted order", async () => {
    writeZcodeShapedApp(appDir);
    const result = await adapter.unpack(
      { kind: "directory", path: appDir },
      outDir
    );
    const rels = result.files.map((f) => path.relative(outDir, f.path));
    assert.deepStrictEqual(rels, [...rels].sort());
  });

  it("refuses an output dir inside the app dir", async () => {
    writeZcodeShapedApp(appDir);
    await assert.rejects(
      adapter.unpack(
        { kind: "directory", path: appDir },
        path.join(appDir, "out")
      ),
      /outside/
    );
  });

  it("throws loudly when the app has no resolvable layout", async () => {
    await assert.rejects(
      adapter.unpack({ kind: "directory", path: appDir }, outDir),
      /package\.json/
    );
  });

  it("throws loudly when the code root holds no JS", async () => {
    // A resolvable but non-JS main gives a layout whose code root has no
    // JS files at all — a run that would process nothing must say so.
    writeApp(appDir, {
      "package.json": JSON.stringify({ main: "app/main.bin" }),
      "app/main.bin": "binary-ish"
    });
    await assert.rejects(
      adapter.unpack({ kind: "directory", path: appDir }, outDir),
      /no JS/
    );
  });
});
