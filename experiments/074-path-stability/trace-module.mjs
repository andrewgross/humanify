/**
 * exp074: trace ONE module's path across two runs and explain the move.
 *
 *   node experiments/074-path-stability/trace-module.mjs <stem>
 */
import fs from "node:fs";

const STEM = process.argv[2] ?? "access-property";
const L = (v) =>
  JSON.parse(
    fs.readFileSync(
      `/tmp/eval-work/exp070-r1/${v}/.humanify/split-ledger.json`,
      "utf8"
    )
  ).fossilModules;
const prior = L("2.1.85-rebased");
const fresh = L("2.1.86");
const sig = (m) => m.hashes.join("|");

const p = prior.filter((m) => m.file.includes(STEM));
const f = fresh.filter((m) => m.file.includes(STEM));
console.log(`prior modules matching "${STEM}": ${p.length}`);
for (const m of p.slice(0, 5))
  console.log(
    `   ${m.file}  (${m.hashes.length} stmts, sig ${sig(m).slice(0, 24)}…)`
  );
console.log(`fresh modules matching "${STEM}": ${f.length}`);
for (const m of f.slice(0, 5))
  console.log(
    `   ${m.file}  (${m.hashes.length} stmts, sig ${sig(m).slice(0, 24)}…)`
  );

// Is the fresh module's content unique on each side?
for (const m of f.slice(0, 3)) {
  const k = sig(m);
  const pSame = prior.filter((x) => sig(x) === k);
  const fSame = fresh.filter((x) => sig(x) === k);
  console.log(`\nfresh ${m.file}:`);
  console.log(`  same-content modules in prior: ${pSame.length}`);
  for (const x of pSame.slice(0, 6)) console.log(`     ${x.file}`);
  console.log(`  same-content modules in fresh: ${fSame.length}`);
  for (const x of fSame.slice(0, 6)) console.log(`     ${x.file}`);
}
