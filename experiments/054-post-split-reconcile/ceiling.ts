/**
 * 054 task 0 — the ceiling for a POST-SPLIT reconcile pass. No pipeline run,
 * no LLM call: the trees are already on disk (`/work/exp050-cold`).
 *
 *   npx tsx experiments/054-post-split-reconcile/ceiling.ts <priorSrc> <freshSrc> [label]
 *   NO_CONSUMER=1 ... prices the same pass with the consumer tier off.
 *
 * The mechanism lives in `pass.ts`; this is the report over a whole tree.
 * The number it prints is git-capped by construction — both sides are counted
 * from a real `diff` of two real texts, so it cannot over-charge the way a
 * decomposition can (051 measured its ledger over-charging by 29%).
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { type FileResult, runFile, walk } from "./pass.js";

const [PRIOR, FRESH, LABEL = ""] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: ceiling.ts <priorSrc> <freshSrc> [label]");
  process.exit(1);
}

const priorFiles = new Set(walk(PRIOR));
const results: FileResult[] = [];
const skipReasons = new Map<string, number>();
const hunks = { changed: 0, noise: 0, genuine: 0, oversized: 0, tainted: 0 };
const renameSamples: Array<{
  file: string;
  from: string;
  to: string;
  kind: string;
  votes: number;
  declLine: number;
}> = [];

for (const f of walk(FRESH)) {
  if (!priorFiles.has(f)) continue;
  const res = runFile(PRIOR, FRESH, f, false);
  results.push(res);
  for (const r of res.skipped) {
    skipReasons.set(r, (skipReasons.get(r) ?? 0) + 1);
  }
  hunks.changed += res.hunks.changed;
  hunks.noise += res.hunks.noise;
  hunks.genuine += res.hunks.genuine;
  hunks.oversized += res.hunks.oversized;
  hunks.tainted += res.hunks.tainted;
  if (res.status !== "ok") continue;
  for (const r of res.renames) {
    renameSamples.push({
      file: f,
      from: r.fromName,
      to: r.toName,
      kind: r.kind,
      votes: r.votes,
      declLine: r.declLine
    });
  }
}

const count = (s: FileResult["status"]) =>
  results.filter((r) => r.status === s).length;
const acted = results.filter((r) => r.status === "ok");
const savedOnly = acted
  .filter((o) => o.before > o.after)
  .reduce((n, o) => n + (o.before - o.after), 0);
const gained = acted
  .filter((o) => o.after > o.before)
  .reduce((n, o) => n + (o.after - o.before), 0);
const net = savedOnly - gained;
const totalBefore = results.reduce((n, o) => n + o.before, 0);
const pad = (n: number, w = 7) => String(n).padStart(w);

console.log(
  `=== 054 POST-SPLIT RECONCILE CEILING — ${LABEL || `${PRIOR} -> ${FRESH}`}` +
    `${process.env.NO_CONSUMER === "1" ? "  [consumer tier OFF]" : ""} ===`
);
console.log(`  files compared (same path both sides): ${results.length}`);
for (const s of [
  "identical",
  "corpus-gate",
  "parse-failed",
  "no-renames",
  "invariant-violated",
  "rewrite-unsound"
] as const) {
  console.log(`  ${s.padEnd(20)} ${pad(count(s))}`);
}
console.log(`  ${"RENAMED files".padEnd(20)} ${pad(acted.length)}`);
console.log("");
console.log(
  `  hunks: ${hunks.changed} changed, ${hunks.noise} clean-noise, ` +
    `${hunks.genuine} genuine, ${hunks.oversized} oversized, ${hunks.tainted} tainted`
);
console.log(`  renames applied: ${renameSamples.length}`);
console.log("");
console.log(`  diff lines BEFORE (all common files): ${pad(totalBefore)}`);
console.log(`  lines REMOVED by the pass:            ${pad(savedOnly)}`);
console.log(`  lines CREATED by the pass:            ${pad(gained)}`);
console.log(`  NET git-capped ceiling:               ${pad(net)}`);
console.log(
  `ROW|${LABEL}|${totalBefore}|${savedOnly}|${gained}|${net}|${renameSamples.length}`
);

console.log(`\n  skip reasons (top 15):`);
for (const [r, n] of [...skipReasons.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 15)) {
  console.log(`    ${pad(n, 6)}  ${r}`);
}

console.log(`\n  files with the largest saving:`);
for (const o of acted
  .filter((x) => x.before !== x.after)
  .sort((a, b) => b.before - b.after - (a.before - a.after))
  .slice(0, 15)) {
  console.log(
    `    ${pad(o.before - o.after, 6)} ln  (${o.before} -> ${o.after}, ${o.renames.length} renames)  ${o.file}`
  );
}

const suffix = process.env.NO_CONSUMER === "1" ? "-noconsumer" : "";
fs.writeFileSync(
  path.join(
    path.dirname(new URL(import.meta.url).pathname),
    `renames-${(LABEL || "run").replace(/[^\w]+/g, "_")}${suffix}.json`
  ),
  JSON.stringify(
    {
      label: LABEL,
      totalBefore,
      savedOnly,
      gained,
      net,
      renames: renameSamples,
      perFile: acted
        .filter((o) => o.before !== o.after)
        .map((o) => ({
          file: o.file,
          before: o.before,
          after: o.after,
          renames: o.renames.length
        }))
    },
    null,
    2
  )
);
