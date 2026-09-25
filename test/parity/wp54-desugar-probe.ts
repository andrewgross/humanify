// WP5.4 probe: real-TS `desugarUsing` outputs over constructed inputs that
// reach every branch of the plugin + generator the Rust port reproduces,
// frozen as test/parity/wp54-desugar.json for
// crates/humanify-core/src/finish/using/using_test.rs.
//
//   npx tsx test/parity/wp54-desugar-probe.ts test/parity/wp54-desugar.json
//
// `refused` marks inputs the Rust refuses on purpose (comments, a spelled
// uid, a shadowed helper global): the TS output is still recorded so the
// refusal stays a visible, documented boundary.
import fs from "node:fs";
import { desugarUsing } from "../../src/split/using-desugar.js";

const CASES: { name: string; code: string; refused?: string }[] = [
  {
    name: "function body",
    code: "function f(a) {\n  using x = open(a);\n  return x.read();\n}\nf(1);\n"
  },
  {
    name: "two using in one block + a plain statement between",
    code: "function f() {\n  using a = g();\n  work(a);\n  using b = h(a), c = k();\n  return [a, b, c];\n}\n"
  },
  {
    name: "await using in an async arrow",
    code: "const run = async (p) => {\n  await using fh = await open(p);\n  return fh.stat();\n};\n"
  },
  {
    name: "mixed using + await using",
    code: "async function m() {\n  using s = sync();\n  await using a = await asy();\n  return s + a;\n}\n"
  },
  {
    name: "nested blocks: if, bare block, labeled, loop body",
    code: "function f(c) {\n  if (c) {\n    using a = g();\n    use(a);\n  } else {\n    using b = h();\n  }\n  {\n    using d = k();\n  }\n  lbl: {\n    using e = k();\n    break lbl;\n  }\n  for (const i of c) {\n    using z = i;\n  }\n}\n"
  },
  {
    name: "try and catch bodies",
    code: "function f() {\n  try {\n    using a = g();\n  } catch (e) {\n    using b = h(e);\n  } finally {\n    using c = k();\n  }\n}\n"
  },
  {
    name: "async ordering: a nested function before the async function's own block",
    code: "async function outer() {\n  const inner = () => {\n    using a = g();\n    return a;\n  };\n  {\n    using b = h();\n  }\n  return inner;\n}\nfunction later() {\n  using c = k();\n}\n"
  },
  {
    name: "eleven contexts (the uid suffix wraps: 9, 0, 1, 10)",
    code: Array.from(
      { length: 11 },
      (_, i) => `function f${i}() {\n  using v${i} = g(${i});\n}\n`
    ).join("")
  },
  {
    name: "class static block + method",
    code: "class A {\n  static {\n    using s = init();\n  }\n  m() {\n    using t = this.r();\n    return t;\n  }\n}\n"
  },
  {
    name: "setFunctionName: arrow, anonymous function, anonymous class",
    code: "function f() {\n  using a = () => 1;\n  using b = function () {};\n  using c = class {};\n  using d = function named() {};\n}\n"
  },
  {
    name: "directive prologue + requires + object/array shapes",
    code: '"use strict";\nconst lib = require("./lib.js");\nfunction f(o) {\n  using h = lib.f(o);\n  const { a, b: [c, ...d] = [], ...rest } = o;\n  return { a, c, d, rest, [h.k]: h?.v?.(1) ?? 0, get x() { return 1; } };\n}\nmodule.exports = { f };\n'
  },
  {
    name: "retainLines: multi-line call args, template literals, sequences",
    code: "function f(x) {\n  using t = tag`a${x}\nb`;\n  return g(\n    x,\n    `multi\nline ${x}`,\n    (x, t)\n  );\n}\n"
  },
  {
    name: "return argument on a later line after the rewrite",
    code: "function f() {\n  using a = g();\n  return (\n    a.b &&\n    a.c\n  );\n}\n"
  },
  {
    name: "comments are refused by the Rust",
    code: "function f() {\n  // a comment\n  using a = g();\n}\n",
    refused: "comments"
  },
  {
    name: "an input spelling a generated uid is refused",
    code: "var _usingCtx = 1;\nfunction f() {\n  using a = g();\n}\n",
    refused: "uid"
  },
  {
    name: "a program-scope binding shadowing a helper global is refused",
    code: "var Promise = require('p');\nasync function f() {\n  await using a = g();\n}\n",
    refused: "helper global"
  }
];

const outPath = process.argv[2];
if (!outPath) throw new Error("usage: see header");
const rows = CASES.map((c) => ({ ...c, out: desugarUsing(c.code) }));
fs.writeFileSync(outPath, `${JSON.stringify(rows, null, 2)}\n`);
console.log(`${rows.length} cases`);
