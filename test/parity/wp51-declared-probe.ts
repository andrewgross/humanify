// WP5.1 declared-names probe: stable-split.ts's `declaredNames`
// (`Object.keys(t.getBindingIdentifiers(stmt, false))`) and the anchor
// preempt's `Object.keys(t.getOuterBindingIdentifiers(stmt, false))` —
// the exact calls — over one program's top-level statements.
// Output: test/parity/wp51-declared.json ({code, rows: [{names, outer}]}).
//
//   npx tsx test/parity/wp51-declared-probe.ts > test/parity/wp51-declared.json
import { parseSync } from "@babel/core";
import * as t from "@babel/types";

const code = [
  "var a = 1, b, c = 2;",
  "var { d, e: { f = 3 }, ...g } = obj, [h, , i = 4, ...j] = arr;",
  "function k(l, m = 1, { n, o: [p] }, ...q) { var inner = 1; }",
  "function r() {}",
  "class S extends T { m(u) {} }",
  "let v = function w(x) {}, y = (z) => z;",
  "const { aa: bb, ['cc']: dd, ee = function ff(gg) {} } = o2;",
  "var { get: hh, set: ii } = o3;",
  "({ jj } = o4);",
  "foo(kk);",
  "kk = 5;",
  "label1: for (var ll in o5) {}",
  "for (var mm of o6) {}",
  "for (let nn = 0; nn < 1; nn++) {}",
  "if (a) { var oo = 1; }",
  "try {} catch (pp) {}",
  "var __proto__ = 1, constructor = 2, toString;",
  "function qq(rr, rr2 = function ss(tt) {}, [uu = (vv) => vv]) {}",
  "var ww = 1, ww = 2, xx = { ww };",
  "async function* yy(zz) { yield zz; }"
].join("\n");

const ast = parseSync(code, {
  sourceType: "script",
  configFile: false,
  babelrc: false
}) as t.File;
const rows = ast.program.body.map((stmt) => ({
  names: Object.keys(t.getBindingIdentifiers(stmt, false)),
  outer: Object.keys(t.getOuterBindingIdentifiers(stmt, false))
}));
process.stdout.write(`${JSON.stringify({ code, rows }, null, 1)}\n`);
