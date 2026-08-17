/**
 * exp074: does the fossil matcher recover a module that changed slightly?
 *
 *   npx tsx experiments/074-path-stability/match-probe.mts
 *
 * The dominant path-churn target (access-property.js, 3,204 churned
 * require lines) went 17 → 18 statements between releases: overlap 0.94
 * with an unchanged importer set. Tier B (edge-corroborated, overlap
 * ≥ 0.5) should recover it. This asks the real matcher whether it does.
 */
import * as fs from "node:fs";
import { matchFossilModules } from "../../src/split/fossil-match.js";

const L = (v: string) =>
  JSON.parse(
    fs.readFileSync(
      `/tmp/eval-work/exp070-r1/${v}/.humanify/split-ledger.json`,
      "utf8"
    )
  ).fossilModules as { file: string; hashes: string[]; imports: number[] }[];

const prior = L("2.1.85-rebased");
const fresh = L("2.1.86");
const { matches, tiers } = matchFossilModules(
  prior.map((m) => ({ hashes: m.hashes, imports: m.imports })),
  fresh.map((m) => ({ hashes: m.hashes, imports: m.imports }))
);
console.log(`matches: ${matches.size} of ${fresh.length} fresh modules`);
console.log(`tiers: ${JSON.stringify(tiers)}`);

const targetIdx = fresh.findIndex((m) => m.file.includes("access-property"));
const priorIdx = prior.findIndex((m) => m.file.includes("access-property"));
console.log(`\ntarget fresh[${targetIdx}] = ${fresh[targetIdx]?.file}`);
console.log(`target prior[${priorIdx}] = ${prior[priorIdx]?.file}`);
const got = matches.get(targetIdx);
console.log(
  got === undefined
    ? `MATCHER RESULT: unmatched → mints a fresh path (this is the 3,204-line churn)`
    : `MATCHER RESULT: matched to prior[${got}] = ${prior[got].file}` +
        (got === priorIdx
          ? " (CORRECT — path would be inherited)"
          : " (WRONG partner)")
);

// How many importers does it have, and did they match?
const importers = fresh.filter((m) => m.imports.includes(targetIdx)).length;
console.log(`fresh importers of the target: ${importers}`);

// Overlap with its prior counterpart
const ca = new Map<string, number>();
for (const h of prior[priorIdx].hashes) ca.set(h, (ca.get(h) ?? 0) + 1);
let inter = 0;
for (const h of fresh[targetIdx].hashes) {
  const n = ca.get(h) ?? 0;
  if (n > 0) {
    inter++;
    ca.set(h, n - 1);
  }
}
const union =
  prior[priorIdx].hashes.length + fresh[targetIdx].hashes.length - inter;
console.log(
  `content overlap with prior counterpart: ${(inter / union).toFixed(3)} (${inter} shared of ${union})`
);
