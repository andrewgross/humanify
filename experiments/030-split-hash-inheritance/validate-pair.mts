/**
 * Replay one walk hop (A -> B, forward inheritance) with and without the
 * hash tier, using RUN 3's real artifacts (read-only). Controlled
 * comparison: same inputs, only the prior ledger's hashes toggled.
 *
 *   npx tsx validate-pair.mts <verA> <verB> <outRoot>
 *   e.g. npx tsx validate-pair.mts 2.1.89 2.1.90 /tmp/val
 *
 * Writes <outRoot>/val-<A>/, val-<B>-old/, val-<B>-new/ raw statement trees.
 */
import fs from "node:fs";
import path from "node:path";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import {
  type StableSplitLedger,
  stableSplitFromCode
} from "../../src/split/stable-split.js";
import {
  STATEMENT_HASH_VERSION,
  statementHash
} from "../../src/split/statement-hash.js";

const [, , verA, verB, outRoot] = process.argv;
if (!verA || !verB || !outRoot) {
  throw new Error("usage: validate-pair.mts <verA> <verB> <outRoot>");
}
const V = "/Users/andrewgross/Development/unpacked-claude-code/versions";

function wrapperBody(code: string): t.Statement[] {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error("no wrapper");
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode)) throw new Error("no block body");
  return bodyNode.body;
}

function writeTree(dir: string, files: Map<string, string>): void {
  fs.rmSync(dir, { recursive: true, force: true });
  for (const [rel, content] of files) {
    const p = path.join(dir, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, content);
  }
}

function emitByAssignment(
  body: t.Statement[],
  assignment: string[],
  code: string
): Map<string, string> {
  const byFile = new Map<string, string[]>();
  for (let i = 0; i < body.length; i++) {
    const { start, end } = body[i];
    if (start == null || end == null) throw new Error(`stmt ${i} no offsets`);
    const parts = byFile.get(assignment[i]) ?? [];
    parts.push(code.slice(start, end));
    byFile.set(assignment[i], parts);
  }
  return new Map(
    [...byFile.entries()].map(([f, parts]) => [f, `${parts.join("\n")}\n`])
  );
}

const codeA = fs.readFileSync(
  `${V}/claude-code-${verA}/.humanify/humanified.js`,
  "utf8"
);
const codeB = fs.readFileSync(
  `${V}/claude-code-${verB}/.humanify/humanified.js`,
  "utf8"
);
const ledgerA = JSON.parse(
  fs.readFileSync(
    `${V}/claude-code-${verA}/.humanify/split-ledger.json`,
    "utf8"
  )
) as StableSplitLedger;

const bodyA = wrapperBody(codeA);
if (bodyA.length !== ledgerA.order.length) {
  throw new Error(
    `${verA} body ${bodyA.length} != ledger ${ledgerA.order.length}`
  );
}
const t0 = Date.now();
const hashesA = bodyA.map(statementHash);
console.log(
  `${verA}: hashed ${hashesA.length} statements in ${Date.now() - t0}ms`
);
writeTree(
  path.join(outRoot, `val-${verA}`),
  emitByAssignment(bodyA, ledgerA.order, codeA)
);

const oldResult = await stableSplitFromCode(codeB, { prior: ledgerA });
if (!oldResult) throw new Error("old split returned null");
console.log(`OLD stats: ${JSON.stringify(oldResult.stats)}`);
writeTree(path.join(outRoot, `val-${verB}-old`), oldResult.fileContents);

const priorWithHashes: StableSplitLedger = {
  ...ledgerA,
  hashes: hashesA,
  hashVersion: STATEMENT_HASH_VERSION
};
const newResult = await stableSplitFromCode(codeB, { prior: priorWithHashes });
if (!newResult) throw new Error("new split returned null");
console.log(`NEW stats: ${JSON.stringify(newResult.stats)}`);
writeTree(path.join(outRoot, `val-${verB}-new`), newResult.fileContents);

// How many statements changed file between the two behaviors?
let diff = 0;
for (let i = 0; i < oldResult.ledger.order.length; i++) {
  if (oldResult.ledger.order[i] !== newResult.ledger.order[i]) diff++;
}
console.log(
  `assignment differs on ${diff}/${oldResult.ledger.order.length} statements`
);
