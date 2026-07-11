/**
 * Exp025 probe: characterize what makes the wrapper body hard to emit as
 * independent modules — cross-file writes and forward refs, resolved
 * through the ledger's file assignments.
 * Usage: probe-structure.ts <humanified.js> <ledger.json>
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";

const [input, ledgerPath] = process.argv.slice(2);
const ast = parseSync(fs.readFileSync(input, "utf-8"), {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
}) as t.File;
const first = ast.program.body[0] as t.ExpressionStatement;
const wrapper = first.expression as t.FunctionExpression;
const body = wrapper.body.body;

const ledger = JSON.parse(fs.readFileSync(ledgerPath, "utf-8"));
const order: string[] = ledger.order;

// name -> declaring file (first decl)
const nameToFile = new Map<string, string>();
body.forEach((s, i) => {
  for (const n of Object.keys(t.getBindingIdentifiers(s, false))) {
    if (!nameToFile.has(n)) nameToFile.set(n, order[i]);
  }
});

let crossWriteStmts = 0;
const crossWrittenByFile = new Map<string, number>();
const crossWrittenBindings = new Map<string, number>();
let pureExportOnly = 0; // files whose bindings are never written cross-file

body.forEach((stmt, i) => {
  const own = new Set(Object.keys(t.getBindingIdentifiers(stmt, false)));
  const file = order[i];
  let cw = false;
  t.traverseFast(stmt, (node) => {
    if (t.isAssignmentExpression(node) && t.isIdentifier(node.left)) {
      const target = node.left.name;
      if (
        !own.has(target) &&
        nameToFile.has(target) &&
        nameToFile.get(target) !== file
      ) {
        cw = true;
        crossWrittenBindings.set(
          target,
          (crossWrittenBindings.get(target) ?? 0) + 1
        );
      }
    }
  });
  if (cw) {
    crossWriteStmts++;
    crossWrittenByFile.set(file, (crossWrittenByFile.get(file) ?? 0) + 1);
  }
});

const files = new Set(order);
for (const f of files) {
  // does any binding declared in f get written from elsewhere?
  pureExportOnly++; // placeholder — recompute below
}
// files that are TARGETS of cross-file writes
const writtenTargetFiles = new Set(
  [...crossWrittenBindings.keys()].map((n) => nameToFile.get(n))
);

console.log(
  JSON.stringify(
    {
      files: files.size,
      crossWriteStatements: crossWriteStmts,
      crossWrittenBindings: crossWrittenBindings.size,
      filesContainingCrossWrites: crossWrittenByFile.size,
      filesTargetedByCrossWrites: writtenTargetFiles.size,
      filesCleanOfCrossWriteTargets: files.size - writtenTargetFiles.size,
      topWrittenBindings: [...crossWrittenBindings.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, 10)
    },
    null,
    2
  )
);
