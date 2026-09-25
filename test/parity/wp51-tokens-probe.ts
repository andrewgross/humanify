// WP5.2 tokens probe: the TS fossil modules' graded shape tokens (exp078
// `moduleTokens`, read through assignFossil's ledger output) for a
// synthetic Bun-shaped program whose modules cover every node shape the
// ESTree→babel translation handles: literals of each class, object
// methods/getters/shorthands/patterns, class members (public, private,
// static blocks, accessors), optional chains in all compound forms,
// directives, template literals, import(), labels, switch/try, spread.
// Output: test/parity/wp51-tokens.json ({code, modules: [{file, tokens}]}).
//
//   npx tsx test/parity/wp51-tokens-probe.ts > test/parity/wp51-tokens.json
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { assignFossil } from "../../src/split/fossil-assign.js";
import { statementHash } from "../../src/split/statement-hash.js";

const modules = [
  // literals
  `var lit1 = ["a string literal longer than forty characters in total length", 'x', 0x10, 1e21, 0.5, 1.5e-7, 12n, /re+g/gi, null, true, false, 9007199254740993, "\\u{1F600}${"a".repeat(38)}", "é${"b".repeat(39)}"];`,
  `var lit2 = \`tpl \${lit1} mid \${1 + 2} end\`, lit3 = tagFn\`raw\${lit1}\`;`,
  // numbers whose shortest JSON spelling does not round-trip through
  // serde_json's default float parser (the 2.1.216 FLT_MAX literal), plus
  // every radix / separator / legacy-octal spelling
  `var lit4 = [340282346638528860000000000000000000000, 5e-324, 1.7976931348623157e308, 0x1fffffffffffff1, 0XFF, 0o777, 0b101, 1_000_000, 0.000_1, .5e1, 5., 2e-7, 123456789012345678901234567890, 0.1, 0.30000000000000004, 1e-7, 123e-20, 4.35, 0, 1e300 * 1];`,
  `var lit5 = [017, 019, 08, 00, 0.0];`,
  // objects
  `var obj1 = { a: 1, "b-c": 2, 3: 3, [lit1]: 4, m() { return 1; }, get g() { return 2; }, set s(v) {}, async *gen() {}, lit2, ...lit1 };`,
  `var { p1, p2: { p3 = 5 }, ...rest1 } = obj1, [a1, , b1 = 2, ...c1] = lit1;`,
  // classes
  `class Klass extends Base { #priv = 1; static sfield = 2; field; static { init(); } constructor(x) { super(x); this.#priv++; } #pm() { return #priv in this; } get val() { return this.#priv; } static async sm() {} [lit1]() {} }`,
  `var ce = class Named { m() { return new.target; } };`,
  // chains
  `var ch1 = a?.b, ch2 = a.b?.c(), ch3 = (a?.b)(), ch4 = a?.[b](c), ch5 = a?.b.c?.(d)?.e, ch6 = f(x?.y) ?? z?.w;`,
  // statements
  `function fn1(p = 1, { q } = {}, ...r) { outer: for (let i = 0; i < 3; i++) { for (const k in obj1) { if (k) continue outer; else break outer; } } switch (p) { case 1: return; default: throw new Error("x"); } }`,
  `function fn2() { "use strict"; "second directive"; try { fn1(); } catch { } finally { } try { x(); } catch (e) { void e; } do { y--; } while (y); while (z) z = !z; debugger; ; }`,
  `var fn3 = async () => { await import("mod"); for await (const v of gen()) yield1(v); }, fn4 = function* () { yield* other(); yield; }, fn5 = x => x, seq = (a, b, c);`,
  `var ops = [a = b, a += b, a ||= b, a ?? b, a ** b, typeof a, delete a.b, -a, ++a, a--, a ? b : c, a instanceof B, "k" in a, new A, new A.B(c)];`,
  `with (obj1) {}
var meta = 1, spread = [...a, ...b], call = fn(...args);`
];

const code = [
  "var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);",
  ...modules.flatMap((m, i) => [
    m,
    `var init_m${i} = __esm(() => { m${i}(); });`
  ]),
  "console.log(1);"
].join("\n");

const ast = parseSync(code, {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
}) as t.File;
const body = ast.program.body;
const out = await assignFossil(body, body.map(statementHash), undefined);
process.stdout.write(
  `${JSON.stringify({ code, modules: out.fossilModules.map((m) => ({ file: m.file, tokens: m.tokens })) }, null, 1)}\n`
);
