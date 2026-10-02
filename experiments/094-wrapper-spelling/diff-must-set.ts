/**
 * exp094: the EXACT must-set delta on the real corpus dumps — which pairs
 * entered/left under the exp094 canonicalizer versus the exp092 erasure,
 * computed with the scorer's own mustPairs (unique-canonical pairing).
 * Usage: npx tsx diff-must-set.ts <runs-dir> <baseline-json> <package>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { canonical } from "../lib/match-truth/canonical.js";

/** score.ts's mustPairs, inlined: the scorer pairings this diff must use
 * exactly (the 094 record dir is outside knip's project set, so the
 * scorer's helper is not exported to it). */
function mustPairs(
  priorCanon: string[],
  freshCanon: string[]
): { must: Array<[number, number]>; duplicateClass: number } {
  const counts = (values: string[]) => {
    const m = new Map<string, number>();
    for (const v of values) m.set(v, (m.get(v) ?? 0) + 1);
    return m;
  };
  const pc = counts(priorCanon);
  const fc = counts(freshCanon);
  const firstFresh = new Map<string, number>();
  freshCanon.forEach((v, i) => {
    if (!firstFresh.has(v)) firstFresh.set(v, i);
  });
  const must: Array<[number, number]> = [];
  let duplicateClass = 0;
  priorCanon.forEach((c, i) => {
    if ((pc.get(c) ?? 0) === 1 && (fc.get(c) ?? 0) === 1) {
      must.push([i, firstFresh.get(c) ?? -1]);
    } else if ((pc.get(c) ?? 0) > 1 || (fc.get(c) ?? 0) > 1) {
      duplicateClass += 1;
    }
  });
  return { must, duplicateClass };
}

const runsDir = path.resolve(process.argv[2]);
const baselinePath = path.resolve(process.argv[3]);
const want = process.argv[4] ?? "lodash";

const FUNCTION_HEAD =
  /^(\s*(?:(?:var|let|const)\s+[A-Za-z_$][A-Za-z0-9_$]*\s*=\s*)?)(async\s+)?function\s*(\([^)]*\))\s*\{/;
/** The exp092 erasure, unconditional. */
function oldCanonical(slice: string): string {
  const head = slice.match(FUNCTION_HEAD);
  if (!head) return canonical(slice);
  return canonical(slice.replace(FUNCTION_HEAD, "$1$2$3 => {"));
}

const dump = JSON.parse(
  fs.readFileSync(path.join(runsDir, want, "dump.json"), "utf8")
);
const file = (dump as { files: unknown[] }).files[0] as {
  functions: {
    prior: { name: string; slice: string }[];
    fresh: { name: string; slice: string }[];
  };
};
const p = file.functions.prior;
const f = file.functions.fresh;
const pairsOf = (form: (s: string) => string): string[] => {
  const pp = p.map((r) => form(r.slice));
  const ff = f.map((r) => form(r.slice));
  return mustPairs(pp, ff).must.map(([i, j]) => `${p[i].name} -> ${f[j].name}`);
};
const before = new Set(pairsOf(oldCanonical));
const after = new Set(pairsOf(canonical));
for (const entered of [...after].filter((x) => !before.has(x))) {
  console.log("ENTERED:", entered);
}
for (const left of [...before].filter((x) => !after.has(x))) {
  console.log("LEFT:   ", left);
}
