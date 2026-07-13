import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import { emitRunnableCjs, tryEmitRunnableCjs } from "./cjs-emit.js";
import type { StableSplitLedger } from "./stable-split.js";

/** Filler declarations so fixtures clear the 50-binding wrapper detection
 * threshold shared with stable-split (WRAPPER_IIFE_BINDING_THRESHOLD). */
const PAD_FILE = "pad/fill.js";
const PADDING = Array.from(
  { length: 60 },
  (_, i) => `var padFiller${i} = ${i};`
);

type Stmt = [file: string, src: string];

function bodyOf(stmts: Stmt[]): { body: string[]; order: string[] } {
  return {
    body: [...stmts.map(([, s]) => s), ...PADDING],
    order: [...stmts.map(([f]) => f), ...PADDING.map(() => PAD_FILE)]
  };
}

function ledgerOf(order: string[]): StableSplitLedger {
  return {
    version: 1,
    files: [...new Set(order)],
    nameToFiles: {},
    order
  };
}

/** One wrapper-bundle fixture: each entry is ONE wrapper-body statement
 * (may be multi-line) assigned to a ledger file. */
function bundle(stmts: Stmt[]): { code: string; ledger: StableSplitLedger } {
  const { body, order } = bodyOf(stmts);
  const code = [
    "(function (exports, require, module, __filename, __dirname) {",
    ...body.map((s) =>
      s
        .split("\n")
        .map((l) => `  ${l}`)
        .join("\n")
    ),
    "});"
  ].join("\n");
  return { code, ledger: ledgerOf(order) };
}

describe("emitRunnableCjs input contract", () => {
  it("throws a descriptive error on non-wrapper input", () => {
    assert.throws(
      () =>
        emitRunnableCjs("var a = 1;\nvar b = 2;", ledgerOf(["a.js", "a.js"])),
      /wrapper/
    );
  });

  it("tryEmitRunnableCjs reports the decline reason and returns null", () => {
    const reasons: string[] = [];
    const result = tryEmitRunnableCjs(
      "var a = 1;\nvar b = 2;",
      ledgerOf(["a.js", "a.js"]),
      (r) => reasons.push(r)
    );
    assert.strictEqual(result, null);
    assert.strictEqual(reasons.length, 1);
    assert.match(reasons[0], /wrapper/);
  });

  it("accepts the wrapper shapes stable-split accepts", () => {
    const { body, order } = bodyOf([
      ["core/a.js", "var shared = 1;"],
      ["core/b.js", "function readIt() { return shared; }"]
    ]);
    const inner = body.map((s) => `  ${s}`).join("\n");
    const shapes = [
      `((exports, require, module, __filename, __dirname) => {\n${inner}\n})();`,
      `!function (exports, require, module, __filename, __dirname) {\n${inner}\n}();`,
      `(function (exports, require, module, __filename, __dirname) {\n${inner}\n}).call(this);`
    ];
    for (const code of shapes) {
      const files = emitRunnableCjs(code, ledgerOf(order));
      const b = files.get("core/b.js") ?? "";
      assert.match(
        b,
        /__core_a_js\.shared/,
        `cross read rewritten in:\n${code.slice(0, 60)}`
      );
    }
  });

  it("throws when ledger.order disagrees with the statement count", () => {
    const { code, ledger } = bundle([
      ["a.js", "var one = 1;"],
      ["b.js", "var two = one + 1;"]
    ]);
    const short = { ...ledger, order: ledger.order.slice(0, -1) };
    assert.throws(() => emitRunnableCjs(code, short), /statement/);
  });

  it("throws when ledger.order references a file missing from ledger.files", () => {
    const { code, ledger } = bundle([
      ["a.js", "var one = 1;"],
      ["b.js", "var two = one + 1;"]
    ]);
    const rogue = {
      ...ledger,
      files: ledger.files.filter((f) => f !== "b.js")
    };
    assert.throws(() => emitRunnableCjs(code, rogue), /files/);
  });
});

describe("emitRunnableCjs reference rewriting", () => {
  it("exports cross-file bindings as live accessors and rewrites reads and writes", () => {
    const { code, ledger } = bundle([
      ["core/a.js", "function sharedHelper(x) {\n  return x + 1;\n}"],
      ["core/a.js", "var counter = 0;"],
      [
        "core/b.js",
        "function useHelper(y) {\n  counter = counter + 1;\n  return sharedHelper(y);\n}"
      ]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const a = files.get("core/a.js") ?? "";
    const b = files.get("core/b.js") ?? "";
    assert.match(
      a,
      /Object\.defineProperty\(module\.exports,\s*"sharedHelper",\s*\{\s*get:/,
      `sharedHelper accessor:\n${a}`
    );
    assert.match(a, /"counter",[^}]*set:\s*v\s*=>/, `counter setter:\n${a}`);
    assert.doesNotMatch(
      a,
      /"sharedHelper",[^}]*set:/,
      "read-only binding gets no setter"
    );
    assert.match(
      b,
      /const __core_a_js = require\("\.\.\/core\/a\.js"\)|const __core_a_js = require\("\.\/a\.js"\)/,
      `require header:\n${b}`
    );
    assert.match(
      b,
      /__core_a_js\.counter = __core_a_js\.counter \+ 1;/,
      `write + read rewritten:\n${b}`
    );
  });

  it("rewrites cross-file writes through destructuring assignment targets", () => {
    const { code, ledger } = bundle([
      ["core/a.js", "var counter = 0;"],
      ["core/a.js", "var cursor = 0;"],
      ["core/a.js", "var items = [1, 2];"],
      ["core/b.js", "function writeArr() { [counter] = items; }"],
      ["core/b.js", "function writeObj() { ({ cursor } = { cursor: 5 }); }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const a = files.get("core/a.js") ?? "";
    const b = files.get("core/b.js") ?? "";
    assert.match(
      b,
      /\[__core_a_js\.counter\] = __core_a_js\.items;/,
      `array target:\n${b}`
    );
    assert.match(
      b,
      /\{\s*cursor: __core_a_js\.cursor\s*\}\s*=/,
      `object shorthand target:\n${b}`
    );
    assert.match(
      a,
      /"counter",[^}]*set:/,
      "array-destructured binding needs a setter"
    );
    assert.match(
      a,
      /"cursor",[^}]*set:/,
      "object-destructured binding needs a setter"
    );
  });

  it("emits statements containing a top-level return without crashing", () => {
    const { code, ledger } = bundle([
      ["a.js", "var flag = 1;"],
      ["a.js", "var setup = 2;"],
      ["b.js", "if (flag) return setup;"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const b = files.get("b.js") ?? "";
    assert.match(
      b,
      /if \(__a_js\.flag\) return __a_js\.setup;/,
      `top-level return:\n${b}`
    );
  });

  it("preserves callee `this` with a (0, ns.fn) indirection", () => {
    const { code, ledger } = bundle([
      ["a.js", "function probe() { return this; }"],
      ["b.js", "function callProbe() { return probe(); }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const b = files.get("b.js") ?? "";
    assert.match(
      b,
      /return \(0, __a_js\.probe\)\(\);/,
      `callee indirection:\n${b}`
    );
  });

  it("replaces delete-on-binding with `false` (sloppy var-delete semantics)", () => {
    const { code, ledger } = bundle([
      ["a.js", "var counter = 1;"],
      ["b.js", "function drop() { return delete counter; }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const b = files.get("b.js") ?? "";
    assert.match(b, /return false;/, `delete neutralized:\n${b}`);
    assert.doesNotMatch(
      b,
      /delete __a_js\.counter/,
      "must not delete the accessor"
    );
  });

  it("rewrites object-shorthand reads with an explicit key", () => {
    const { code, ledger } = bundle([
      ["a.js", "var counter = 1;"],
      ["b.js", "function pack() { return { counter }; }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const b = files.get("b.js") ?? "";
    assert.match(
      b,
      /\{\s*counter: __a_js\.counter\s*\}/,
      `shorthand read:\n${b}`
    );
  });

  it("does not rewrite a shadowing local of the same name", () => {
    const { code, ledger } = bundle([
      ["core/a.js", "var counter = 0;"],
      ["core/b.js", "function bump() { counter = counter + 1; }"],
      [
        "core/b.js",
        "function localOnly(z) {\n  var counter = z;\n  return counter * 2;\n}"
      ]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const b = files.get("core/b.js") ?? "";
    assert.match(b, /var counter = z;/, `local decl untouched:\n${b}`);
    assert.match(b, /return counter \* 2;/, `local read untouched:\n${b}`);
  });
});

describe("emitRunnableCjs cross-file var redeclaration", () => {
  it("turns a cross-file var redeclaration into a namespace assignment", () => {
    const { code, ledger } = bundle([
      ["a.js", "var cfg = 1;"],
      ["b.js", "var cfg = 2;"],
      ["b.js", "function getCfg() { return cfg; }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const a = files.get("a.js") ?? "";
    const b = files.get("b.js") ?? "";
    assert.match(b, /__a_js\.cfg = 2;/, `redecl becomes assignment:\n${b}`);
    assert.doesNotMatch(b, /var cfg/, "no dead local left behind");
    assert.match(b, /return __a_js\.cfg;/, `reads go through namespace:\n${b}`);
    assert.match(a, /"cfg",[^}]*set:/, "redeclared binding needs a setter");
  });

  it("splits mixed declarator statements preserving order", () => {
    const { code, ledger } = bundle([
      ["a.js", "var cfg = 1;"],
      ["b.js", "var before = 0, cfg = before + 2, after = cfg + 3;"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const b = files.get("b.js") ?? "";
    const beforeIdx = b.indexOf("var before = 0;");
    const crossIdx = b.indexOf("__a_js.cfg = before + 2;");
    const afterIdx = b.indexOf("var after = __a_js.cfg + 3;");
    assert.ok(beforeIdx >= 0, `local kept:\n${b}`);
    assert.ok(crossIdx > beforeIdx, `cross assignment after local:\n${b}`);
    assert.ok(afterIdx > crossIdx, `trailing local after cross:\n${b}`);
  });

  it("rewrites for-init and for-of var redeclarations in place", () => {
    const { code, ledger } = bundle([
      ["a.js", "var i = 99;"],
      ["a.js", "var item = null;"],
      ["a.js", "var total = 0;"],
      ["b.js", "for (var i = 0; i < 3; i++) { total = total + 1; }"],
      ["b.js", "for (var item of [1, 2]) { total = total + 1; }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const b = files.get("b.js") ?? "";
    assert.match(
      b,
      /for \(__a_js\.i = 0; __a_js\.i < 3; __a_js\.i\+\+\)/,
      `for-init redecl:\n${b}`
    );
    assert.match(b, /for \(__a_js\.item of \[1, 2\]\)/, `for-of redecl:\n${b}`);
  });

  it("throws on cross-file function redeclaration (hoisting unpreservable)", () => {
    const { code, ledger } = bundle([
      ["a.js", "function dup() { return 1; }"],
      ["b.js", "function dup() { return 2; }"],
      ["b.js", "var use = dup();"]
    ]);
    assert.throws(() => emitRunnableCjs(code, ledger), /redeclaration/);
  });

  it("throws on a redeclaration through a destructuring declarator", () => {
    const { code, ledger } = bundle([
      ["a.js", "var cfg = 1;"],
      ["a.js", "var box = { cfg: 2 };"],
      ["b.js", "var { cfg } = box;"]
    ]);
    assert.throws(() => emitRunnableCjs(code, ledger), /destructuring/);
  });
});

describe("emitRunnableCjs output shape", () => {
  it("emits parseable files covering every ledger file", () => {
    const { code, ledger } = bundle([
      ["core/a.js", "var counter = 0;"],
      ["core/b.js", "function bump() { counter = counter + 1; }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    for (const f of ledger.files) {
      assert.ok(files.has(f), `missing ${f}`);
    }
    for (const [file, content] of files) {
      assert.ok(
        parseSync(content, { sourceType: "unambiguous", configFile: false }),
        `${file} must parse:\n${content}`
      );
    }
  });

  it("keeps namespace vars distinct when sanitized paths collide", () => {
    const { code, ledger } = bundle([
      ["core/a-x.js", "var alpha = 1;"],
      ["core/a_x.js", "var beta = 2;"],
      ["core/b.js", "function readBoth() { return alpha + beta; }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const b = files.get("core/b.js") ?? "";
    const consts = [...b.matchAll(/const (__\S+) = require/g)].map((m) => m[1]);
    assert.strictEqual(consts.length, 2, `two requires:\n${b}`);
    assert.notStrictEqual(
      consts[0],
      consts[1],
      `distinct namespace vars:\n${b}`
    );
  });
});
