import * as fs from "node:fs";
import * as parser from "@babel/parser";
import { statementHash } from "../../../experiments/lib/js/statement-hash.js";
const led = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216/.humanify/split-ledger.json", "utf8"));
const shipped = fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body: any[] = p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression" ? p0.expression.body.body : ast.program.body;
let same = 0, diff = 0;
const diffs: number[] = [];
body.forEach((s: any, i: number) => {
  if (i >= led.hashes.length) return;
  if (statementHash(s) === led.hashes[i]) same++;
  else { diff++; if (diffs.length < 5) diffs.push(i); }
});
console.log(`era statementHash == ledger hashes: ${same} same, ${diff} diff of ${body.length}`);
console.log("first diffs:", diffs.join(","));
// where does the ledger's hash list hold the giant's hash?
