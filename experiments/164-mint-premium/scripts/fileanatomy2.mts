import * as fs from "node:fs";
import * as parser from "@babel/parser";
import { statementHash } from "../../../experiments/lib/js/statement-hash.js";

const led = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/split-ledger.json", "utf8"));
const file = "/work/exp164-mint-census/dump-run/tree/vendor/zod/module-b9b02094.js";
const text = fs.readFileSync(file, "utf8");
const ast = parser.parse(text, { sourceType: "unambiguous", errorRecovery: true });
const stmts = ast.program.body;
const hashToOrdinals = new Map<string, number[]>();
led.hashes.forEach((h: string, i: number) => { const a = hashToOrdinals.get(h) ?? []; a.push(i); hashToOrdinals.set(h, a); });
stmts.forEach((s: any, i: number) => {
  const h = statementHash(s);
  const hits = hashToOrdinals.get(h) ?? [];
  const src = text.slice(s.start, s.end);
  console.log(`stmt ${i} ${hits.length ? "LEDGER " + hits.join(",") : "not-in-ledger"}  len=${src.length}  ${src.slice(0, 88).replace(/\n/g, " ")}`);
});
for (const o of [1127, 1128, 1129]) {
  console.log(`ledger ${o}: file=${led.files[o]} hash=${(led.hashes[o] ?? "").slice(0, 12)} emit=${led.emitNames[o]}`);
}
