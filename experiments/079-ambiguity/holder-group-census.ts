/**
 * 079 — is a "holder group" ONE statement, or every statement in the bundle
 * that happens to hash the same?
 *
 *   npx tsx --max-old-space-size=32768 \
 *     experiments/079-ambiguity/holder-group-census.ts <humanified.js>
 *
 * WHY THIS BLOCKS THE FIX. The hop reported 2,032 count mismatches, and the
 * obvious reading is "a sibling was added to my options object". But the rung
 * builds its group by filtering the function's structural-hash bucket for
 * members whose ENCLOSING STATEMENT HASH matches — and that hash is
 * rename-invariant, so two textually distinct `register(() => a, () => b)`
 * calls in different modules hash IDENTICALLY.
 *
 * If groups routinely span several statement instances, then:
 *   - a count mismatch is not a local edit at all, it is the number of
 *     instances of a PATTERN changing anywhere in the bundle, and
 *   - the equal-count path that resolves 12,997 functions is pairing across
 *     unrelated statements by global source order, which is the positional
 *     assignment that cost this project +50,606 lines in exp035/036.
 *
 * Those need opposite fixes, so this must be measured before either is built.
 * Static property of one tree — no pipeline run needed.
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFingerprintIndex } from "../../src/analysis/fingerprint-index.js";
import { buildFunctionGraph } from "../../src/analysis/function-graph.js";
import { hashPathWithMapping } from "../../src/analysis/structural-hash.js";

const [BUNDLE] = process.argv.slice(2);
if (!BUNDLE) {
  console.error("usage: holder-group-census.ts <humanified.js>");
  process.exit(1);
}
const MAX_SPAN = 50; // mirrors MAX_ENCLOSING_STMT_LINES

const ast = parseSync(fs.readFileSync(BUNDLE, "utf8"), {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
}) as t.File;
if (!ast) throw new Error("parse failed");

const fns = buildFunctionGraph(ast, BUNDLE);
const index = buildFingerprintIndex(new Map(fns.map((f) => [f.sessionId, f])));

const stmtHashCache = new Map<t.Node, string | null>();
const stmtHash = (
  stmt: { node: t.Node } & Record<string, unknown>
): string | null => {
  const cached = stmtHashCache.get(stmt.node);
  if (cached !== undefined) return cached;
  const loc = stmt.node.loc;
  let value: string | null = null;
  if (loc && loc.end.line - loc.start.line + 1 <= MAX_SPAN) {
    try {
      // biome-ignore lint/suspicious/noExplicitAny: NodePath shape from the graph
      value = hashPathWithMapping(stmt as any).hash;
    } catch {
      value = null;
    }
  }
  stmtHashCache.set(stmt.node, value);
  return value;
};

/** group key -> distinct statement nodes, and member count */
const groups = new Map<string, { nodes: Set<t.Node>; members: number }>();

for (const fn of fns) {
  const fp = index.fingerprints.get(fn.sessionId);
  if (!fp) continue;
  const stmt = fn.path.getStatementParent();
  if (!stmt || stmt.node === fn.path.node) continue;
  const h = stmtHash(stmt as never);
  if (!h) continue;
  const key = `${fp.structuralHash}|${h}`;
  const g = groups.get(key) ?? { nodes: new Set<t.Node>(), members: 0 };
  g.nodes.add(stmt.node);
  g.members++;
  groups.set(key, g);
}

const multi = [...groups.values()].filter((g) => g.members > 1);
const spanning = multi.filter((g) => g.nodes.size > 1);
const membersInSpanning = spanning.reduce((a, g) => a + g.members, 0);
const membersInMulti = multi.reduce((a, g) => a + g.members, 0);

console.log(`bundle: ${BUNDLE}`);
console.log(
  `functions with a usable enclosing statement: ${[...groups.values()].reduce((a, g) => a + g.members, 0)}`
);
console.log(`(structuralHash, stmtHash) groups: ${groups.size}`);
console.log(
  `  groups with >1 member (the ones the rung acts on): ${multi.length}`
);
console.log(
  `  ...of those, groups SPANNING >1 statement node:    ${spanning.length}`
);
console.log(`\nfunctions in multi-member groups:            ${membersInMulti}`);
console.log(
  `functions in groups spanning >1 statement:   ${membersInSpanning}` +
    ` (${((100 * membersInSpanning) / Math.max(1, membersInMulti)).toFixed(1)}%)`
);

const dist = new Map<number, number>();
for (const g of multi)
  dist.set(g.nodes.size, (dist.get(g.nodes.size) ?? 0) + 1);
console.log(`\ndistinct statement NODES per multi-member group:`);
for (const n of [...dist.keys()].sort((a, b) => a - b).slice(0, 12)) {
  console.log(
    `  ${String(n).padStart(4)} node(s)  ${String(dist.get(n)).padStart(7)} group(s)`
  );
}
const maxNodes = Math.max(...multi.map((g) => g.nodes.size));
console.log(`  max: ${maxNodes} distinct statements sharing one group`);
console.log(
  `\nIf the spanning share is large, a count mismatch is NOT a local sibling\n` +
    `edit and the equal-count path is pairing across unrelated statements by\n` +
    `global source order.`
);
