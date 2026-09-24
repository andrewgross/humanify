// WP3.1 scope-model corpus: [code, sourceType]. Shared by the scope probe
// (wp31-scope-probe.mjs). Covers every construct whose Babel scope shape
// differs from oxc's (catch bodies, Annex-B block functions, method keys,
// class names, pattern params, loop heads, redeclarations) plus the
// validated-rename test fixtures' programs.
export const SCOPE_SNIPPETS = [
  ["try { f() } catch (e) { let x = e; var y; }", "script"],
  ["try { f() } catch ({ message: m = d }) { g(m); }", "script"],
  ["try { f() } catch { g(); }", "script"],
  ["function g() { if (a) { function h() {} h(); } h(); }", "script"],
  ["function g() { { function h() {} } }", "module"],
  [
    "class C { m() { return C; } } var D = class E extends E2 { n() { return E; } };",
    "script"
  ],
  [
    "class K extends B { [x]() { return y; } static { let s = 1; } get [z]() {} p = q; #r() {} static t = () => u; }",
    "script"
  ],
  [
    "var o = { m() { return o; }, [k]: 1, async *n(a) { yield a; }, get g() {} };",
    "script"
  ],
  ["var F = function G() { return G; };", "script"],
  [
    "var a = 1; var a = 2; for (var a of b) {} a += 1; a++; [a] = c; ({x: a} = c); for (a in c) {} delete a; --a;",
    "script"
  ],
  ["function k(p) { var p; return arguments; }", "script"],
  ["function dup() { return 1 } dup(); function dup() { return 2 }", "script"],
  [
    "label: for (;;) { break label; } while (w) { let v = w; } do { let dd; } while (x);",
    "script"
  ],
  ["function q({a = z}, [b], ...rest) { return a + b + rest; }", "script"],
  ["const r = ({a}) => a; const s = (x = y) => x;", "script"],
  ["switch (v) { case 1: let w = 1; function sw() {} }", "script"],
  [
    "for (let i = 0; i < n; i++) { let j = i; } for (const k of ks) k; for (var m in o) {}",
    "script"
  ],
  [
    "for (k in o) {} for (l of ls) {} undeclared = 1; ({ p: q2 } = o); typeof tt;",
    "script"
  ],
  ["export default function mitt(e) { return e; }", "module"],
  [
    "export const aa = 1; export { aa as bb }; export function cc() {} export class DD {} export default class EE {}",
    "module"
  ],
  [
    "import def, { named as local, other } from 'x'; import * as ns from 'y'; def(local, other, ns);",
    "module"
  ],
  [
    "var x = function () { var x; return x; }; (function y() { y = 1; })();",
    "script"
  ],
  ["function f() { let a; { let a; { a; } } }", "script"],
  ["if (c) function ff() {} ff();", "script"],
  ["var obj = { a, b: c, ...d }; var { e, f: [g], ...h } = obj;", "script"],
  // biome-ignore lint/suspicious/noTemplateCurlyInString: the snippet IS source text containing a template literal
  ["a: { break a; } var t1 = `${tpl}`; new N(); x?.y; z?.();", "script"],
  [
    "var x, y; class K2 { [x]() { return y; } static [y] = 1; } var o2 = { [x]() {}, [y]: 2 };",
    "script"
  ],
  [
    "for (var i = 0; i < 1; i++) {} function ff2() { for (var j of js) {} for (var p in ps) {} }",
    "script"
  ],
  ["function g2() { switch (a) { case 1: function sf() {} } sf(); }", "script"],
  [
    "var C2 = 1; var D2 = class C2 {}; class C3 {} var E3 = class C3 { m() { C3; } };",
    "script"
  ],
  ["function h2(a = b, { c } = d) { var e; } var b, d;", "script"],
  ["try {} catch (err) { var err = 1; } ", "script"],
  [
    "function outer() { var v = 1; return function inner() { v = 2; return typeof v; }; }",
    "script"
  ],
  [
    "const fe = async function* agen(p1) { await p1; }; const ar = async (p2) => p2;",
    "module"
  ],
  ["x = 1; y += 2; [z] = []; w++; delete q3;", "script"],
  [
    "var a, c, d, r; for ([a, {b: c}] of xs) {} for ({d} in o) {} ({ a = 1 } = o); [...r] = x; [a = c] = x;",
    "script"
  ],
  [
    "var sv = 1; function sw2(p) { switch (sv) { case p: let sv = 2; return sv; } }",
    "script"
  ],
  ["var g1; function fg() { if (a) { g1 = function g1() {}; } }", "script"],
  [
    "function lf(H) { var _ = -1; while (++_ < q) { var $ = H[_]; if ($) { var k = 1; function g3() {} } } for (;;) var u; do { var dd2 = 1; } while (0); for (var i2 = 0;;) {} }",
    "script"
  ],
  [
    "async function us() { using r = a(); await using o = b(); { using n = c(); r; } }",
    "module"
  ],
  ["var a, x; [x = (a = 2), a] = y; a = (a = 3);", "script"],
  ["var a, b; [a = b++] = c; ({ k: a = delete b } = c);", "script"],
  // validated-rename.test.ts fixtures
  ["var a = 1; console.log(a);", "module"],
  ["var a = 1; var b = 2;", "module"],
  ["var helper = 1; function f(a) { return a + helper; }", "module"],
  ["function f(a) { { let helper = 1; console.log(a, helper); } }", "module"],
  [
    "function f(a) { return a; } function g(helper) { return helper; }",
    "module"
  ],
  ["var a = 1; a = 2; a += 3; [a] = [4]; console.log(a);", "module"],
  ["function a() { return 1; } a(); var r = a;", "module"],
  ["var a = 1; console.log(a); var a = 2; console.log(a);", "module"],
  [
    "function a() { return 1; } console.log(a()); function a() { return 2; }",
    "script"
  ],
  ["var a = 1; for (var a of [2]) { console.log(a); }", "module"],
  [
    "var source = { x: 2 }; var a = 1; var { x: a } = source; console.log(a);",
    "module"
  ],
  ["var d = 1; console.log(document.title, d);", "module"],
  ["var d = 1; console.log(myAppGlobal.title, d);", "module"],
  ["var d = 1; function f() { return myAppGlobal.title + d; }", "module"],
  ["export const a = 1; console.log(a);", "module"],
  [
    "\n      function connect(cfg) {\n        let transport;\n        if (cfg) {\n          let env = { a: 1 };\n          transport = { env: env };\n        }\n        return transport;\n      }",
    "module"
  ],
  [
    "\n      function connect(cfg) {\n        let transport;\n        const setup = () => {\n          let env = { a: 1 };\n          transport = { env: env };\n        };\n        return [setup, transport];\n      }",
    "module"
  ],
  [
    "\n      function build(list) {\n        let validationErrorList = list.filter(Boolean);\n        for (let entry of validationErrorList) {\n          console.log(entry);\n        }\n        return validationErrorList;\n      }",
    "module"
  ],
  [
    "\n      function walk(obj) {\n        let keyMap = obj.entries;\n        for (const propKey in keyMap) {\n          console.log(propKey, keyMap[propKey]);\n        }\n      }",
    "module"
  ],
  [
    "export function mitt(e) { return e; }\nconst use = mitt;\nexport { use };\n",
    "module"
  ],
  [
    "\nfunction getFileWriter() {\n  let outerDir = null;\n  register({ writeFn: (task) => {\n    let innerDir = dirname(getPath());\n    let changed = outerDir !== innerDir;\n    outerDir = innerDir;\n    return changed;\n  }});\n}",
    "script"
  ],
  [
    "var X; X = class q extends X {}; var Y = function r() { return Y; };",
    "script"
  ]
];
