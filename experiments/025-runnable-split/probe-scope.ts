/**
 * Exp025 probe v2: SCOPE-AWARE cross-file write analysis. Only counts
 * writes to genuine wrapper-scope (module) bindings, via Babel's
 * constantViolations — function-local `errorMessage` etc. are excluded.
 * Usage: probe-scope.ts <humanified.js> <ledger.json>
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import traversepkg from "@babel/traverse";
import * as t from "@babel/types";
const traverse =
  (traversepkg as unknown as { default: typeof traversepkg }).default ??
  traversepkg;

const [input, ledgerPath] = process.argv.slice(2);
const code = fs.readFileSync(input, "utf-8");
const ast = parseSync(code, {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
}) as t.File;
const ledger = JSON.parse(fs.readFileSync(ledgerPath, "utf-8"));
const order: string[] = ledger.order;

// Map wrapper-body statement START line -> file, to locate a node's file.
let wrapperScope: import("@babel/traverse").Scope | null = null;
const stmtFileByStart = new Map<number, string>();
{
  const first = ast.program.body[0] as t.ExpressionStatement;
  const wrapper = first.expression as t.FunctionExpression;
  wrapper.body.body.forEach((s, i) => {
    if (s.loc) stmtFileByStart.set(s.start ?? -1, order[i]);
  });
}

// Which top-level statement (by byte range) contains a given node position?
const stmtRanges: Array<{ start: number; end: number; file: string }> = [];
{
  const first = ast.program.body[0] as t.ExpressionStatement;
  const wrapper = first.expression as t.FunctionExpression;
  wrapper.body.body.forEach((s, i) => {
    stmtRanges.push({ start: s.start ?? 0, end: s.end ?? 0, file: order[i] });
  });
}
function fileOfPos(pos: number): string | null {
  // binary search
  let lo = 0,
    hi = stmtRanges.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const r = stmtRanges[mid];
    if (pos < r.start) hi = mid - 1;
    else if (pos >= r.end) lo = mid + 1;
    else return r.file;
  }
  return null;
}

traverse(ast, {
  FunctionExpression(path) {
    if (wrapperScope) return;
    // the outermost function expression = wrapper
    wrapperScope = path.scope;
    path.stop();
  }
});

let moduleBindings = 0;
let writtenBindings = 0;
let crossFileWrittenBindings = 0;
const offenders: Array<{
  name: string;
  declFile: string;
  writeFiles: string[];
}> = [];

const scope = wrapperScope as unknown as import("@babel/traverse").Scope;
for (const name of Object.keys(scope.bindings)) {
  const binding = scope.bindings[name];
  moduleBindings++;
  if (binding.constantViolations.length === 0) continue;
  writtenBindings++;
  const declFile = fileOfPos(binding.identifier.start ?? -1);
  const writeFiles = new Set<string>();
  for (const v of binding.constantViolations) {
    const f = fileOfPos(v.node.start ?? -1);
    if (f && f !== declFile) writeFiles.add(f);
  }
  if (writeFiles.size > 0) {
    crossFileWrittenBindings++;
    if (offenders.length < 15)
      offenders.push({
        name,
        declFile: declFile ?? "?",
        writeFiles: [...writeFiles]
      });
  }
}

console.log(
  JSON.stringify(
    {
      moduleScopeBindings: moduleBindings,
      writtenModuleBindings: writtenBindings,
      crossFileWrittenModuleBindings: crossFileWrittenBindings,
      sampleOffenders: offenders
    },
    null,
    2
  )
);
