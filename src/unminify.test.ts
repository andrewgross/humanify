import assert from "node:assert";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, it } from "node:test";
import type { PipelineConfig } from "./pipeline/types.js";
import { unminify } from "./unminify.js";

function writeApp(root: string, files: Record<string, string>): void {
  for (const [rel, content] of Object.entries(files)) {
    const dest = path.join(root, rel);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, content);
  }
}

/** A plugin that stamps each file so processing is observable. */
async function stampPlugin(code: string): Promise<string> {
  return `/*processed*/${code}`;
}

describe("unminify input kinds", () => {
  let workDir: string;
  let outDir: string;

  beforeEach(() => {
    workDir = fs.mkdtempSync(path.join(os.tmpdir(), "unminify-in-"));
    outDir = fs.mkdtempSync(path.join(os.tmpdir(), "unminify-out-"));
  });

  afterEach(() => {
    fs.rmSync(workDir, { recursive: true, force: true });
    fs.rmSync(outDir, { recursive: true, force: true });
  });

  it("processes a directory input through the electron adapter", async () => {
    const config: PipelineConfig = {
      bundlerType: "electron",
      bundlerTier: "definitive",
      minifierType: "unknown",
      unpackAdapterName: "electron"
    };
    writeApp(workDir, {
      "package.json": JSON.stringify({
        name: "fake-app",
        version: "1.0.0",
        main: "out/main/index.js",
        devDependencies: { electron: "^30.0.0" }
      }),
      "out/main/index.js": "console.log('main')",
      "out/renderer/chunk-abc123.js": "console.log('chunk')",
      "node_modules/react/package.json": JSON.stringify({ name: "react" }),
      "node_modules/react/index.js": "module.exports={}"
    });

    await unminify(
      { kind: "directory", path: workDir },
      outDir,
      config,
      [stampPlugin],
      { log: () => {} }
    );

    assert.strictEqual(
      fs.readFileSync(path.join(outDir, "out/main/index.js"), "utf-8"),
      "/*processed*/console.log('main')"
    );
    assert.strictEqual(
      fs.readFileSync(
        path.join(outDir, "out/renderer/chunk-abc123.js"),
        "utf-8"
      ),
      "/*processed*/console.log('chunk')"
    );
    assert.ok(!fs.existsSync(path.join(outDir, "node_modules")));
  });

  it("still processes a single-file input through passthrough", async () => {
    const config: PipelineConfig = {
      bundlerType: "unknown",
      bundlerTier: "unknown",
      minifierType: "unknown",
      unpackAdapterName: "passthrough"
    };

    await unminify(
      { kind: "file", code: "console.log('one')" },
      outDir,
      config,
      [stampPlugin],
      { log: () => {} }
    );

    assert.strictEqual(
      fs.readFileSync(path.join(outDir, "index.js"), "utf-8"),
      "/*processed*/console.log('one')"
    );
  });
});
