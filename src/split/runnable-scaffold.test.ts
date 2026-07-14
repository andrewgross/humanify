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
  it("boots the entry AND faithfully disposes `using` resources", async () => {
    const dir = mkdtempSync(path.join(tmpdir(), "scaffold-"));
    try {
      // A tiny runnable tree: entry + a factory-style file that requires an
      // external and acquires a `using` resource whose Symbol.dispose records
      // that it ran. The runner must run `using` faithfully (real disposal),
      // not rewrite it to `const` (which would silently leak the resource).
      writeFileSync(path.join(dir, "index.js"), 'require("./core/app.js");\n');
      mkdirSync(path.join(dir, "core"), { recursive: true });
      writeFileSync(
        path.join(dir, "core", "app.js"),
        'const ext = require("leftpad-ish");\n' +
          'let disposed = "no";\n' +
          "function f() {\n" +
          '  using x = { [Symbol.dispose]() { disposed = "yes"; } };\n' +
          "  return ext.ok;\n" +
          "}\n" +
          "const ok = f();\n" +
          "console.log(JSON.stringify({ started: true, ext: ok, disposed }));\n"
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

      // The runner boots the entry — even on a Node that needs the V8
      // explicit-resource-management flag — and disposal actually fires.
      const out = execFileSync("node", [path.join(dir, RUNNER_FILENAME)], {
        encoding: "utf-8"
      });
      assert.match(out, /"started":true/, out);
      assert.match(out, /"ext":7/, out);
      assert.match(out, /"disposed":"yes"/, out);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("emits an honest `using` fallback, not a silent strip", async () => {
    const dir = mkdtempSync(path.join(tmpdir(), "scaffold-runner-"));
    try {
      await writeRunnableScaffold(dir, "index.js", []);
      const runner = readFileSync(path.join(dir, RUNNER_FILENAME), "utf-8");
      // Prefers native execution via the V8 flag (faithful disposal)…
      assert.ok(runner.includes("--js-explicit-resource-management"), runner);
      // …falls back to a loud error (process.exit(1))…
      assert.ok(/process\.exit\(1\)/.test(runner), runner);
      // …and the lossy strip is gated behind the HUMANIFY_STRIP_USING opt-in,
      // never unconditional: the guard precedes the _compile override.
      const guardIdx = runner.indexOf('HUMANIFY_STRIP_USING === "1"');
      const stripIdx = runner.indexOf("Module.prototype._compile = function");
      assert.ok(guardIdx !== -1, runner);
      assert.ok(stripIdx !== -1 && guardIdx < stripIdx, runner);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
