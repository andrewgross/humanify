import assert from "node:assert";
import { beforeEach, describe, it } from "node:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import type { PipelineConfig } from "../../pipeline/types.js";
import type { WebcrackFile } from "../../plugins/webcrack.js";
import { BunLibraryDetector } from "./bun.js";

describe("BunLibraryDetector", () => {
  const detector = new BunLibraryDetector();
  let tmpDir: string;

  beforeEach(async () => {
    tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "humanify-bun-lib-"));
  });

  async function writeFile(name: string, content: string): Promise<string> {
    const filePath = path.join(tmpDir, name);
    await fs.writeFile(filePath, content);
    return filePath;
  }

  it("supports bun adapter", () => {
    assert.strictEqual(
      detector.supports({ unpackAdapterName: "bun" } as PipelineConfig),
      true
    );
  });

  it("does not support non-bun adapters", () => {
    assert.strictEqual(
      detector.supports({ unpackAdapterName: "webcrack" } as PipelineConfig),
      false
    );
    assert.strictEqual(
      detector.supports({
        unpackAdapterName: "passthrough"
      } as PipelineConfig),
      false
    );
  });

  it("detects library when banner is in first 1KB", async () => {
    const code = "/*! React v18.2.0 */\nfunction a() { return 1; }";
    const filePath = await writeFile("react.js", code);
    const files: WebcrackFile[] = [{ path: filePath }];

    const result = await detector.detectLibraries(files);

    assert.ok(result.libraryFiles.has(filePath));
    assert.strictEqual(result.libraryFiles.get(filePath)?.libraryName, "react");
    assert.strictEqual(result.novelFiles.length, 0);
  });

  it("detects library when banner is deep in file (past 1KB)", async () => {
    // This is the key difference from the default detector:
    // Bun factories are single modules, so a banner ANYWHERE means library
    const padding = "x".repeat(2000);
    const code = `${padding}\n/*! lodash v4.17.21 */\nfunction chunk() {}`;
    const filePath = await writeFile("lodash.js", code);
    const files: WebcrackFile[] = [{ path: filePath }];

    const result = await detector.detectLibraries(files);

    assert.ok(result.libraryFiles.has(filePath));
    assert.strictEqual(
      result.libraryFiles.get(filePath)?.libraryName,
      "lodash"
    );
  });

  it("does not detect library for files without banners", async () => {
    const code = "function app() { return 1; }";
    const filePath = await writeFile("app.js", code);
    const files: WebcrackFile[] = [{ path: filePath }];

    const result = await detector.detectLibraries(files);

    assert.strictEqual(result.libraryFiles.size, 0);
    assert.ok(result.novelFiles.includes(filePath));
  });

  it("never produces mixed files (each Bun factory is one module)", async () => {
    // Even with multiple banners, the whole file is library
    const code = [
      "/*! React v18.2.0 */",
      "function reactInternal() { return 2; }",
      "/*! zustand v4.0.0 */",
      "function zustandStore() { return 3; }"
    ].join("\n");
    const filePath = await writeFile("multi.js", code);
    const files: WebcrackFile[] = [{ path: filePath }];

    const result = await detector.detectLibraries(files);

    assert.strictEqual(result.mixedFiles.size, 0);
    assert.ok(result.libraryFiles.has(filePath));
  });

  it("detects @license banners", async () => {
    const padding = "x".repeat(2000);
    const code = `${padding}\n/** @license MIT lodash */\nvar _ = {};`;
    const filePath = await writeFile("lodash.js", code);
    const files: WebcrackFile[] = [{ path: filePath }];

    const result = await detector.detectLibraries(files);

    assert.ok(result.libraryFiles.has(filePath));
  });

  it("handles multiple files", async () => {
    const libCode = "/*! React v18.2.0 */\nfunction a() {}";
    const appCode = "function app() { return 1; }";
    const libPath = await writeFile("react.js", libCode);
    const appPath = await writeFile("app.js", appCode);
    const files: WebcrackFile[] = [{ path: libPath }, { path: appPath }];

    const result = await detector.detectLibraries(files);

    assert.ok(result.libraryFiles.has(libPath));
    assert.strictEqual(result.libraryFiles.has(appPath), false);
    assert.ok(result.novelFiles.includes(appPath));
  });

  it("uses the bun-modules manifest to classify factory files as library", async () => {
    // The unpack adapter's real shape: factory files + the manifest live
    // in vendor/ (fileName entries are output-root-relative), the leftover
    // runtime at the root. Only factories should land in libraryFiles.
    // None of them carry banners, so without the manifest the banner-scan
    // fallback would miss them.
    await fs.mkdir(path.join(tmpDir, "vendor"), { recursive: true });
    const axiosPath = await writeFile("vendor/axios.js", "function noop(){}");
    const libPath = await writeFile(
      "vendor/lib_abcdef12.js",
      "function noop2(){}"
    );
    const runtimePath = await writeFile("runtime.js", "main()");
    await writeFile(
      "vendor/_bun-modules.json",
      JSON.stringify(
        {
          adapter: "bun",
          runtimeFile: "runtime.js",
          factories: [
            {
              fileName: "vendor/axios.js",
              name: "axios",
              nameSource: "url",
              structuralHash: "0".repeat(16),
              factoryVar: "Q9k"
            },
            {
              fileName: "vendor/lib_abcdef12.js",
              name: "lib_abcdef12",
              nameSource: "fallback",
              structuralHash: `abcdef12${"0".repeat(8)}`,
              factoryVar: "Zx8"
            }
          ]
        },
        null,
        2
      )
    );

    const files: WebcrackFile[] = [
      { path: axiosPath },
      { path: libPath },
      { path: runtimePath }
    ];
    const result = await detector.detectLibraries(files);

    assert.strictEqual(result.libraryFiles.size, 2);
    assert.strictEqual(
      result.libraryFiles.get(axiosPath)?.libraryName,
      "axios"
    );
    assert.strictEqual(
      result.libraryFiles.get(libPath)?.libraryName,
      "lib_abcdef12"
    );
    assert.deepStrictEqual(result.novelFiles, [runtimePath]);
    assert.strictEqual(result.mixedFiles.size, 0);
  });

  it("classifies a vendor factory named 'runtime' as library, not the app", async () => {
    // Vendor names come from the LLM, so a package can legitimately be named
    // "runtime" -> vendor/runtime.js. Matching manifest entries by BASENAME
    // made that file compare equal to the root runtime.js (the app), so the
    // factory was handed to the rename pipeline as app code. Only the file at
    // the output root is the app.
    await fs.mkdir(path.join(tmpDir, "vendor"), { recursive: true });
    const vendorRuntime = await writeFile(
      "vendor/runtime.js",
      "function v(){}"
    );
    const appRuntime = await writeFile("runtime.js", "main()");
    await writeFile(
      "vendor/_bun-modules.json",
      JSON.stringify({
        adapter: "bun",
        runtimeFile: "runtime.js",
        factories: [
          {
            fileName: "vendor/runtime.js",
            name: "runtime",
            nameSource: "llm",
            structuralHash: "1".repeat(16),
            factoryVar: "Ab1"
          }
        ]
      })
    );

    const files: WebcrackFile[] = [
      { path: vendorRuntime },
      { path: appRuntime }
    ];
    const result = await detector.detectLibraries(files);

    assert.ok(
      result.libraryFiles.has(vendorRuntime),
      "vendor/runtime.js must be a library"
    );
    assert.strictEqual(
      result.libraryFiles.get(vendorRuntime)?.libraryName,
      "runtime"
    );
    // The app is the ONLY novel file.
    assert.deepStrictEqual(result.novelFiles, [appRuntime]);
  });

  it("distinguishes factories that share a basename across package folders", async () => {
    // Package folders (vendor/@scope/pkg/index.js) make basenames
    // non-unique, so a basename-keyed lookup is ambiguous: both files
    // resolve to whichever entry was inserted last.
    await fs.mkdir(path.join(tmpDir, "vendor/@scope/pkg"), { recursive: true });
    const flat = await writeFile("vendor/index.js", "function a(){}");
    const nested = await writeFile(
      "vendor/@scope/pkg/index.js",
      "function b(){}"
    );
    const appRuntime = await writeFile("runtime.js", "main()");
    await writeFile(
      "vendor/_bun-modules.json",
      JSON.stringify({
        adapter: "bun",
        runtimeFile: "runtime.js",
        factories: [
          {
            fileName: "vendor/index.js",
            name: "flat-lib",
            nameSource: "llm",
            structuralHash: "2".repeat(16),
            factoryVar: "Cd2"
          },
          {
            fileName: "vendor/@scope/pkg/index.js",
            name: "@scope/pkg",
            nameSource: "banner",
            structuralHash: "3".repeat(16),
            factoryVar: "Ef3"
          }
        ]
      })
    );

    const files: WebcrackFile[] = [
      { path: flat },
      { path: nested },
      { path: appRuntime }
    ];
    const result = await detector.detectLibraries(files);

    assert.strictEqual(result.libraryFiles.size, 2);
    assert.strictEqual(result.libraryFiles.get(flat)?.libraryName, "flat-lib");
    assert.strictEqual(
      result.libraryFiles.get(nested)?.libraryName,
      "@scope/pkg"
    );
    assert.deepStrictEqual(result.novelFiles, [appRuntime]);
  });

  it("finds the manifest when the first file sits in a package folder", async () => {
    // loadManifest resolved the manifest from dirname(files[0]), which only
    // works when the first factory is flat in vendor/. A nested first file
    // silently fell back to the banner scan -> unbannered factories leak
    // into the app pipeline.
    await fs.mkdir(path.join(tmpDir, "vendor/@scope/pkg"), { recursive: true });
    const nested = await writeFile(
      "vendor/@scope/pkg/index.js",
      "function noBanner(){}"
    );
    const appRuntime = await writeFile("runtime.js", "main()");
    await writeFile(
      "vendor/_bun-modules.json",
      JSON.stringify({
        adapter: "bun",
        runtimeFile: "runtime.js",
        factories: [
          {
            fileName: "vendor/@scope/pkg/index.js",
            name: "@scope/pkg",
            nameSource: "banner",
            structuralHash: "4".repeat(16),
            factoryVar: "Gh4"
          }
        ]
      })
    );

    const files: WebcrackFile[] = [{ path: nested }, { path: appRuntime }];
    const result = await detector.detectLibraries(files);

    assert.ok(
      result.libraryFiles.has(nested),
      "nested factory must be found via the manifest"
    );
    assert.deepStrictEqual(result.novelFiles, [appRuntime]);
  });
});
