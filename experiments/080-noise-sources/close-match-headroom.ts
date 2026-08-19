/**
 * 080 — how much of a function's naming is BODY LOCALS?
 *
 *   npx tsx experiments/080-noise-sources/close-match-headroom.ts <treeRoot> [maxFiles]
 *
 * For a CLOSE-MATCHED function — one paired with a prior version but whose body
 * changed — `computePartialTransfer` carries over only the function's own name
 * and its parameters, by index. Body locals are deliberately never transferred:
 * "Body locals can shift when statements are added/removed."
 *
 * So every body local in a close-matched function is re-picked by the model,
 * even though we know exactly which prior function it belongs to. This counts
 * the split, which is the ceiling on what a body-local matcher could reach.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { traverse } from "../../src/babel-utils.js";

const [ROOT, MAX = "600"] = process.argv.slice(2);
if (!ROOT) {
  console.error("usage: close-match-headroom.ts <treeRoot> [maxFiles]");
  process.exit(1);
}

function walk(dir: string, out: string[], limit: number): void {
  if (out.length >= limit) return;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (out.length >= limit) return;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out, limit);
    else if (e.name.endsWith(".js")) out.push(p);
  }
}
const files: string[] = [];
walk(path.join(ROOT, "src"), files, Number(MAX));

let fns = 0;
let params = 0;
let bodyLocals = 0;
let fnNames = 0;
const bodyPerFn: number[] = [];

for (const file of files) {
  let ast: t.File | null = null;
  try {
    ast = parseSync(fs.readFileSync(file, "utf8"), {
      sourceType: "unambiguous",
      configFile: false,
      babelrc: false
    }) as t.File;
  } catch {
    continue;
  }
  if (!ast) continue;
  traverse(ast, {
    Function(p) {
      fns++;
      const node = p.node;
      if (
        (node.type === "FunctionDeclaration" ||
          node.type === "FunctionExpression") &&
        node.id
      ) {
        fnNames++;
      }
      const paramNames = new Set<string>();
      for (const prm of node.params) {
        if (prm.type === "Identifier") paramNames.add(prm.name);
      }
      params += paramNames.size;
      // Bindings owned by this function's own scope, minus its parameters.
      let locals = 0;
      for (const name of Object.keys(p.scope.bindings)) {
        if (!paramNames.has(name)) locals++;
      }
      bodyLocals += locals;
      bodyPerFn.push(locals);
    }
  });
}

const total = params + bodyLocals + fnNames;
console.log(`functions scanned              ${fns.toLocaleString()}`);
console.log(`\nbindings a close match CAN transfer today:`);
console.log(`  function names               ${fnNames.toLocaleString()}`);
console.log(`  parameters                   ${params.toLocaleString()}`);
console.log(`\nbindings it CANNOT (go to the model):`);
console.log(`  body locals                  ${bodyLocals.toLocaleString()}`);
console.log(
  `\nbody locals are ${((100 * bodyLocals) / Math.max(1, total)).toFixed(1)}% of all function-scoped bindings`
);
bodyPerFn.sort((a, b) => a - b);
const p = (q: number) => bodyPerFn[Math.floor((q / 100) * bodyPerFn.length)];
console.log(
  `body locals per function: p50 ${p(50)}  p90 ${p(90)}  p99 ${p(99)}`
);
