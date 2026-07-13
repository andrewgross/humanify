import assert from "node:assert";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import * as path from "node:path";
import { pathToFileURL } from "node:url";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { BunModulesManifest } from "../unpack/adapters/bun.js";
import {
  BUN_RELINK_RUNTIME,
  BUN_RELINK_RUNTIME_FILENAME,
  factoryLookup,
  relinkBunModules,
  relinkFactoryReferences,
  wrapExtractedFactory
} from "./bun-relink.js";

function parses(code: string): boolean {
  return !!parseSync(code, { sourceType: "unambiguous", configFile: false });
}

describe("BUN_RELINK_RUNTIME", () => {
  it("exports a memoizing __commonJS matching Bun's Q helper", () => {
    const dir = mkdtempSync(path.join(tmpdir(), "bun-relink-rt-"));
    try {
      writeFileSync(
        path.join(dir, BUN_RELINK_RUNTIME_FILENAME),
        BUN_RELINK_RUNTIME
      );
      const req = createRequire(pathToFileURL(path.join(dir, "_p.js")).href);
      const { __commonJS } = req(`./${BUN_RELINK_RUNTIME_FILENAME}`);
      let ran = 0;
      const mod = __commonJS(
        (_exports: Record<string, unknown>, module: { exports: unknown }) => {
          ran++;
          module.exports = { value: 42 };
        }
      );
      assert.strictEqual(ran, 0, "factory is lazy until first call");
      assert.deepStrictEqual(mod(), { value: 42 });
      assert.strictEqual(mod(), mod(), "memoized: same exports object");
      assert.strictEqual(ran, 1, "factory runs exactly once");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("relinkFactoryReferences", () => {
  const lookup = factoryLookup({
    adapter: "bun",
    factories: [
      {
        fileName: "lib_aaaa.js",
        name: "a",
        nameSource: "fallback",
        structuralHash: "a",
        factoryVar: "x1",
        runtimeIdentifier: "lib_aaaa"
      },
      {
        fileName: "pkg/axios.js",
        name: "axios",
        nameSource: "url",
        structuralHash: "b",
        factoryVar: "x2",
        runtimeIdentifier: "lib_bbbb"
      }
    ]
  });

  it("injects a require header for each free factory reference, with correct relative paths", () => {
    const code = "var x = lib_aaaa();\nvar y = lib_bbbb().default;\n";
    const out = relinkFactoryReferences(code, "core/deep/app.js", lookup);
    assert.match(
      out,
      /const lib_aaaa = require\("\.\.\/\.\.\/lib_aaaa\.js"\);/,
      out
    );
    assert.match(
      out,
      /const lib_bbbb = require\("\.\.\/\.\.\/pkg\/axios\.js"\);/,
      out
    );
    // References stay bare — the module exports the callable directly.
    assert.match(out, /var x = lib_aaaa\(\);/);
    assert.ok(parses(out), out);
  });

  it("computes ./ paths for a factory referenced from the output root", () => {
    const out = relinkFactoryReferences(
      "var z = lib_aaaa();\n",
      "runtime-part.js",
      lookup
    );
    assert.match(out, /const lib_aaaa = require\("\.\/lib_aaaa\.js"\);/, out);
  });

  it("does not inject for a factory id that is locally bound (shadowed)", () => {
    const code = "function f(lib_aaaa) { return lib_aaaa(); }\n";
    const out = relinkFactoryReferences(code, "a.js", lookup);
    assert.doesNotMatch(
      out,
      /require\(/,
      `bound param must not be relinked:\n${out}`
    );
  });

  it("injects each factory only once even with multiple references", () => {
    const code = "lib_aaaa(); lib_aaaa(); lib_aaaa();\n";
    const out = relinkFactoryReferences(code, "a.js", lookup);
    assert.strictEqual(
      (out.match(/require\("\.\/lib_aaaa\.js"\)/g) ?? []).length,
      1,
      out
    );
  });

  it("preserves a leading directive prologue", () => {
    const code = '"use strict";\nvar x = lib_aaaa();\n';
    const out = relinkFactoryReferences(code, "a.js", lookup);
    assert.ok(
      out.startsWith('"use strict";'),
      `directive must stay first:\n${out}`
    );
    assert.match(out, /const lib_aaaa = require/);
  });

  it("returns the code unchanged when no factory is referenced", () => {
    const code = "var x = 1 + 2;\n";
    assert.strictEqual(relinkFactoryReferences(code, "a.js", lookup), code);
  });
});

describe("wrapExtractedFactory", () => {
  const lookup = factoryLookup({
    adapter: "bun",
    factories: [
      {
        fileName: "lib_aaaa.js",
        name: "a",
        nameSource: "fallback",
        structuralHash: "a",
        factoryVar: "x1",
        runtimeIdentifier: "lib_aaaa"
      }
    ]
  });

  it("wraps a factory body as a runnable CJS module exporting the __commonJS thunk", () => {
    const body = "(exports, module) => { module.exports = 42; }";
    const out = wrapExtractedFactory(body, "lib_bbbb.js", lookup);
    assert.match(
      out,
      /const \{ __commonJS \} = require\("\.\/__bun-runtime\.js"\);/,
      out
    );
    assert.match(
      out,
      /module\.exports = __commonJS\(\(exports, module\) => \{ module\.exports = 42; \}\);/,
      out
    );
    assert.ok(parses(out), out);
  });

  it("injects cross-module factory requires referenced inside the body", () => {
    const body = "(exports, module) => { module.exports = lib_aaaa() + 1; }";
    const out = wrapExtractedFactory(body, "sub/lib_cccc.js", lookup);
    assert.match(out, /const lib_aaaa = require\("\.\.\/lib_aaaa\.js"\);/, out);
    assert.match(
      out,
      /const \{ __commonJS \} = require\("\.\.\/__bun-runtime\.js"\);/,
      out
    );
  });
});

describe("relinkBunModules (end to end, executed)", () => {
  it("composes a runnable graph: extracted factories + split files, memoized cross-module", async () => {
    const dir = mkdtempSync(path.join(tmpdir(), "bun-relink-e2e-"));
    try {
      // Two extracted factory bodies (raw arrows, as unpack writes them).
      writeFileSync(
        path.join(dir, "lib_aaaa.js"),
        "(exports, module) => { module.exports = { n: 42 }; }"
      );
      writeFileSync(
        path.join(dir, "lib_bbbb.js"),
        "(exports, module) => { module.exports = lib_aaaa().n + 1; }"
      );
      // A split-tree file (in a folder) referencing both factories.
      mkdirSync(path.join(dir, "core"), { recursive: true });
      writeFileSync(
        path.join(dir, "core", "app.js"),
        "module.exports = { a: lib_aaaa(), b: lib_bbbb() };\n"
      );
      const manifest: BunModulesManifest = {
        adapter: "bun",
        factories: [
          {
            fileName: "lib_aaaa.js",
            name: "a",
            nameSource: "fallback",
            structuralHash: "a",
            factoryVar: "x1",
            runtimeIdentifier: "lib_aaaa"
          },
          {
            fileName: "lib_bbbb.js",
            name: "b",
            nameSource: "fallback",
            structuralHash: "b",
            factoryVar: "x2",
            runtimeIdentifier: "lib_bbbb"
          }
        ]
      };
      writeFileSync(
        path.join(dir, "_bun-modules.json"),
        JSON.stringify(manifest)
      );

      await relinkBunModules(dir, manifest, ["core/app.js"]);

      const req = createRequire(
        pathToFileURL(path.join(dir, "_probe.js")).href
      );
      const app = req("./core/app.js");
      assert.deepStrictEqual(app.a, { n: 42 }, "factory a memoized value");
      assert.strictEqual(
        app.b,
        43,
        "factory b sees a's value via cross-module require"
      );
      // Memoization: a's exports object is identity-stable across callers.
      const aDirect = req("./lib_aaaa.js")();
      assert.strictEqual(
        aDirect,
        app.a,
        "single memoized instance across the graph"
      );
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
