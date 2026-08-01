/**
 * Verify — not infer — what the widened placement trail pointed at.
 *
 *   npx tsx experiments/057-alias-stability/hash-collision-probe.ts <freshBundle> <priorBundle> <priorLedger> <name>
 *
 * The trail reported that the statement declaring `commandLib` was placed by the
 * HASH tier into `auth-manager.js`, while the prior ledger's `nameToFiles` puts
 * that name in `task-serializer.js`. Hash placement is supposed to be the one
 * tier that cannot move a statement, so either the trail is wrong or the hash
 * tier matched the wrong prior statement.
 *
 * This settles it with the real `statementHash`, printing both texts so the
 * answer is read rather than deduced (rule 1).
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { topLevelStatements } from "../../src/prior-version/statement-twin.js";
import { statementHash } from "../../src/split/statement-hash.js";

const [FRESH, PRIOR, LEDGER, NAME] = process.argv.slice(2);

function statements(path: string): { node: t.Node; text: string }[] {
  const code = fs.readFileSync(path, "utf8");
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast) throw new Error(`cannot parse ${path}`);
  return topLevelStatements(buildUnifiedGraph(ast, path)).map((p) => ({
    node: p.node,
    text:
      p.node.start != null && p.node.end != null
        ? code.slice(p.node.start, p.node.end)
        : ""
  }));
}

function declares(node: t.Node, name: string): boolean {
  let hit = false;
  t.traverseFast(node, (n) => {
    if (
      t.isVariableDeclarator(n) &&
      t.isIdentifier(n.id) &&
      n.id.name === name
    ) {
      hit = true;
    }
    if (
      (t.isFunctionDeclaration(n) || t.isClassDeclaration(n)) &&
      n.id?.name === name
    ) {
      hit = true;
    }
  });
  return hit;
}

const ledger = JSON.parse(fs.readFileSync(LEDGER, "utf8"));
const fresh = statements(FRESH);
const prior = statements(PRIOR);

const idx = fresh.findIndex((s) => declares(s.node, NAME));
if (idx < 0) throw new Error(`${NAME} not declared at top level in ${FRESH}`);
const hash = statementHash(fresh[idx].node as t.Statement);

console.log(`\n=== FRESH statement declaring ${NAME} (index ${idx}) ===`);
console.log(`  hash: ${hash}`);
console.log(`  text: ${fresh[idx].text.slice(0, 220)}`);
console.log(
  `  declarators: ${(fresh[idx].text.match(/,/g) ?? []).length + 1} (comma count + 1)`
);

const freshSame = fresh.filter(
  (s) => statementHash(s.node as t.Statement) === hash
).length;
const priorIdx = (ledger.hashes as string[])
  .map((h, i) => (h === hash ? i : -1))
  .filter((i) => i >= 0);

console.log(`\n=== how many statements carry that hash ===`);
console.log(`  fresh release: ${freshSame}`);
console.log(`  prior release: ${priorIdx.length}`);
console.log(
  `  hash tier fires only when those counts MATCH and every prior occurrence sits in ONE file.`
);

console.log(`\n=== the prior statement(s) it matched ===`);
for (const i of priorIdx.slice(0, 4)) {
  console.log(`  [prior ${i}] file per ledger.order: ${ledger.order[i]}`);
  console.log(
    `     text: ${(prior[i]?.text ?? "<out of range>").slice(0, 220)}`
  );
  console.log(
    `     declares ${NAME}? ${prior[i] ? declares(prior[i].node, NAME) : "?"}`
  );
}

console.log(`\n=== what the ledger says about the NAME ===`);
console.log(
  `  nameToFiles[${NAME}] = ${JSON.stringify(ledger.nameToFiles[NAME])}`
);
