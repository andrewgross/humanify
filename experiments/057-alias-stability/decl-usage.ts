/**
 * What do the names in a bare multi-declarator `var` actually DO?
 *
 *   npx tsx experiments/057-alias-stability/decl-usage.ts <treeSrc> <file> <anchorName>
 *
 * The statement that the masked-hash collision moved is
 * `var a, b, c, … ;` with 32 declarators and no initializers. Whether that is a
 * dead forward-declaration or the head of a live lazy-init group changes what a
 * fix should even look like — so count, per name:
 *
 *   exported   the file re-exports it via `defineProperty(module.exports, …)`
 *   assigned   something in the SAME file writes it (the lazy-init body)
 *   readLocal  something in the same file reads it
 *   consumers  other files in the tree that mention it
 *
 * A name with no assignment and no reader anywhere is genuinely dead; one that
 * is assigned in a lazy-init body and read across the tree is load-bearing, and
 * the declaration is only the hoisted head of it.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";

const [ROOT, FILE, ANCHOR] = process.argv.slice(2);

const text = fs.readFileSync(path.join(ROOT, FILE), "utf8");
const ast = parseSync(text, {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
});
if (!ast) throw new Error("parse failed");

/** The bare declaration statement containing the anchor name. */
let names: string[] = [];
for (const node of (ast.program.body ?? []) as t.Statement[]) {
  if (!t.isVariableDeclaration(node)) continue;
  const ids = node.declarations
    .filter((d) => t.isIdentifier(d.id) && d.init == null)
    .map((d) => (d.id as t.Identifier).name);
  if (ids.includes(ANCHOR)) names = ids;
}
if (!names.length) throw new Error(`no bare declaration holding ${ANCHOR}`);

const exported = new Set<string>();
for (const line of text.split("\n")) {
  const m = /^Object\.defineProperty\(module\.exports, "([^"]+)"/.exec(line);
  if (m) exported.add(m[1]);
}

const assigned = new Set<string>();
const read = new Map<string, number>();
t.traverseFast(ast, (n) => {
  if (t.isAssignmentExpression(n) && t.isIdentifier(n.left)) {
    assigned.add(n.left.name);
  }
});
// Reads: every identifier occurrence minus the declaration itself and the
// export-getter boilerplate is close enough to answer "is this referenced".
for (const name of names) {
  const re = new RegExp(`\\b${name}\\b`, "g");
  read.set(name, (text.match(re) ?? []).length);
}

/** Files elsewhere in the tree that mention the name. */
function walk(dir: string, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.name.endsWith(".js")) out.push(p);
  }
  return out;
}
const others = walk(ROOT).filter((p) => path.relative(ROOT, p) !== FILE);
const consumers = new Map<string, number>();
const consumerSites = new Map<string, number>();
for (const p of others) {
  const body = fs.readFileSync(p, "utf8");
  for (const name of names) {
    if (!body.includes(name)) continue;
    const hits = (body.match(new RegExp(`\\b${name}\\b`, "g")) ?? []).length;
    if (!hits) continue;
    consumers.set(name, (consumers.get(name) ?? 0) + 1);
    consumerSites.set(name, (consumerSites.get(name) ?? 0) + hits);
  }
}

console.log(`\n=== ${FILE} — bare declaration holding ${ANCHOR} ===`);
console.log(`  declarators: ${names.length}\n`);
console.log(
  `  ${"name".padEnd(30)} ${"exported".padEnd(9)} ${"assigned".padEnd(9)} ${"refs here".padEnd(10)} ${"consumers".padEnd(10)} sites`
);
let dead = 0;
for (const name of names) {
  const c = consumers.get(name) ?? 0;
  const isDead = !assigned.has(name) && c === 0;
  if (isDead) dead++;
  console.log(
    `  ${name.slice(0, 29).padEnd(30)} ${(exported.has(name) ? "yes" : "-").padEnd(9)} ${(assigned.has(name) ? "yes" : "-").padEnd(9)} ${String(read.get(name) ?? 0).padStart(9)}  ${String(c).padStart(9)}  ${consumerSites.get(name) ?? 0}`
  );
}
console.log(
  `\n  exported ${[...names].filter((n) => exported.has(n)).length}/${names.length}` +
    `   assigned in-file ${[...names].filter((n) => assigned.has(n)).length}/${names.length}` +
    `   used by other files ${[...names].filter((n) => (consumers.get(n) ?? 0) > 0).length}/${names.length}`
);
console.log(`  declared but neither assigned nor referenced anywhere: ${dead}`);
