/**
 * Exp025: is the module graph LOADABLE? Build the load-time dependency
 * graph (A->B iff A reads a B-declared binding at module load time) and
 * test for a topological order. Cycles among load-time edges are the only
 * true executability blocker; everything else CommonJS handles.
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import type { Binding, NodePath, Scope } from "@babel/traverse";
import * as t from "@babel/types";
import { traverse } from "../../src/babel-utils.js";

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
let sc: Scope | undefined;
let wrapperNode: t.Node | undefined;
traverse(ast, {
  FunctionExpression(p) {
    sc = p.scope;
    wrapperNode = p.node;
    p.stop();
  }
});
const scope = sc as Scope;
function isDeferred(ref: NodePath): boolean {
  let p: NodePath | null = ref.parentPath;
  while (p) {
    if (p.node === wrapperNode) return false;
    if (p.isFunction() || p.isClassMethod?.() || p.isObjectMethod?.())
      return true;
    p = p.parentPath;
  }
  return false;
}

// load-time edges: reader file -> declaring file
const edges = new Map<string, Set<string>>();
for (const f of ledger.files) edges.set(f, new Set());
for (const name of Object.keys(scope.bindings)) {
  const b: Binding = scope.bindings[name];
  const declFile = fileOfPos(b.identifier.start ?? -1);
  if (!declFile) continue;
  for (const ref of b.referencePaths) {
    const f = fileOfPos(ref.node.start ?? -1);
    if (f && f !== declFile && !isDeferred(ref)) edges.get(f)!.add(declFile);
  }
}
const edgeCount = [...edges.values()].reduce((s, v) => s + v.size, 0);

// Kahn topological sort over load-time edges (A must load AFTER its deps).
const indeg = new Map<string, number>();
for (const f of ledger.files) indeg.set(f, 0);
for (const [, deps] of edges) for (const d of deps) indeg.set(d, indeg.get(d)!); // deps are targets
// build reverse: dep -> dependents to decrement
const dependents = new Map<string, string[]>();
for (const f of ledger.files) dependents.set(f, []);
for (const [f, deps] of edges)
  for (const d of deps) {
    dependents.get(d)!.push(f);
    indeg.set(f, indeg.get(f)! + 1);
  }
const queue = [...ledger.files].filter((f) => indeg.get(f) === 0);
let loaded = 0;
while (queue.length) {
  const f = queue.pop()!;
  loaded++;
  for (const dep of dependents.get(f)!) {
    indeg.set(dep, indeg.get(dep)! - 1);
    if (indeg.get(dep) === 0) queue.push(dep);
  }
}

// Find files stuck in cycles.
const inCycle = ledger.files.filter((f: string) => indeg.get(f)! > 0);
console.log(
  JSON.stringify(
    {
      files: ledger.files.length,
      loadTimeEdges: edgeCount,
      topologicallyLoadable: loaded,
      filesInLoadTimeCycle: inCycle.length,
      ACYCLIC_LOADABLE: inCycle.length === 0,
      cycleSample: inCycle.slice(0, 10)
    },
    null,
    2
  )
);
