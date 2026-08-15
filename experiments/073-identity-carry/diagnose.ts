/**
 * 073 diagnostic — WHY does a provably-identical module's emitted file
 * still differ after masking identifiers? Prints the first differing
 * masked lines for a few matched pairs.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { tokenizeLine } from "../../src/rename/diff-reconcile.js";

const [PRIOR, FRESH] = process.argv.slice(2);
interface LedgerModule {
  file: string;
  hashes: string[];
  imports: number[];
}
const ledger = (root: string): LedgerModule[] =>
  JSON.parse(fs.readFileSync(path.join(root, ".humanify/split-ledger.json"), "utf8"))
    .fossilModules;

const key = (m: LedgerModule) => m.hashes.join("|");
const prior = ledger(PRIOR);
const fresh = ledger(FRESH);
const cp = new Map<string, number>();
for (const m of prior) cp.set(key(m), (cp.get(key(m)) ?? 0) + 1);
const cf = new Map<string, number>();
for (const m of fresh) cf.set(key(m), (cf.get(key(m)) ?? 0) + 1);
const priorByKey = new Map<string, LedgerModule>();
for (const m of prior) if (cp.get(key(m)) === 1) priorByKey.set(key(m), m);

const mask = (line: string): string | null => {
  const t = tokenizeLine(line);
  return t ? t.map((x) => (x.kind === "ident" ? "@" : x.text)).join("") : null;
};

let shown = 0;
const reasons = new Map<string, number>();
for (const f of fresh) {
  if (cf.get(key(f)) !== 1) continue;
  const p = priorByKey.get(key(f));
  if (!p) continue;
  const pp = path.join(PRIOR, p.file);
  const fp = path.join(FRESH, f.file);
  if (!fs.existsSync(pp) || !fs.existsSync(fp)) continue;
  const la = fs.readFileSync(pp, "utf8").split("\n");
  const lb = fs.readFileSync(fp, "utf8").split("\n");
  for (let i = 0; i < Math.min(la.length, lb.length); i++) {
    if (la[i] === lb[i]) continue;
    const ma = mask(la[i]);
    const mb = mask(lb[i]);
    if (ma === null || mb === null) {
      reasons.set("untokenizable-line", (reasons.get("untokenizable-line") ?? 0) + 1);
      continue;
    }
    if (ma === mb) continue; // name-only, fine
    const isRequire = /require\(/.test(la[i]) || /require\(/.test(lb[i]);
    const isExport = /module\.exports|Object\.defineProperty/.test(la[i] + lb[i]);
    const r = isRequire ? "require-header" : isExport ? "export-block" : "other";
    reasons.set(r, (reasons.get(r) ?? 0) + 1);
    if (r === "other" && shown < 8) {
      shown++;
      console.log(`--- ${f.file} line ${i + 1} [${r}]`);
      console.log(`  prior : ${la[i].trim().slice(0, 150)}`);
      console.log(`  fresh : ${lb[i].trim().slice(0, 150)}`);
      console.log(`  maskP : ${ma.trim().slice(0, 150)}`);
      console.log(`  maskF : ${mb.trim().slice(0, 150)}`);
    }
  }
}
console.log("\nnon-name differing lines inside provably-identical modules, by kind:");
for (const [k, v] of [...reasons.entries()].sort((a, b) => b[1] - a[1]))
  console.log(`  ${k.padEnd(20)} ${v}`);
