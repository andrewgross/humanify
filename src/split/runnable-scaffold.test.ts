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
  resolveExternalVersions,
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

describe("resolveExternalVersions", () => {
  it("pins to the version installed nearest fromDir, scoped packages included", () => {
    const root = mkdtempSync(path.join(tmpdir(), "resolve-"));
    try {
      const nm = path.join(root, "node_modules");
      mkdirSync(path.join(nm, "foo"), { recursive: true });
      writeFileSync(
        path.join(nm, "foo", "package.json"),
        '{"name":"foo","version":"1.2.3"}'
      );
      mkdirSync(path.join(nm, "@sc", "bar"), { recursive: true });
      writeFileSync(
        path.join(nm, "@sc", "bar", "package.json"),
        '{"name":"@sc/bar","version":"4.5.6"}'
      );
      const deps = resolveExternalVersions(["foo", "@sc/bar", "missing"], root);
      assert.deepStrictEqual(deps, {
        foo: "1.2.3",
        "@sc/bar": "4.5.6",
        missing: "*"
      });
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("walks up parent directories to find node_modules (nearest wins)", () => {
    const root = mkdtempSync(path.join(tmpdir(), "resolve-up-"));
    try {
      mkdirSync(path.join(root, "node_modules", "foo"), { recursive: true });
      writeFileSync(
        path.join(root, "node_modules", "foo", "package.json"),
        '{"version":"9.9.9"}'
      );
      const deep = path.join(root, "dist", "nested");
      mkdirSync(deep, { recursive: true });
      assert.deepStrictEqual(resolveExternalVersions(["foo"], deep), {
        foo: "9.9.9"
      });
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("returns '*' for everything when fromDir is undefined", () => {
    assert.deepStrictEqual(resolveExternalVersions(["a", "b"], undefined), {
      a: "*",
      b: "*"
    });
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

  it("boots natively under Bun without tripping the `using` guard", async (t) => {
    // #3b of issue-runnable-trees-dont-run: Bun's file loader parses
    // `using` natively, but its eval/new Function REJECTS it — so
    // usingParses() false-negatives under Bun, and the runner re-execed
    // with a V8 flag Bun doesn't have and refused to run. The guard must
    // short-circuit under Bun (and never install the _compile strip hook,
    // which breaks Bun's CJS loader).
    try {
      execFileSync("bun", ["--version"], { encoding: "utf-8" });
    } catch {
      t.skip("bun not installed");
      return;
    }
    const dir = mkdtempSync(path.join(tmpdir(), "scaffold-bun-"));
    try {
      writeFileSync(path.join(dir, "index.js"), 'require("./core/app.js");\n');
      mkdirSync(path.join(dir, "core"), { recursive: true });
      writeFileSync(
        path.join(dir, "core", "app.js"),
        'let disposed = "no";\n' +
          "function f() {\n" +
          '  using x = { [Symbol.dispose]() { disposed = "yes"; } };\n' +
          "  return 7;\n" +
          "}\n" +
          "const ok = f();\n" +
          "console.log(JSON.stringify({ started: true, ext: ok, disposed }));\n"
      );
      await writeRunnableScaffold(dir, "index.js", []);
      const out = execFileSync("bun", [path.join(dir, RUNNER_FILENAME)], {
        encoding: "utf-8"
      });
      assert.match(out, /"started":true/, out);
      assert.match(out, /"disposed":"yes"/, out);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("explains Bun's CJS `using` bug actionably instead of the raw internal TypeError", async (t) => {
    // oven-sh/bun#11100: Bun cannot require a CommonJS module (any file
    // with module.exports/require/a directive) that contains `using` — the
    // transpiler injects ESM `bun:wrap` imports into the CJS wrapper and
    // the loader dies with "Expected CommonJS module to have a function
    // wrapper". Every real CC tree carries `using`+CJS files, so under Bun
    // the runner must convert that internal error into an actionable one
    // pointing at Node >= 24 / the upstream issue.
    try {
      execFileSync("bun", ["--version"], { encoding: "utf-8" });
    } catch {
      t.skip("bun not installed");
      return;
    }
    const dir = mkdtempSync(path.join(tmpdir(), "scaffold-bun-cjs-"));
    try {
      writeFileSync(path.join(dir, "index.js"), 'require("./core/app.js");\n');
      mkdirSync(path.join(dir, "core"), { recursive: true });
      writeFileSync(
        path.join(dir, "core", "app.js"),
        "function f() {\n" +
          "  using x = { [Symbol.dispose]() {} };\n" +
          "}\n" +
          "f();\n" +
          "module.exports.ok = 1;\n"
      );
      await writeRunnableScaffold(dir, "index.js", []);
      let out = "";
      let status = 0;
      try {
        out = execFileSync("bun", [path.join(dir, RUNNER_FILENAME)], {
          encoding: "utf-8",
          stdio: ["ignore", "pipe", "pipe"]
        });
      } catch (err) {
        const e = err as { status?: number; stdout?: string; stderr?: string };
        status = e.status ?? -1;
        out = `${e.stdout ?? ""}${e.stderr ?? ""}`;
      }
      assert.notStrictEqual(
        status,
        0,
        `must fail while bun#11100 stands:\n${out}`
      );
      assert.match(
        out,
        /bun#11100|oven-sh\/bun/i,
        `names the upstream issue:\n${out}`
      );
      assert.match(out, /Node >= 24/, `points at the working runtime:\n${out}`);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("pins detected versions in package.json when resolvable", async () => {
    const root = mkdtempSync(path.join(tmpdir(), "scaffold-pin-"));
    try {
      mkdirSync(path.join(root, "node_modules", "ws"), { recursive: true });
      writeFileSync(
        path.join(root, "node_modules", "ws", "package.json"),
        '{"version":"8.17.1"}'
      );
      const out = path.join(root, "out");
      mkdirSync(out);
      // resolveFromDir = root; "ws" resolves to its installed version, the
      // unresolvable package stays at "*".
      await writeRunnableScaffold(out, "index.js", ["ws", "nope"], root);
      const pkg = JSON.parse(
        readFileSync(path.join(out, "package.json"), "utf-8")
      );
      assert.deepStrictEqual(pkg.dependencies, { ws: "8.17.1", nope: "*" });
    } finally {
      rmSync(root, { recursive: true, force: true });
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
