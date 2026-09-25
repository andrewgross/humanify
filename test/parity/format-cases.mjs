// WP5.6 golden inputs for `core::format` (written to format-cases.json;
// test/parity/format-probe.ts `snippets` turns them into
// format-goldens.json, the formatter's frozen spec). Groups:
//   beautifier/*  babel-plugin-transform-beautifier 0.1.1 __tests__ inputs
//   babel-test/*  src/plugins/babel/babel.test.ts inputs
//   f42/*         finding #42's reproduced bugs
//   engine/*      @babel/traverse queue/requeue/insert/remove paths
//   plugin/*      the three small plugins
//   printer/*     generator paths (ESM, directives, classes, parens)
//   comments/*    attached-but-unprinted comments that steer the printer
//   error/*       inputs Babel's module-mode parse rejects
//   node format-cases.mjs > format-cases.json
const cases = {
  // -- the beautifier's own spec (index.spec.ts) --------------------------
  "beautifier/declaration": "var a, b = 1",
  "beautifier/declaration full": "var a = 1, b = 2, c = 3",
  "beautifier/declaration empty": "var a, b, c",
  "beautifier/declaration in for": "for (var a = 1, b = 2;;);",
  "beautifier/conditional": "a ? b : c",
  "beautifier/conditional return": "() => { return a ? b : c }",
  "beautifier/conditional assign": "a = b ? c : d",
  "beautifier/conditional deep": "a(b ? c : d)",
  "beautifier/for stmt": "for (;;);",
  "beautifier/for block": "for (;;) {}",
  "beautifier/seq": "a, b, c",
  "beautifier/seq return": "() => { return a, b, c }",
  "beautifier/seq deep": "a((b, c, d))",
  "beautifier/logical and": "a && b",
  "beautifier/logical or": "a || b",
  "beautifier/logical deep": "if (a && b);",
  "beautifier/unary true": "!0",
  "beautifier/unary false": "!1",
  "beautifier/unary undefined": "void 0",
  "beautifier/unary void": "void x()",
  "beautifier/unary void return": "() => { return void x() }",
  "beautifier/unary void deep": "a(void b())",
  "beautifier/if block": "if (x) {}",
  "beautifier/if else block": "if (x) {} else {}",
  "beautifier/if else logical": "if (x); else a && b",
  "beautifier/if else conditional": "if (x); else a ? b : c",
  "beautifier/if else conditional assign": "if (x); else a = b ? c : d",
  "beautifier/if else conditional assign complex": "if (x); else a().v = b ? c : d",
  "beautifier/template literal": '"a".concat(b).concat(c)',
  "beautifier/template literal with tail": '"a".concat(b).concat("c")',
  "beautifier/template literal with string": '"a".concat("b").concat("c")',
  "beautifier/#3-1": "({ a: async function() { await 0; } })",
  "beautifier/#3-2": "new ({ a: function() {} }).a()",
  "beautifier/#3-3": "export var a, b = 1;",
  "beautifier/#3-4": "for (let i = 0, j; i < 3; i++);",
  "beautifier/#3-5": "let undefined = 1; console.log(void 0);",
  "beautifier/#3-6": "a().b = c() ? d() : e(); // a, c, d",

  // -- src/plugins/babel/babel.test.ts ------------------------------------
  "babel-test/for update sequence": "for (let i = 0; i < n; i++, j += 2) { foo(i); }",
  "babel-test/statement sequence": "a(), b();",
  "babel-test/for single update": "for (let i = 0; i < n; i++) { foo(i); }",

  // -- finding #42 (reproduced, not fixed) --------------------------------
  "f42/nullish statement": "H._zod ?? (H._zod = {});",
  "f42/nullish chain": "a ?? b ?? c;",
  "f42/concat raw is cooked": '"a\\nb\\\\c".concat(x);',
  "f42/concat backtick in string": '"a`b${c}".concat(x);',
  "f42/template concat doubled tail": "`x${y}z`.concat(\"!\");",
  "f42/template concat empty cooked": "`${y}`.concat(\"!\");",
  "f42/void number unguarded": "function f(undefined) { return void 0; }",
  "f42/void string guarded": "function f(undefined) { g(void 'x'); }",
  "f42/void string unguarded": "function f() { g(void 'x'); }",
  "f42/void template": "g(void `a${b()}`);",

  // -- the traversal engine ------------------------------------------------
  "engine/labeled var split": "lbl: var a = 1, b = 2;",
  "engine/if consequent var split": "if (x) var a = 1, b = 2; else var c, d = 3;",
  "engine/while body var split": "while (x) var a = 1, b = 2;",
  "engine/with-less do body": "do var a = 1, b = 2; while (x);",
  "engine/for-init var hoist": "for (var a = 1, b = 2, c; c < 3; c++) f(a, b);",
  "engine/labeled for-init var hoist": "lbl: for (var a, b = 0; b < 1; b++) continue lbl;",
  "engine/nested for-init hoist": "if (x) for (var a, b;;) break;",
  "engine/while test sequence": "while (a(), b) c();",
  "engine/if test sequence": "if ((a, b)) c; else d;",
  "engine/switch sequence": "switch ((a, b)) { case 1: c(), d(); }",
  "engine/throw sequence": "function f() { throw a(), b(); }",
  "engine/return sequence in if": "function f() { if (x) return a(), b(); else return c(), d; }",
  "engine/logical then sequence": "a && (b, c);",
  "engine/sequence then logical": "(p, q) && r;",
  "engine/logical chain and": "a && b && c;",
  "engine/logical chain or": "a || b || c;",
  "engine/logical nested": "a && (b || c);",
  "engine/conditional nested": "a ? b ? c : d : e;",
  "engine/conditional sequence branches": "x ? (a, b) : (c, d);",
  "engine/conditional logical branches": "x ? a && b : c || d;",
  "engine/assign conditional ops": "v += a ? b : c; w.y = a ? b : c;",
  "engine/return conditional nested": "function f() { return a ? b : c ? d : e; }",
  "engine/else-if chains": "if (a) b; else if (c) d; else e;",
  "engine/else assign conditional": "if (a) x = b ? c : d; else y = e ? f : g;",
  "engine/void in if test": "if (void a()) b();",
  "engine/void statement": "void a(), void b();",
  "engine/void in throw": "function f() { throw void a(); }",
  "engine/void in return seq": "function f() { return void a(), b; }",
  "engine/void in for update": "for (;; void a()) b();",
  "engine/for body sequence": "for (;;) a(), b();",
  "engine/for all heads": "for (a, b; c, d; e, f) g();",
  "engine/for-in body": "for (k in o) a(), b();",
  "engine/for-of body": "for (const v of o) a && b;",
  "engine/arrow bodies": "const f = () => (a, b); const g = () => a && b; const h = () => void a();",
  "engine/export default sequence": "export default (a(), b);",
  "engine/export let split": "export let a, b = 1, c = 2;",
  "engine/export const inits": "export const a = 1, b = 2;",
  "engine/deep sequence in arrow block": "const f = () => { a(), b(); return c ? d : e; };",
  "engine/class method bodies": "class A { m() { a && b(); return x ? y : z; } static { c(), d(); } }",
  "engine/object method bodies": "const o = { m() { var a, b = 1; }, get g() { return a, b; } };",
  "engine/try catch finally": "try { a(), b(); } catch (e) { c && d(); } finally { var x, y = 1; }",
  "engine/switch cases": "switch (x) { case 1: a(), b(); break; default: c ? d() : e(); }",
  "engine/labeled statement bodies": "a: b(), c();",
  "engine/do while": "do a(), b(); while (c);",
  "engine/let undefined block": "{ let undefined = 1; void 'a'; void b(); } void 'c';",
  "engine/catch undefined": "try {} catch (undefined) { void 'a'; void b(); }",
  "engine/fn expr named undefined": "(function undefined() { void 'a'; })();",
  "engine/var undefined hoisted": "function f() { if (x) { var undefined; } void 'a'; }",
  // A statement wrapped in a new block: the block's Scope crawls it and
  // re-parents the requeued sequence path (Babel's NodePath.get in the
  // crawl), so the logical sees an ExpressionStatement parent.
  "engine/crawl reparents a requeued path": "while (x) a, `u`.concat(y) ?? z;",
  "engine/crawl in if branch": "if (x) a(), b ? c() : d(); else e(), f && g();",

  // -- the three small plugins --------------------------------------------
  "plugin/numbers": "a(5e3, 1e21, 1e-7, 0xe1, 0xff, 0XE1, 1E3, .5e1, 1e400, 5e-324, 0e0, 12e2);",
  "plugin/numbers separators": "a(1_0e3, 1_000, 0xe_1);",
  "plugin/number member": "a = 5e3.toString(); b = 1e3[0];",
  "plugin/comparisons": "a(5 < x, 5 <= x, 5 > x, 5 >= x, null == x, null != x, 'a' === x, 1 !== x);",
  "plugin/comparisons literal both": "a('a' == 'b', 1 < 2, x == 1, `t` === y, /r/ == z, 1n < x, true == x, 1 in x, 1 + x);",
  "plugin/not numbers": "a(!0, !1, !2, !0.0, !-1, !0x0);",
  "plugin/void": "a(void 0, void 1, void 'x', void null, void x);",

  // -- printer ------------------------------------------------------------
  "printer/esm imports": 'import d, { a, b as c } from "m"; import * as ns from "n"; import "side"; import {} from "empty"; import { "str" as s } from "o";',
  "printer/esm exports": 'export { a, b as c }; export * from "m"; export * as ns from "n"; export { x as default, y } from "o"; export {} from "p"; export function f() {} export class K {}',
  "printer/esm default": "export default function () {}",
  "printer/esm default class": "export default class {}",
  "printer/esm default expr": "export default a + b;",
  "printer/import attributes": 'import j from "./x.json" with { type: "json" }; export { k } from "./y.json" with { type: "json" };',
  "printer/directives": '"use strict"; function f() { "use asm"; return 1; }',
  "printer/directive only": '"use strict";',
  "printer/empty program": "",
  "printer/hashbang": "#!/usr/bin/env node\na();",
  "printer/classes": "class A extends B { #p = 1; static s; get g() { return this.#p; } set g(v) {} static async *m() { yield* x; } accessor z = 2; [k]() {} }",
  "printer/arrows": "a(x => x, (x) => x, (x, y) => x, async x => x, ({ a }) => a, ([b]) => b, (c = 1) => c, (...d) => d, () => ({}));",
  "printer/objects": "a({}, { a }, { a: 1, b }, { [k]: 1, 'q': 2, 3: 4 }, { ...s }, { m() {}, get x() { return 1; } });",
  "printer/templates": "a(`x`, `a${b}c${d}e`, tag`t${x}`, `multi\nline`);",
  "printer/optional": "a?.b?.[c]?.(d); (a?.b)(); new (a?.b)();",
  "printer/parens": "a = (b, c); (function () {})(); (class {}); ({}).x; (a || b) && c; a ?? (b || c); (-a) ** 2; (await x)?.y;",
  "printer/in in for": "for (var i = (a in b); ;); for (const k in (a, b));",
  "printer/regex bigint": "a(/re/g, 10n, 0x1Fn, /[/]/);",
  "printer/strings": "a('single', \"double\", 'q\"uote', 'esc\\n');",
  "printer/new": "new A; new A(); new (a())(); new (a.b()); new a.b.C();",
  "printer/labels": "outer: for (;;) { inner: for (;;) { continue outer; break inner; } }",
  "printer/getter setter static": "class A { static get x() { return 1; } static set x(v) {} static #y() {} }",
  "printer/async await": "async function f() { await a; for await (const x of y) {} } const g = async () => { await b; };",
  "printer/generators": "function* g() { yield; yield a; yield* b; }",
  "printer/meta": "function f() { new.target; } import.meta.url;",
  "printer/dynamic import": 'import("m").then(x => x); import("n", { with: { type: "json" } });',
  "printer/update": "a++; --b; c = d++ + ++e; f = g - -h; i = j + +k;",
  "printer/let edge": "(let[0] = 1);",
  "printer/in operator for-init": "for (var x = function () { return a in b; }; ;) break;",
  "printer/long var list": "var a = 1, b = function () { return c; }, d;",

  // -- attached comments that are never printed but steer the printer -----
  "comments/paren leading block": "x = (/* c */ a + b) * 2;",
  "comments/return newline comment": "function f() { return (// c\n a); }",
  "comments/return block newline": "function f() { return /* c\n */ a; }",
  "comments/arrow param comment": "a((/* c */ x) => x, (x /* d */) => x);",
  "comments/if branch leading": "if (a) /* c */ b(); else /* d */ c();",
  "comments/if block leading": "if (a) /* c */ { b(); }",
  "comments/statement banners": "/*! banner */ var a = 1, b = 2; /*! two */ a && b;",
  "comments/line comments": "// head\nvar a = 1; // tail\nb();",
  "comments/sequence with comments": "/* s */ a(), /* t */ b();",

  // -- inputs Babel's module-mode parse rejects ---------------------------
  "error/octal literal": "a = 010;",
  "error/with statement": "with (a) b;",
  "error/syntax": "a = ;",
  // Babel's own validate() throws on these valid inputs (the TS stage
  // crashes; so must the port).
  "error/labeled var in do body": "do lbl: var a = 1, b = 2; while (x);",
  "error/labeled var in if body": "if (x) lbl: var a = 1, b = 2;",
};
process.stdout.write(
  `${JSON.stringify(
    Object.entries(cases).map(([name, code]) => ({ name, code })),
    null,
    2
  )}\n`
);
