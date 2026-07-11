/**
 * Exp025: how many cross-file reads happen at MODULE LOAD TIME (top-level,
 * outside any function/class body) vs deferred inside function bodies?
 * Load-time cross-file reads are the ones circular CJS requires can break;
 * deferred reads are safe. This is the execution-runnability number.
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import _traverse from "@babel/traverse";
import type { Binding, NodePath } from "@babel/traverse";
import * as t from "@babel/types";
const traverse = (_traverse as unknown as { default: typeof _traverse })
  .default;

const [input, ledgerPath] = process.argv.slice(2);
const code = fs.readFileSync(input, "utf-8");
const ledger = JSON.parse(fs.readFileSync(ledgerPath, "utf-8"));
const ast = parseSync(code, {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
}) as t.File;
const wrapper = (ast.program.body[0] as t.ExpressionStatement)
  .expression as t.FunctionExpression;
const ranges = wrapper.body.body.map((s, i) => ({
  start: s.start ?? 0,
  end: s.end ?? 0,
  file: ledger.order[i]
}));
function fileOfPos(pos: number): string | null {
  let lo = 0,
    hi = ranges.length - 1;
  while (lo <= hi) {
    const m = (lo + hi) >> 1;
    const r = ranges[m];
    if (pos < r.start) hi = m - 1;
    else if (pos >= r.end) lo = m + 1;
    else return r.file;
  }
  return null;
}
let scope: NodePath<t.FunctionExpression>["scope"] | null = null;
let wrapperNode: t.Node | null = null;
traverse(ast, {
  FunctionExpression(p) {
    scope = p.scope;
    wrapperNode = p.node;
    p.stop();
  }
});
const sc = scope as NonNullable<typeof scope>;

// A reference is "deferred" if an ancestor BETWEEN it and the wrapper is a
// function/class body (executes later). Reaching the wrapper itself without
// crossing an inner function means the ref runs at module load time.
function isDeferred(ref: NodePath): boolean {
  let p: NodePath | null = ref.parentPath;
  while (p) {
    if (p.node === wrapperNode) return false; // reached module scope
    if (p.isFunction() || p.isClassMethod?.() || p.isObjectMethod?.())
      return true;
    p = p.parentPath;
  }
  return false;
}

let loadTimeCross = 0,
  deferredCross = 0;
const loadTimeSamples: string[] = [];
for (const name of Object.keys(sc.bindings)) {
  const b: Binding = sc.bindings[name];
  const declFile = fileOfPos(b.identifier.start ?? -1);
  for (const ref of b.referencePaths) {
    const f = fileOfPos(ref.node.start ?? -1);
    if (!f || f === declFile) continue;
    if (isDeferred(ref)) deferredCross++;
    else {
      loadTimeCross++;
      if (loadTimeSamples.length < 20)
        loadTimeSamples.push(`${name} (${declFile} -> ${f})`);
    }
  }
}
console.log(
  JSON.stringify(
    {
      loadTimeCrossFileReads: loadTimeCross,
      deferredCrossFileReads: deferredCross,
      deferredShare:
        ((100 * deferredCross) / (loadTimeCross + deferredCross)).toFixed(1) +
        "%",
      loadTimeSamples
    },
    null,
    2
  )
);

// Concentration: which binding NAMES account for the load-time reads?
{
  const byName = new Map<string, number>();
  for (const name of Object.keys(sc.bindings)) {
    const b: Binding = sc.bindings[name];
    const declFile = fileOfPos(b.identifier.start ?? -1);
    let n = 0;
    for (const ref of b.referencePaths) {
      const f = fileOfPos(ref.node.start ?? -1);
      if (f && f !== declFile && !isDeferred(ref)) n++;
    }
    if (n > 0) byName.set(name, n);
  }
  const top = [...byName.entries()].sort((a, b) => b[1] - a[1]);
  const total = [...byName.values()].reduce((s, v) => s + v, 0);
  const top20 = top.slice(0, 20).reduce((s, [, v]) => s + v, 0);
  console.log("\n=== load-time read concentration ===");
  console.log(
    `distinct binding names with load-time cross-file reads: ${byName.size}`
  );
  console.log(
    `top-20 names account for ${top20}/${total} (${((100 * top20) / total).toFixed(0)}%)`
  );
  console.log("top 15:");
  for (const [n, c] of top.slice(0, 15))
    console.log(`  ${String(c).padStart(4)}  ${n}`);
}
