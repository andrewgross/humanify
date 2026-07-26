/**
 * Rule 1 support tool: for one vendor file pair, print the FIRST canonical
 * tokens that actually diverge. `vendor-churn-decompose.ts` classifies a file
 * as real change when its literal-preserving signature moves; this shows the
 * token that moved, so the classification can be eyeballed instead of trusted.
 *
 *   npx tsx experiments/046-vendor-noise/why-differs.ts <priorFile> <freshFile>
 */
import fs from "node:fs";
import { parseSourceAst, traverse } from "../../src/babel-utils.js";
import { serializePathTokens } from "../../src/analysis/structural-hash.js";
import type { NodePath } from "@babel/traverse";

function tokens(file: string): string[] {
  const ast = parseSourceAst(fs.readFileSync(file, "utf-8"));
  if (!ast) throw new Error(`failed to parse ${file}`);
  let out: string[] = [];
  traverse(ast, {
    Program(p: NodePath) {
      out = serializePathTokens(p, { preserveLiterals: true });
      p.stop();
    }
  });
  return out;
}

const [a, b] = process.argv.slice(2);
const ta = tokens(a);
const tb = tokens(b);
console.log(`tokens: prior ${ta.length}, fresh ${tb.length}`);

let shown = 0;
const n = Math.max(ta.length, tb.length);
for (let i = 0; i < n && shown < 12; i++) {
  if (ta[i] === tb[i]) continue;
  const ctx = ta.slice(Math.max(0, i - 6), i).join(" ");
  console.log(`\n@${i}  ...${ctx}`);
  console.log(`  prior: ${ta[i]}`);
  console.log(`  fresh: ${tb[i]}`);
  shown++;
  // A single insertion desynchronizes everything after it; stop reporting
  // once the streams are clearly misaligned rather than printing noise.
  if (ta.length !== tb.length) {
    console.log("  (lengths differ — stream desynchronized past this point)");
    break;
  }
}
if (shown === 0)
  console.log("\nIDENTICAL under literal-preserving canonicalization.");
