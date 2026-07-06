/**
 * Pinpoints WHY two supposedly-identical functions hash differently:
 * serializes both to their token streams and prints the first divergence
 * with context. This is the tool that found the shorthand-flag
 * instability; use it on the residual hash-absent samples from
 * measure-close-match-anomaly.ts compare output.
 *
 * Usage:
 *   node --max-old-space-size=8192 --import tsx/esm \
 *     experiments/013-bun-cjs-classification/inspect-hash-divergence.ts \
 *     <fileA> <lineA>:<colA> <fileB> <lineB>:<colB>
 */

import fs from "node:fs";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/core";
import type * as t from "@babel/types";
import {
  hashPathWithMapping,
  serializePathTokens
} from "../../src/analysis/structural-hash.js";
import { traverse } from "../../src/babel-utils.js";

function fnPathAt(
  file: string,
  line: number,
  col: number
): NodePath<t.Function> | null {
  const code = fs.readFileSync(file, "utf-8");
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error(`failed to parse ${file}`);
  let found: NodePath<t.Function> | null = null;
  traverse(ast, {
    Function(p: NodePath<t.Function>) {
      const loc = p.node.loc;
      if (loc?.start.line === line && loc.start.column === col) {
        found = p;
        p.stop();
      }
    }
  });
  return found;
}

function parsePos(s: string): [number, number] {
  const [line, col] = s.split(":").map(Number);
  return [line, col];
}

const [fileA, posA, fileB, posB] = process.argv.slice(2);
if (!fileA || !posA || !fileB || !posB) {
  console.error(
    "usage: inspect-hash-divergence.ts <fileA> <line:col> <fileB> <line:col>"
  );
  process.exit(1);
}

const [lineA, colA] = parsePos(posA);
const [lineB, colB] = parsePos(posB);
const pathA = fnPathAt(fileA, lineA, colA);
const pathB = fnPathAt(fileB, lineB, colB);
if (!pathA || !pathB) {
  console.error(`function not found: A=${Boolean(pathA)} B=${Boolean(pathB)}`);
  process.exit(1);
}

const tokensA = serializePathTokens(pathA);
const tokensB = serializePathTokens(pathB);
console.log(`tokens: A=${tokensA.length} B=${tokensB.length}`);

const limit = Math.min(tokensA.length, tokensB.length);
let index = 0;
while (index < limit && tokensA[index] === tokensB[index]) index++;

if (index === limit && tokensA.length === tokensB.length) {
  console.log("streams identical — hashes should match");
  process.exit(0);
}

const from = Math.max(0, index - 12);
console.log(`first divergence at token ${index}:`);
console.log(
  `  A[${from}..${index + 8}]:`,
  tokensA.slice(from, index + 8).join(" ")
);
console.log(
  `  B[${from}..${index + 8}]:`,
  tokensB.slice(from, index + 8).join(" ")
);

// When the diverging tokens are slots, a mint/reuse divergence happened —
// name the bindings so the resolution difference is visible.
const mappingA = hashPathWithMapping(pathA).mapping;
const mappingB = hashPathWithMapping(pathB).mapping;
const slotA = tokensA[index];
const slotB = tokensB[index];
if (slotA?.startsWith("$") || slotB?.startsWith("$")) {
  console.log(`  A ${slotA} = ${mappingA.get(slotA ?? "")}`);
  console.log(`  B ${slotB} = ${mappingB.get(slotB ?? "")}`);
  console.log(`  A also has ${slotB} = ${mappingA.get(slotB ?? "")}`);
  console.log(`  B also has ${slotA} = ${mappingB.get(slotA ?? "")}`);
  // Find the first slot NUMBER where the two mappings' names stop
  // corresponding — that's where a binding was seen "early" on one side.
  const size = Math.max(mappingA.size, mappingB.size);
  for (let n = 0; n < size; n++) {
    const a = mappingA.get(`$${n}`);
    const b = mappingB.get(`$${n}`);
    if (a === undefined || b === undefined) {
      console.log(`  slot count divergence at $${n}: A=${a} B=${b}`);
      break;
    }
  }
  console.log(`  total slots: A=${mappingA.size} B=${mappingB.size}`);
}
