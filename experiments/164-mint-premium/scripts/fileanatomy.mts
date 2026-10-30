import * as fs from "node:fs";
import * as parser from "@babel/parser";
import { statementHash } from "../../../experiments/lib/js/statement-hash.js";

const led = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/tree/.humanify/split-ledger.json", "utf8"));
const file = "/work/exp164-mint-census/dump-run/tree/vendor/zod/module-b9b02094.js";
const ast = parser.parse(fs.readFileSync(file, "utf8"), { sourceType: "unambiguous", errorRecovery: true });
const stmts = ast.program.body;
console.log("file top-level statements:", stmts.length);
// ledger hash index
const hashToOrdinals = new Map<string, number[]>();
led.hashes.forEach((h: string, i: number) => { const a = hashToOrdinals.get(h) ?? []; a.push(i); hashToOrdinals.set(h, a); });
let matched = 0;
const ords: (number[] | null)[] = [];
stmts.forEach((s: any, i: number) => {
  const h = statementHash(s);
  const hits = hashToOrdinals.get(h) ?? [];
  ords.push(hits.length ? hits : null);
  if (hits.length) matched++;
});
console.log(`statements whose content hash is in the ledger: ${matched} of ${stmts.length}`);
ords.forEach((o, i) => console.log(`  stmt ${i}: ${o ? "ledger " + o.slice(0, 4).join(",") + (o.length > 4 ? ` (+${o.length - 4})` : "") : "NOT in ledger"}  ${String(stmts[i]).slice(0, 70).replace(/\n/g, " ")}`));
