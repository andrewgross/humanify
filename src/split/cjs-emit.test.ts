import assert from "node:assert";
import { describe, it } from "node:test";
import { parseSync } from "@babel/core";
import type { WrapperFunctionResult } from "../analysis/wrapper-detection.js";
import { findWrapperFunction } from "../analysis/wrapper-detection.js";
import { parseFileAst, traverse } from "../babel-utils.js";
import { emitRunnableCjs, tryEmitRunnableCjs } from "./cjs-emit.js";
import type { StableSplitLedger } from "./stable-split.js";

/** Capture the first FunctionExpression's path+scope as a wrapper — used to
 * hand emitRunnableCjs a wrapper for code below findWrapperFunction's binding
 * threshold, proving the supplied wrapper is honored (not re-detected). */
function captureWrapper(code: string): WrapperFunctionResult {
  const ast = parseFileAst(code);
  assert.ok(ast);
  let wrapper: WrapperFunctionResult | null = null;
  traverse(ast, {
    FunctionExpression(p) {
      wrapper ??= { functionPath: p, scope: p.scope };
      p.stop();
    }
  });
  assert.ok(wrapper);
  return wrapper;
}

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
 * (may be multi-line) assigned to a ledger file. Directives (e.g.
 * '"use strict";') sit in the wrapper prologue, outside the ledger. */
function bundle(
  stmts: Stmt[],
  opts: { directives?: string[] } = {}
): { code: string; ledger: StableSplitLedger } {
  const { body, order } = bodyOf(stmts);
  const code = [
    "(function (exports, require, module, __filename, __dirname) {",
    ...(opts.directives ?? []).map((d) => `  ${d}`),
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

  it("honors a supplied wrapper, skipping re-detection (works below threshold)", () => {
    // A wrapper IIFE with too few bindings for findWrapperFunction to accept.
    const code = [
      "(function (exports, require, module, __filename, __dirname) {",
      "  function one() { return 1; }",
      "  var two = one() + 1;",
      "});"
    ].join("\n");
    const ledger = ledgerOf(["a/one.js", "a/two.js"]);
    // Internal detection declines (below WRAPPER_IIFE_BINDING_THRESHOLD)…
    assert.throws(() => emitRunnableCjs(code, ledger), /wrapper/);
    // …but a manually captured wrapper is honored: emit succeeds, proving the
    // supplied wrapper is consumed and the second parse/detect is skipped.
    const out = emitRunnableCjs(code, ledger, captureWrapper(code));
    assert.ok(
      out.has("a/one.js") && out.has("a/two.js"),
      [...out.keys()].join()
    );
  });

  it("emits identical output whether it parses itself or gets the wrapper", () => {
    const { code, ledger } = bundle([
      ["a/one.js", "function one() { return 1; }"],
      ["a/two.js", "var two = one() + 1;"]
    ]);
    const internal = emitRunnableCjs(code, ledger);
    const ast = parseFileAst(code);
    assert.ok(ast);
    const wrapper = findWrapperFunction(ast);
    assert.ok(wrapper);
    const reused = emitRunnableCjs(code, ledger, wrapper);
    assert.deepStrictEqual([...reused.entries()], [...internal.entries()]);
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

describe("emitRunnableCjs directives", () => {
  it("propagates wrapper directives to every emitted file", () => {
    const { code, ledger } = bundle(
      [
        ["core/a.js", "var counter = 0;"],
        ["core/b.js", "function bump() { counter = counter + 1; }"]
      ],
      { directives: ['"use strict";'] }
    );
    const files = emitRunnableCjs(code, ledger);
    for (const [file, content] of files) {
      assert.ok(
        content.startsWith('"use strict";'),
        `${file} must start with the wrapper directive:\n${content}`
      );
    }
  });

  it("keeps an inert mid-body string statement from becoming a directive", () => {
    const { code, ledger } = bundle([
      ["a.js", "var x0 = 1;"],
      ["b.js", '"use strict";'],
      ["b.js", "function localOnly() { return 1; }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const b = files.get("b.js") ?? "";
    assert.ok(
      b.startsWith('("use strict");'),
      `mid-body string must be parenthesized, not a prologue:\n${b}`
    );
    assert.ok(
      parseSync(b, { sourceType: "unambiguous", configFile: false }),
      `b.js must still parse:\n${b}`
    );
  });
});

describe("emitRunnableCjs shared bundle context", () => {
  it("routes the wrapper's module context through _bundle.js", () => {
    const { code, ledger } = bundle([
      ["a.js", "var api = { v: 1 };"],
      ["b.js", "module.exports = api;"],
      ["b.js", 'var p = __dirname + "/x";'],
      ["b.js", "var t0 = this;"],
      ["b.js", "exports.ready = 1;"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const b = files.get("b.js") ?? "";
    assert.match(
      b,
      /__bundle\.module\.exports = __a_js\.api;/,
      `module routed:\n${b}`
    );
    assert.match(
      b,
      /var p = __bundle\.dirname \+ "\/x";/,
      `__dirname routed:\n${b}`
    );
    assert.match(b, /var t0 = __bundle\.thisArg;/, `this routed:\n${b}`);
    assert.match(b, /__bundle\.exports\.ready = 1;/, `exports routed:\n${b}`);
    assert.match(
      b,
      /const __bundle = require\("\.\/_bundle\.js"\);/,
      `bundle required:\n${b}`
    );
    const runtime = files.get("_bundle.js") ?? "";
    assert.match(runtime, /init\(/, `runtime emitted:\n${runtime}`);
  });

  it("emits no _bundle.js when the wrapper context is unused", () => {
    const { code, ledger } = bundle([
      ["a.js", "var counter = 0;"],
      ["b.js", "function bump() { counter = counter + 1; }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    assert.ok(!files.has("_bundle.js"), "no context use, no runtime module");
  });

  it("rewrites only wrapper-level `this` (arrows yes, functions no)", () => {
    const { code, ledger } = bundle([
      ["a.js", "function probe() { return this; }"],
      ["b.js", "var getThis = () => this;"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const a = files.get("a.js") ?? "";
    const b = files.get("b.js") ?? "";
    assert.match(a, /return this;/, `function this untouched:\n${a}`);
    assert.match(
      b,
      /var getThis = \(\) => __bundle\.thisArg;/,
      `top-level arrow this routed:\n${b}`
    );
  });

  it("does not route `this` inside class field initializers or static blocks", () => {
    const { code, ledger } = bundle([
      ["a.js", "var topThis = this;"],
      [
        "b.js",
        "class Widget {\n  opts = this.compute();\n  static reg = this.seed;\n  static { this.init(); }\n  run() { return this.value; }\n}"
      ]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const a = files.get("a.js") ?? "";
    const b = files.get("b.js") ?? "";
    // Wrapper top-level `this` still routes.
    assert.match(
      a,
      /var topThis = __bundle\.thisArg;/,
      `top-level this:\n${a}`
    );
    // Class-element `this` is the instance/class — must stay bare.
    assert.match(b, /opts = this\.compute\(\);/, `field init this:\n${b}`);
    assert.match(b, /static reg = this\.seed;/, `static field this:\n${b}`);
    assert.match(b, /this\.init\(\);/, `static block this:\n${b}`);
    assert.match(b, /return this\.value;/, `method this:\n${b}`);
    assert.doesNotMatch(b, /__bundle\.thisArg/, `no class this routed:\n${b}`);
  });
});

describe("emitRunnableCjs entry point and load order", () => {
  it("emits an index.js entry requiring every file in first-appearance order", () => {
    const { code, ledger } = bundle([
      ["core/a.js", "var counter = 0;"],
      ["core/b.js", "function bump() { counter = counter + 1; }"],
      ["side/effect.js", 'var boot = "boot";']
    ]);
    const files = emitRunnableCjs(code, ledger);
    const index = files.get("index.js") ?? "";
    const posA = index.indexOf('require("./core/a.js");');
    const posB = index.indexOf('require("./core/b.js");');
    const posS = index.indexOf('require("./side/effect.js");');
    const posPad = index.indexOf('require("./pad/fill.js");');
    assert.ok(posA >= 0, `entry requires a:\n${index}`);
    assert.ok(posB > posA, `b after a:\n${index}`);
    assert.ok(
      posS > posB,
      `unreferenced side-effect file still loads:\n${index}`
    );
    assert.ok(posPad > posS, `padding last:\n${index}`);
  });

  it("initializes the bundle context before any file loads", () => {
    const { code, ledger } = bundle([
      ["a.js", "var api = 1;"],
      ["b.js", "module.exports = api;"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    const index = files.get("index.js") ?? "";
    const initPos = index.indexOf(
      "__bundle.init(module, require, __filename, __dirname, this);"
    );
    const firstRequire = index.indexOf('require("./a.js");');
    assert.ok(initPos >= 0, `entry initializes context:\n${index}`);
    assert.ok(firstRequire > initPos, `init precedes loads:\n${index}`);
  });

  it("throws on a load-time cross-file reference cycle", () => {
    const { code, ledger } = bundle([
      ["a.js", "var xa = yb + 1;"],
      ["b.js", "var yb = 2;"],
      ["b.js", "var zb = xa + 1;"]
    ]);
    assert.throws(() => emitRunnableCjs(code, ledger), /load-time.*cycle/);
  });

  it("treats reads inside functions as deferred (require cycle allowed)", () => {
    const { code, ledger } = bundle([
      ["a.js", "var xa = 1;"],
      ["a.js", "function fa() { return yb; }"],
      ["b.js", "var yb = 2;"],
      ["b.js", "function fb() { return xa; }"]
    ]);
    const files = emitRunnableCjs(code, ledger);
    assert.ok(files.get("a.js")?.includes("__b_js.yb"), "deferred reads emit");
  });

  it("classifies top-level IIFE bodies as load-time", () => {
    const { code, ledger } = bundle([
      ["a.js", "var xa = (function () { return yb; })();"],
      ["b.js", "var yb = (function () { return xa; })();"]
    ]);
    assert.throws(() => emitRunnableCjs(code, ledger), /load-time.*cycle/);
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
