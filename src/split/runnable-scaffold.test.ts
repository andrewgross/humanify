import assert from "node:assert";
import { execFileSync } from "node:child_process";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync
} from "node:fs";
import { tmpdir } from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import {
  detectExternalPackages,
  externalPackagesFrom,
  RUNNER_FILENAME,
  writeRunnableScaffold
} from "./runnable-scaffold.js";

describe("externalPackagesFrom", () => {
  it("collects bare external packages, excluding builtins and relative requires", () => {
    const files = [
      'const a = require("ws");\nconst b = require("node:fs");\n',
      'var c = require("./lib_x.js"), d = require("path");\n',
      'require("ajv/dist/runtime/equal");\nrequire("@scope/pkg/sub/deep");\n',
      'require("fs/promises");\nrequire("../up.js");\n'
    ];
    const ext = externalPackagesFrom(files);
    // ws + ajv + @scope/pkg; NOT node:fs, path, fs/promises, or relatives.
    assert.deepStrictEqual(ext, ["@scope/pkg", "ajv", "ws"]);
  });

  it("reduces subpath imports to the installable package name", () => {
    const ext = externalPackagesFrom([
      'require("ajv/dist/runtime/uri");require("ajv-formats/dist/formats");require("ajv");'
    ]);
    assert.deepStrictEqual(ext, ["ajv", "ajv-formats"]);
  });

  it("returns nothing when only builtins and relatives are required", () => {
    const ext = externalPackagesFrom([
      'require("crypto");require("./a.js");require("node:path");'
    ]);
    assert.deepStrictEqual(ext, []);
  });

  it("excludes runtime builtins under any scheme (bun:, node:, data:)", () => {
    const ext = externalPackagesFrom([
      'require("bun:jsc");require("bun:ffi");require("node:fs");require("ws");'
    ]);
    assert.deepStrictEqual(ext, ["ws"]);
  });
});

describe("writeRunnableScaffold + detectExternalPackages (executed)", () => {
  it("scans the tree, emits a runner, and the runner boots the entry", async () => {
    const dir = mkdtempSync(path.join(tmpdir(), "scaffold-"));
    try {
      // A tiny runnable tree: entry + a factory-style file that requires an
      // external, and one that uses `using` (to exercise the strip shim).
      writeFileSync(path.join(dir, "index.js"), 'require("./core/app.js");\n');
      mkdirSync(path.join(dir, "core"), { recursive: true });
      writeFileSync(
        path.join(dir, "core", "app.js"),
        'const ext = require("leftpad-ish");\n' +
          "function f() { using x = { [Symbol.dispose]() {} }; return ext.ok; }\n" +
          "console.log(JSON.stringify({ started: true, ext: f() }));\n"
      );

      const externals = await detectExternalPackages(dir);
      assert.deepStrictEqual(externals, ["leftpad-ish"]);

      await writeRunnableScaffold(dir, "index.js", externals);

      // The scaffold's package.json declares the detected external.
      const pkg = JSON.parse(
        readFileSync(path.join(dir, "package.json"), "utf-8")
      );
      assert.deepStrictEqual(pkg.dependencies, { "leftpad-ish": "*" });
      assert.ok(
        readFileSync(path.join(dir, "RUNNABLE.md"), "utf-8").includes(
          "npm install"
        )
      );

      // Provide the "external" so the runner can boot.
      mkdirSync(path.join(dir, "node_modules", "leftpad-ish"), {
        recursive: true
      });
      writeFileSync(
        path.join(dir, "node_modules", "leftpad-ish", "package.json"),
        '{"name":"leftpad-ish","main":"index.js"}'
      );
      writeFileSync(
        path.join(dir, "node_modules", "leftpad-ish", "index.js"),
        "module.exports = { ok: 7 };\n"
      );

      // The runner must boot the entry despite the unsupported `using` syntax.
      const out = execFileSync("node", [path.join(dir, RUNNER_FILENAME)], {
        encoding: "utf-8"
      });
      assert.match(out, /"started":true/, out);
      assert.match(out, /"ext":7/, out);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
