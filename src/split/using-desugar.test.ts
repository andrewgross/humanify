import assert from "node:assert";
import { execFileSync } from "node:child_process";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync
} from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import { desugarUsing, desugarUsingInTree } from "./using-desugar.js";

const requireCjs = createRequire(import.meta.url);

/** Import a code string as a temp CJS module and return its exports. */
function requireCode(code: string): Record<string, unknown> {
  const dir = mkdtempSync(path.join(tmpdir(), "desugar-req-"));
  const file = path.join(dir, "mod.js");
  try {
    writeFileSync(file, code);
    return requireCjs(file) as Record<string, unknown>;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

describe("desugarUsing", () => {
  it("returns null for code with no using declarations (byte-identity path)", () => {
    assert.strictEqual(
      desugarUsing("var a = 1;\nmodule.exports.a = a;\n"),
      null
    );
  });

  it("does not transform when `using` appears only in comments or strings", () => {
    // A regex prefilter alone would false-positive here and regenerate the
    // file for nothing — churning formatting across the whole tree.
    const code =
      '// we are using the disposable pattern\nvar label = "using x";\nmodule.exports.label = label;\n';
    assert.strictEqual(desugarUsing(code), null);
  });

  it("removes every using declaration and stays CommonJS", () => {
    const code =
      "function f() {\n" +
      "  using a = { [Symbol.dispose]() {} };\n" +
      "  return 1;\n" +
      "}\n" +
      "async function g() {\n" +
      "  await using b = { async [Symbol.asyncDispose]() {} };\n" +
      "  return 2;\n" +
      "}\n" +
      "module.exports = { f, g };\n";
    const out = desugarUsing(code);
    assert.ok(out, "must transform");
    const ast = parseSync(out, {
      sourceType: "unambiguous",
      configFile: false
    });
    assert.ok(ast, `output must parse:\n${out}`);
    let usingCount = 0;
    for (const line of out.split("\n")) {
      if (/^\s*(await\s+)?using\s+[A-Za-z_$]/.test(line)) usingCount++;
    }
    assert.strictEqual(
      usingCount,
      0,
      `no using declarations may remain:\n${out}`
    );
    assert.doesNotMatch(
      out,
      /^import /m,
      "helpers must be inlined, not imported"
    );
  });

  it("preserves disposal semantics: LIFO order, and disposal on throw", () => {
    const code =
      "const order = [];\n" +
      "function run() {\n" +
      '  using a = { [Symbol.dispose]() { order.push("a"); } };\n' +
      '  using b = { [Symbol.dispose]() { order.push("b"); } };\n' +
      '  order.push("body");\n' +
      "}\n" +
      "function boom() {\n" +
      '  using c = { [Symbol.dispose]() { order.push("c"); } };\n' +
      '  throw new Error("boom");\n' +
      "}\n" +
      "module.exports = { run, boom, order };\n";
    const out = desugarUsing(code);
    assert.ok(out);
    const mod = requireCode(out) as {
      run: () => void;
      boom: () => void;
      order: string[];
    };
    mod.run();
    assert.deepStrictEqual(mod.order, ["body", "b", "a"], "LIFO disposal");
    assert.throws(() => mod.boom(), /boom/);
    assert.deepStrictEqual(
      mod.order,
      ["body", "b", "a", "c"],
      "disposal must fire on the throwing path too"
    );
  });

  it("preserves await using semantics (async disposal actually awaited)", async () => {
    const code =
      "const order = [];\n" +
      "async function run() {\n" +
      "  await using a = { async [Symbol.asyncDispose]() {\n" +
      "    await new Promise((r) => setTimeout(r, 5));\n" +
      '    order.push("disposed");\n' +
      "  } };\n" +
      '  order.push("body");\n' +
      "}\n" +
      "module.exports = { run, order };\n";
    const out = desugarUsing(code);
    assert.ok(out);
    const mod = requireCode(out) as {
      run: () => Promise<void>;
      order: string[];
    };
    await mod.run();
    assert.deepStrictEqual(mod.order, ["body", "disposed"]);
  });

  it("is deterministic (same input, same output)", () => {
    const code =
      "function f() {\n  using a = { [Symbol.dispose]() {} };\n  return 1;\n}\nmodule.exports.f = f;\n";
    assert.strictEqual(desugarUsing(code), desugarUsing(code));
  });

  it("makes a CJS-marked using file requireable under Bun (bun#11100 workaround)", (t) => {
    // The exact upstream-bug shape: module.exports + `using` in a required
    // .js file dies in Bun's CJS transpiler. After desugaring there is no
    // `using` left, so Bun loads it.
    try {
      execFileSync("bun", ["--version"], { encoding: "utf-8" });
    } catch {
      t.skip("bun not installed");
      return;
    }
    const code =
      "let disposed = false;\n" +
      "function f() {\n" +
      "  using x = { [Symbol.dispose]() { disposed = true; } };\n" +
      "  return 7;\n" +
      "}\n" +
      "const ok = f();\n" +
      "module.exports = { ok, disposed: () => disposed };\n";
    const out = desugarUsing(code);
    assert.ok(out);
    const dir = mkdtempSync(path.join(tmpdir(), "desugar-bun-"));
    try {
      writeFileSync(path.join(dir, "mod.js"), out);
      writeFileSync(
        path.join(dir, "main.cjs"),
        'const m = require("./mod.js");\n' +
          "console.log(JSON.stringify({ ok: m.ok, disposed: m.disposed() }));\n"
      );
      const res = execFileSync("bun", [path.join(dir, "main.cjs")], {
        encoding: "utf-8"
      });
      assert.match(res, /"ok":7/, res);
      assert.match(res, /"disposed":true/, res);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("desugarUsingInTree", () => {
  it("transforms only using files, skips metadata and node_modules, reports count", async () => {
    const dir = mkdtempSync(path.join(tmpdir(), "desugar-tree-"));
    try {
      mkdirSync(path.join(dir, "src"), { recursive: true });
      mkdirSync(path.join(dir, ".humanify"), { recursive: true });
      mkdirSync(path.join(dir, "node_modules", "dep"), { recursive: true });
      const usingCode =
        "function f() {\n  using a = { [Symbol.dispose]() {} };\n}\nf();\nmodule.exports.done = 1;\n";
      const plainCode = "module.exports.x = 1;\n";
      writeFileSync(path.join(dir, "src", "has-using.js"), usingCode);
      writeFileSync(path.join(dir, "src", "plain.js"), plainCode);
      // Same content in protected locations — must NOT be touched.
      writeFileSync(path.join(dir, ".humanify", "humanified.js"), usingCode);
      writeFileSync(
        path.join(dir, "node_modules", "dep", "index.js"),
        usingCode
      );

      const count = await desugarUsingInTree(dir);
      assert.strictEqual(count, 1, "exactly the one src using-file");
      const transformed = readFileSync(
        path.join(dir, "src", "has-using.js"),
        "utf-8"
      );
      assert.doesNotMatch(transformed, /^\s*using\s/m, transformed);
      assert.strictEqual(
        readFileSync(path.join(dir, "src", "plain.js"), "utf-8"),
        plainCode,
        "no-using file stays byte-identical"
      );
      assert.strictEqual(
        readFileSync(path.join(dir, ".humanify", "humanified.js"), "utf-8"),
        usingCode,
        "metadata (the prior-version target) must never be desugared"
      );
      assert.strictEqual(
        readFileSync(
          path.join(dir, "node_modules", "dep", "index.js"),
          "utf-8"
        ),
        usingCode,
        "node_modules is external"
      );
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
