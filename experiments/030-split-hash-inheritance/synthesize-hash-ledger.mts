/**
 * Bridge a pre-hash-tier prior into the new world: compute statement hashes
 * for an existing humanified.js and write a copy of its split ledger with
 * `hashes[]` + `hashVersion` filled in. The hashes describe exactly the
 * text the ledger accompanies, so this is byte-for-byte what a new-code run
 * of that same release would have persisted — feed the result to
 * `--split-ledger` to activate the hash tier against an old prior.
 *
 *   npx tsx synthesize-hash-ledger.mts <humanified.js> <ledger.json> <out.json>
 */
import fs from "node:fs";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";
import {
  STATEMENT_HASH_VERSION,
  statementHash
} from "../../src/split/statement-hash.js";

const [, , humanifiedPath, ledgerPath, outPath] = process.argv;
if (!humanifiedPath || !ledgerPath || !outPath) {
  throw new Error(
    "usage: synthesize-hash-ledger.mts <humanified.js> <ledger.json> <out.json>"
  );
}

const code = fs.readFileSync(humanifiedPath, "utf8");
const ledger = JSON.parse(
  fs.readFileSync(ledgerPath, "utf8")
) as StableSplitLedger;

const ast = parseFileAst(code);
if (!ast) throw new Error("humanified source failed to parse");
const wrapper = findWrapperFunction(ast);
if (!wrapper) throw new Error("no wrapper IIFE found");
const bodyNode = wrapper.functionPath.node.body;
if (!t.isBlockStatement(bodyNode)) throw new Error("wrapper body not a block");
const body = bodyNode.body;
if (body.length !== ledger.order.length) {
  throw new Error(
    `statement count ${body.length} != ledger order ${ledger.order.length} — ` +
      "this ledger does not belong to this humanified.js"
  );
}

const t0 = Date.now();
const withHashes: StableSplitLedger = {
  ...ledger,
  hashes: body.map(statementHash),
  hashVersion: STATEMENT_HASH_VERSION
};
fs.writeFileSync(outPath, JSON.stringify(withHashes));
console.log(
  `hashed ${body.length} statements in ${Date.now() - t0}ms -> ${outPath}`
);
