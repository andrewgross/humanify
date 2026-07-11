/**
 * Exp025: prove concat-equivalence on a REAL emitted tree — reconstruct
 * the wrapper body from the on-disk files + ledger, re-wrap in the IIFE,
 * and check it is structurally identical to the original bundle (same
 * top-level statement count, same declared-binding set).
 * Usage: verify-reconstruct.ts <original.js> <treeDir>
 */
import fs from "node:fs";
import path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import {
  reconstructBody,
  SPLIT_LEDGER_FILENAME,
  type StableSplitLedger
} from "../../src/split/stable-split.js";

const [original, treeDir] = process.argv.slice(2);
const ledger: StableSplitLedger = JSON.parse(
  fs.readFileSync(path.join(treeDir, SPLIT_LEDGER_FILENAME), "utf-8")
);
const fileContents = new Map<string, string>();
for (const rel of ledger.files) {
  fileContents.set(rel, fs.readFileSync(path.join(treeDir, rel), "utf-8"));
}

console.log(
  `files: ${fileContents.size}, order length: ${ledger.order.length}`
);
const t0 = Date.now();
const body = reconstructBody(fileContents, ledger);
console.log(`reconstruct: ${((Date.now() - t0) / 1000).toFixed(1)}s`);

const rebuilt = `(function (exports, require, module, __filename, __dirname) {\n${body}\n});`;
const rebuiltAst = parseSync(rebuilt, {
  sourceType: "unambiguous",
  configFile: false
}) as t.File;
if (!rebuiltAst) throw new Error("rebuilt does not parse");

// Compare to the original wrapper body.
const origAst = parseSync(fs.readFileSync(original, "utf-8"), {
  sourceType: "unambiguous",
  configFile: false
}) as t.File;
const origWrapper = (origAst.program.body[0] as t.ExpressionStatement)
  .expression as t.FunctionExpression;
const rebuiltWrapper = (rebuiltAst.program.body[0] as t.ExpressionStatement)
  .expression as t.FunctionExpression;

const origStmts = origWrapper.body.body;
const rebuiltStmts = rebuiltWrapper.body.body;

function bindingSet(stmts: t.Statement[]): Set<string> {
  const s = new Set<string>();
  for (const stmt of stmts)
    for (const n of Object.keys(t.getBindingIdentifiers(stmt, false))) s.add(n);
  return s;
}
const ob = bindingSet(origStmts);
const rb = bindingSet(rebuiltStmts);
const missing = [...ob].filter((n) => !rb.has(n));
const extra = [...rb].filter((n) => !ob.has(n));

console.log(
  JSON.stringify(
    {
      originalStatements: origStmts.length,
      rebuiltStatements: rebuiltStmts.length,
      statementCountMatch: origStmts.length === rebuiltStmts.length,
      originalBindings: ob.size,
      rebuiltBindings: rb.size,
      missingBindings: missing.length,
      extraBindings: extra.length,
      RECONSTRUCTION_SOUND:
        origStmts.length === rebuiltStmts.length &&
        missing.length === 0 &&
        extra.length === 0
    },
    null,
    2
  )
);
