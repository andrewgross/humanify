import * as fs from "node:fs";
const dump = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const er = dump.eraRestoration;
const capturedStmts = new Set<number>();
let capturedPrints = 0;
for (const s of er.sites as any[]) {
  for (const p of s.prints) if (p.captured) { capturedPrints++; capturedStmts.add(s.statement); }
}
console.log(`captured prints ${capturedPrints} over ${capturedStmts.size} statements`);
// content-agreeing ordinals from align3's twin map: recompute cheaply —
// the aligned pairs were written to /tmp/exp164-region-align.json only for
// misaligned; recompute the agree set quickly via the region claims?
