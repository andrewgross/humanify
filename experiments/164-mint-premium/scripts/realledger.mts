import * as fs from "node:fs";
const led = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216/.humanify/split-ledger.json", "utf8"));
const shipped = fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216/.humanify/humanified.js", "utf8");
console.log("order", led.order.length, "hashes", led.hashes.length, "emit", led.emitNames.length, "shipped", shipped.length);
for (const o of [1126, 1127, 1128, 1129, 1130]) {
  console.log(`ledger ${o}: file=${led.order[o]} hash=${(led.hashes[o] ?? "").slice(0, 16)} emit=${JSON.stringify(led.emitNames[o])?.slice(0, 60)}`);
}
// dump-run ledger for comparison
const dled = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/split-ledger.json", "utf8"));
console.log("dump order", dled.order.length, "dumps same?", dled.order.length === led.order.length);
let diff = 0;
for (let i = 0; i < Math.min(dled.order.length, led.order.length); i++) if (dled.order[i] !== led.order[i]) diff++;
console.log("order-file diffs between the two runs' ledgers:", diff);
for (const o of [1127, 1128, 1129]) console.log(`dump ${o}: file=${dled.order[o]} hash=${(dled.hashes[o] ?? "").slice(0, 16)}`);
// era statement 1129's text
import * as parser from "@babel/parser";
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body = p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression" ? p0.expression.body.body : ast.program.body;
console.log("era body len", body.length);
for (const o of [1127, 1128, 1129]) {
  const s: any = body[o];
  const src = shipped.slice(s.start, s.end);
  console.log(`era ${o} len=${src.length} ${src.slice(0, 110).replace(/\n/g, " ")}`);
}
fs.writeFileSync("/tmp/exp164-era-1129.txt", shipped.slice(body[1129].start, body[1129].end));
