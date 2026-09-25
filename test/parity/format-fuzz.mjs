// WP5.6 differential fuzz corpus for `core::format`: random module-mode
// programs built from the constructs the stage-6 visitors and Babel's
// traversal engine react to (sequences, logicals incl. `??`, conditionals,
// `void`, `!n`, flipped comparisons, exponent / hex numbers, `.concat`
// folds, multi-declarator `var`/`let`, for/if/while/label/switch/try
// bodies, arrows, ESM exports), with comments sprinkled between tokens.
//   node test/parity/format-fuzz.mjs <seed> <count> > cases.json
//   npx tsx test/parity/format-probe.ts snippets cases.json > fuzz.json
//   humanify format-check fuzz.json
// Deterministic for a seed (mulberry32).
const [seedArg, countArg] = process.argv.slice(2);
let state = Number(seedArg ?? 1) >>> 0;
function rand() {
  state = (state + 0x6d2b79f5) >>> 0;
  let t = state;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const pick = (xs) => xs[Math.floor(rand() * xs.length)];
const chance = (p) => rand() < p;

let uid = 0;
const fresh = () => `v${uid++}`;
const COMMENTS = ["/*c*/", "/* d */", "// l\n", "/*\n*/", "/*! b */"];
const sp = () => (chance(0.06) ? ` ${pick(COMMENTS)} ` : " ");

function num() {
  return pick(["0", "1", "2", "5e3", "1e21", "1e-7", "0xe1", "0xff", ".5e1", "1_0e3", "0x10", "3.0", "12"]);
}
function atom(d) {
  return pick([
    () => pick(["a", "b", "c", "x", "y", "undefined", "this"]),
    () => num(),
    () => pick(['"s"', "'t'", '"a\\nb"', '""', "`q`", "`p${a}q`"]),
    () => "null",
    () => pick(["true", "false"]),
    () => `${pick(["f", "g", "a.b", "h.concat"])}(${args(d + 1)})`,
    () => `${pick(['"s"', "`t${x}`", "`u`", "a", '""'])}.concat(${expr(d + 1)})`,
    () => `(${expr(d + 1)})`,
  ])();
}
function args(d) {
  const n = Math.floor(rand() * 3);
  return Array.from({ length: n }, () => expr(d)).join(",");
}
// An operand: an atom, or a compound expression in parentheses (keeps the
// programs valid: no `a && b = c`, no unparenthesized `a ?? b || c`).
function opnd(d) {
  return chance(0.5) ? atom(d) : `(${expr(d)})`;
}
function expr(d = 0) {
  if (d > 3) return atom(d);
  return pick([
    () => atom(d),
    () => atom(d),
    () => `${opnd(d + 1)}${sp()}${pick(["==", "!=", "===", "<", ">=", "+", "in"])}${sp()}${opnd(d + 1)}`,
    () => `${num()} ${pick(["<", "<=", "==", "!=="])} ${pick(["a", "b.c", "f()"])}`,
    () => `${opnd(d + 1)}${sp()}${pick(["&&", "||", "??"])}${sp()}${opnd(d + 1)}`,
    () => `${opnd(d + 1)} ?${sp()}${opnd(d + 1)} :${sp()}${opnd(d + 1)}`,
    () => `(${expr(d + 1)},${sp()}${expr(d + 1)})`,
    () => `void ${opnd(d + 1)}`,
    () => `!${num()}`,
    () => `${pick(["a", "b", "x"])} ${pick(["=", "+=", "||="])} ${opnd(d + 1)}`,
    () => `${pick(["a.p", "x[0]"])} = ${opnd(d + 1)}`,
    () => `${pick(["z", "(z)", "(p, q)"])} => ${chance(0.5) ? opnd(d + 1) : `{${stmts(d + 1, true, false)}}`}`,
    () => `({${sp()}k: ${opnd(d + 1)}, m${sp()}})`,
    () => `[${args(d + 1)}]`,
    () => `typeof ${opnd(d + 1)}`,
  ])();
}
function decls(d, kind) {
  const n = 1 + Math.floor(rand() * 3);
  return Array.from({ length: n }, () => (chance(0.6) || kind === "const" ? `${fresh()} = ${expr(d)}` : fresh())).join(`,${sp()}`);
}
// A statement; `single` = a single-statement position (an if/loop/label
// body), where Babel's strict mode forbids declarations other than var.
function stmt(d, inFn, inLoop, single = false) {
  if (d > 3) return `${expr(d)};`;
  const body = () => stmt(d + 1, inFn, inLoop, true);
  const loopBody = () => stmt(d + 1, inFn, true, true);
  const options = [
    () => `${expr(d)};`,
    () => `${expr(d)};`,
    () => `${expr(d)},${sp()}${expr(d)};`,
    () => `var ${decls(d, "var")};`,
    () => `if (${expr(d)})${sp()}${body()}${chance(0.5) ? ` else ${body()}` : ""}`,
    () => `for (${pick(["", `var ${decls(d, "var")}`, `let ${fresh()} = 0`, `${opnd(d)}, ${opnd(d)}`])}; ${pick(["", expr(d)])}; ${pick(["", "i++", "i++, j--", expr(d)])})${sp()}${loopBody()}`,
    () => `while (${expr(d)}) ${loopBody()}`,
    () => `do ${loopBody()} while (${expr(d)});`,
    () => `for (const ${fresh()} of ${opnd(d)}) ${loopBody()}`,
    () => `{${stmts(d + 1, inFn, inLoop)}}`,
    () => `${fresh()}:${sp()}${body()}`,
    () => `switch (${expr(d)}) { case 1: ${stmts(d + 1, inFn, inLoop)} break; default: ${stmt(d + 1, inFn, inLoop)} }`,
    () => `try { ${stmts(d + 1, inFn, inLoop)} } catch (${fresh()}) { ${stmts(d + 1, inFn, inLoop)} }`,
    () => `throw ${expr(d)};`,
    () => `void ${opnd(d)};`,
  ];
  if (!single) {
    options.push(() => `let ${decls(d, "let")};`);
    options.push(() => `function ${fresh()}(${pick(["", "p", "p, q"])}) {${stmts(d + 1, true, false)}}`);
  }
  if (inFn) {
    options.push(() => `return${chance(0.8) ? ` ${expr(d)}` : ""};`);
    options.push(() => `return ${opnd(d)}, ${opnd(d)};`);
    options.push(() => `return void ${opnd(d)};`);
    options.push(() => `return ${opnd(d)} ? ${opnd(d)} : ${opnd(d)};`);
  }
  if (inLoop) options.push(() => pick(["break;", "continue;"]));
  return pick(options)();
}
function stmts(d, inFn, inLoop) {
  const n = Math.floor(rand() * 4);
  return Array.from({ length: n }, () => `${sp()}${stmt(d, inFn, inLoop)}`).join("\n");
}
function program() {
  uid = 0;
  const body = [];
  const n = 1 + Math.floor(rand() * 5);
  for (let i = 0; i < n; i++) body.push(stmt(0, false, false));
  if (chance(0.15)) body.push(`export var ${decls(0, "var")};`);
  if (chance(0.1)) body.push(`export let ${decls(0, "let")};`);
  if (chance(0.1)) body.push(`export default (${expr(1)}, ${expr(1)});`);
  return body.join(pick(["\n", " ", sp()]));
}
const count = Number(countArg ?? 1000);
const cases = [];
for (let i = 0; i < count; i++) cases.push({ name: `fuzz/${seedArg}/${i}`, code: program() });
process.stdout.write(`${JSON.stringify(cases)}\n`);
