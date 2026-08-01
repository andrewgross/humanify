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

/**
 * First Bun release containing the fix for oven-sh/bun#11100 — Bun's inability
 * to `require` a CommonJS module containing `using`.
 *
 * Established, not guessed: PR #29538 ("Don't lower `using`/`await using` when
 * targeting Bun") merged 2026-04-21T22:49:27Z, one second before the issue
 * closed, and GitHub's compare API puts its commit AHEAD of `bun-v1.3.13` and
 * BEHIND `bun-v1.3.14`. Confirmed by running the case under 1.3.14: it loads.
 */
const BUN_FIXED_USING_IN_CJS = "1.3.14";

/**
 * `true`/`false` for "is `v` at least `min`", or **null when `v` cannot be
 * parsed**, which callers must treat as "do not know" rather than "yes".
 *
 * A version gate that silently reads "fixed" disables the very check it guards,
 * and nothing goes red to say so — the same shape as leaving a determinism aid
 * on for a verdict. So an unreadable version is an error, not a skip.
 *
 * Only the leading dotted-numeric run is compared: `1.3.14+abc123` is 1.3.14,
 * not [1,3,14,123]. Missing components count as 0, so `1.3` < `1.3.14`.
 */
function atLeastVersion(v: string, min: string): boolean | null {
  const parts = (s: string): number[] => {
    const lead = /^\d+(?:\.\d+)*/.exec(s.trim())?.[0];
    return lead ? lead.split(".").map(Number) : [];
  };
  const a = parts(v);
  if (a.length === 0) return null;
  const b = parts(min);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const x = a[i] ?? 0;
    const y = b[i] ?? 0;
    if (x !== y) return x > y;
  }
  return true;
}

/** The installed Bun's version, or null when Bun is not on PATH. */
function bunVersion(): string | null {
  try {
    return execFileSync("bun", ["--version"], { encoding: "utf-8" }).trim();
  } catch {
    return null;
  }
}

describe("atLeastVersion", () => {
  it("compares dotted numeric versions component-wise", () => {
    assert.strictEqual(atLeastVersion("1.3.14", "1.3.14"), true);
    assert.strictEqual(atLeastVersion("1.3.15", "1.3.14"), true);
    assert.strictEqual(atLeastVersion("1.4.0", "1.3.14"), true);
    assert.strictEqual(atLeastVersion("2.0.0", "1.3.14"), true);
    assert.strictEqual(atLeastVersion("1.3.13", "1.3.14"), false);
    assert.strictEqual(atLeastVersion("1.3.9", "1.3.14"), false, "9 < 14");
    assert.strictEqual(atLeastVersion("1.2.99", "1.3.14"), false);
    assert.strictEqual(atLeastVersion("1.3", "1.3.14"), false, "missing = 0");
    assert.strictEqual(atLeastVersion("2", "1.3.14"), true);
  });

  it("ignores a build or prerelease suffix rather than mis-reading it", () => {
    // `1.3.14+abc123` must not parse as [1,3,14,123].
    assert.strictEqual(atLeastVersion("1.3.14+abc123", "1.3.14"), true);
    assert.strictEqual(atLeastVersion("1.3.13-canary.20", "1.3.14"), false);
  });

  it("refuses to answer for an unparseable version", () => {
    // A gate that silently reads "fixed" would skip its canary forever.
    assert.strictEqual(atLeastVersion("", "1.3.14"), null);
    assert.strictEqual(atLeastVersion("unknown", "1.3.14"), null);
  });
});

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
    if (bunVersion() === null) {
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
    const version = bunVersion();
    if (version === null) {
      t.skip("bun not installed");
      return;
    }
    // This is a CANARY: it asserts the upstream bug still EXISTS, so that the
    // runner's translation of Bun's internal TypeError stays justified. Once a
    // Bun that fixes #11100 is installed the tree simply loads, and asserting a
    // failure would be asserting that Bun is still broken.
    const fixed = atLeastVersion(version, BUN_FIXED_USING_IN_CJS);
    assert.notStrictEqual(
      fixed,
      null,
      `could not parse \`bun --version\` output ${JSON.stringify(version)} — ` +
        "refusing to guess, because a gate that guesses 'fixed' skips itself forever"
    );
    if (fixed) {
      t.skip(
        `bun ${version} >= ${BUN_FIXED_USING_IN_CJS} fixes bun#11100 (PR #29538), ` +
          "so the error this translates can no longer occur — the workaround in " +
          "runnable-scaffold.ts is dead code for this runtime and can be deleted " +
          "once no supported Bun predates the fix"
      );
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
