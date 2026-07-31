/**
 * 054 — the pinned A/B verdict, judged PER HOP.
 *
 *   npx tsx pinned-report.ts <resultsDir> "<from:to> <from:to> ..."
 *
 * Gate criteria (041/gate-verdict.sh, restated here because the legs are
 * pinned-run labels rather than eval-harness sweep labels):
 *
 *   1. GIT CHURN down on EVERY hop, measured directly with `diff -r` against
 *      the prior tree — the number a reviewer actually sees, and the only one
 *      here that does not pass through a decomposition.
 *   2. `layout.noise` down on EVERY hop.
 *   3. `lines.realLines` and `statements.novel` unmoved — necessary, NOT
 *      sufficient: they come from a hash classifier that cannot see a pairing
 *      error, which is why task 1 read the survivors instead.
 *   4. `vendor.*` identical — the pass must not reach outside the ledger.
 *   5. `tree.relocatedStatements` must not rise.
 *
 * TWO COLUMNS ARE `~` — a move in them means nothing on its own, and saying so
 * up front is the point of rule 3. Both are artifacts of `composeDiff`'s
 * bookkeeping reacting to text that got more similar, not of behaviour:
 *
 *   `layout.real`    step 3 charges an edited statement only the lines a line
 *                    diff prints, once its token overlap with the removed prior
 *                    statement clears 50%. A rename that restores the prior
 *                    name raises that overlap, so the SAME edit is charged
 *                    fewer lines. Real change did not shrink — `realLn` and
 *                    `novel`, which come from a different classifier, are the
 *                    guard for that and must be exactly 0.
 *   `layout.reorder` a restored name can promote a statement from
 *                    "same hash, different text" to an EXACT (hash+text) match,
 *                    which adds it to the sequence the reorder LCS runs over.
 *                    Single digits either way is that promotion, not movement.
 */
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

const [
  RESULTS,
  PAIRS_SPEC = "",
  WORK = "/work",
  PRIORS = "/work/exp050-cold",
  TAG = "exp054"
] = process.argv.slice(2);
if (!RESULTS) {
  console.error('usage: pinned-report.ts <resultsDir> "<from:to> ..."');
  process.exit(1);
}

interface Churn {
  statements: { novel: number; unchangedChurned: number };
  lines: { namingNoiseLines: number; realLines: number };
  layout?: {
    churnLines: number;
    noise: number;
    naming: number;
    alias: number;
    reorder: number;
    real: number;
  };
  vendor?: { churnLines: number; noise: number; real: number };
  tree?: { relocatedStatements: number };
}

function load(label: string, to: string): Churn | null {
  const file = path.join(RESULTS, label, `${to}.json`);
  try {
    return JSON.parse(fs.readFileSync(file, "utf8")).churn as Churn;
  } catch {
    return null;
  }
}

/** Lines `diff -r` prints between two trees — the reviewer-facing number,
 * measured directly rather than through any decomposition. */
function gitChurn(priorSrc: string, freshSrc: string): number | null {
  if (!fs.existsSync(priorSrc) || !fs.existsSync(freshSrc)) return null;
  try {
    execFileSync("diff", ["-r", priorSrc, freshSrc], { maxBuffer: 1 << 30 });
    return 0; // identical: diff exits 0 and prints nothing
  } catch (err) {
    const out = String((err as { stdout?: Buffer }).stdout ?? "");
    return out.split("\n").filter((l) => /^[<>]/.test(l)).length;
  }
}

const ROWS: Array<{
  key: string;
  get: (c: Churn) => number;
  dir: "down" | "same" | "~";
}> = [
  {
    key: "layout.churnLines",
    get: (c) => c.layout?.churnLines ?? 0,
    dir: "down"
  },
  { key: "layout.noise", get: (c) => c.layout?.noise ?? 0, dir: "down" },
  { key: "layout.naming", get: (c) => c.layout?.naming ?? 0, dir: "down" },
  { key: "layout.alias", get: (c) => c.layout?.alias ?? 0, dir: "down" },
  { key: "layout.reorder ~", get: (c) => c.layout?.reorder ?? 0, dir: "~" },
  { key: "noiseLn", get: (c) => c.lines.namingNoiseLines, dir: "down" },
  { key: "noise(st)", get: (c) => c.statements.unchangedChurned, dir: "down" },
  { key: "layout.real ~", get: (c) => c.layout?.real ?? 0, dir: "~" },
  { key: "realLn", get: (c) => c.lines.realLines, dir: "same" },
  { key: "novel", get: (c) => c.statements.novel, dir: "same" },
  {
    key: "vendor.churnLines",
    get: (c) => c.vendor?.churnLines ?? 0,
    dir: "same"
  },
  { key: "vendor.noise", get: (c) => c.vendor?.noise ?? 0, dir: "same" },
  {
    key: "reloc(st)",
    get: (c) => c.tree?.relocatedStatements ?? 0,
    dir: "same"
  }
];

const pairs = PAIRS_SPEC.split(/\s+/).filter(Boolean);
let verdict = true;
const failures: string[] = [];

for (const spec of pairs) {
  const to = spec.includes(":") ? spec.split(":")[1] : spec;
  const off = load(`${TAG}-off-${to}`, to);
  const on = load(`${TAG}-on-${to}`, to);
  console.log(`\n=== ${spec} ===`);
  if (!off || !on) {
    console.log("  MISSING RESULT — leg did not complete");
    verdict = false;
    failures.push(`${spec}: missing leg`);
    continue;
  }
  const from = spec.includes(":") ? spec.split(":")[0] : "";
  const priorSrc = path.join(PRIORS, `${from}-rebased`, "src");
  const churnOff = gitChurn(
    priorSrc,
    path.join(WORK, `${TAG}-off-${to}`, "src")
  );
  const churnOn = gitChurn(priorSrc, path.join(WORK, `${TAG}-on-${to}`, "src"));
  if (churnOff !== null && churnOn !== null) {
    const delta = churnOn - churnOff;
    const bad = delta > 0;
    if (bad) {
      verdict = false;
      failures.push(`${spec}: GIT CHURN ${churnOff} -> ${churnOn}`);
    }
    console.log(
      `  ${"GIT CHURN (diff -r)".padEnd(20)}${String(churnOff).padStart(10)}${String(churnOn).padStart(11)}${String(delta).padStart(9)}${bad ? "  <-- UP (fails)" : ""}`
    );
  } else {
    console.log("  GIT CHURN (diff -r)   unavailable (tree missing)");
  }
  console.log(
    `  ${"metric".padEnd(20)}${"OFF".padStart(10)}${"ON".padStart(11)}${"delta".padStart(9)}`
  );
  for (const row of ROWS) {
    const a = row.get(off);
    const b = row.get(on);
    const delta = b - a;
    let mark = "";
    if (row.dir === "down" && delta > 0) mark = "  <-- UP (fails)";
    if (row.dir === "same" && delta !== 0) mark = "  <-- MOVED (fails)";
    if (mark) {
      verdict = false;
      failures.push(`${spec}: ${row.key} ${a} -> ${b}`);
    }
    console.log(
      `  ${row.key.padEnd(20)}${String(a).padStart(10)}${String(b).padStart(11)}${String(delta).padStart(9)}${mark}`
    );
  }
}

console.log(
  `\n################ VERDICT: ${verdict ? "PASS" : "FAIL"} ################`
);
for (const f of failures) console.log(`  ${f}`);
console.log(
  "\nNOTE: `realLn`/`novel` unmoved is NECESSARY, not sufficient — the hash\n" +
    "classifier cannot see a pairing error. The evidence against that is task 1,\n" +
    "where the survivors were read by hand and the one wrong class found was gated."
);
