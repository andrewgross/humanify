/**
 * Compare every model's eval scores side by side — the "which idea won" view.
 * Reads results/<model>/summary.json for each model (or the ones named).
 *
 *   npx tsx experiments/034-eval-harness/leaderboard.ts [model ...]
 *
 * With no args, lists every model that has a summary. A delta vs the
 * FIRST-listed model (the baseline) is shown in parens.
 *
 * Lower is NOT better on every column. Each header carries its direction from
 * the KPI registry (`kpis.ts`), because a column of digits cannot tell you on
 * its own whether a drop is a win — and on the two `=` columns, which measure
 * REAL code change, a drop is a regression.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import {
  caveatLines,
  type Kpi,
  kpisNamed,
  type SummaryTotals
} from "./kpis.js";
import { bandFor, loadNoiseBands } from "./noise-bands.js";

interface Summary {
  model: string;
  totals: Partial<SummaryTotals>;
}

/**
 * Columns in display order. The two `hold` KPIs (novel, realLn) are here on
 * purpose: the gate's rule is that reducible noise falls while real change does
 * NOT move, and a leaderboard that shows only the noise half lets a run that
 * dropped real code read as the winner.
 */
const COLUMNS = [
  // treeLn leads: it is the number a reviewer of the split tree actually sees.
  // exp054 removed 5,026 of these while `noise`/`noiseLn` barely moved.
  "treeLn",
  "noise",
  "noiseLn",
  "novel",
  "realLn",
  "reloc",
  "relocSt",
  "newName",
  "mints",
  "reorderLn",
  "vendorLn",
  "vendorReal"
];

/** Suffix marking which way is good, so the header is self-describing. */
const MARK: Record<Kpi["direction"], string> = {
  lower: "↓",
  higher: "↑",
  hold: "=",
  context: "~"
};

/**
 * Warn when the bands were measured at a DIFFERENT commit from the labels
 * they are being applied to.
 *
 * Both facts were already recorded and neither was ever consulted:
 * `noise-bands.json` carries `provenance.commit`, and every label carries
 * `commit.txt`. The committed bands were measured at `76c012b` — a
 * pre-fossil ~1,500-file tree where `relocSt` never moved, so its band came
 * out 0. Applied unchanged to a 3,274-file fossil tree, that 0 turns a
 * genuinely unmeasured +46 into a confident "outside the band" verdict.
 *
 * A band is a property of a REGIME, not of a repository. Changing the layout
 * changes the regime, and the bands must be re-measured there — which is the
 * only thing this can say, since it cannot know which commits changed what.
 */
function bandCommitWarning(
  resultsDir: string,
  models: string[],
  bands: ReturnType<typeof loadNoiseBands>
): string[] {
  if (!bands || bands.provenance.provisional) return [];
  const bandCommit = bands.provenance.commit;
  const labelCommits = new Map<string, string>();
  for (const m of models) {
    const p = path.join(resultsDir, m, "commit.txt");
    if (!fs.existsSync(p)) continue;
    labelCommits.set(m, fs.readFileSync(p, "utf8").trim().split(/\s/)[0]);
  }
  const foreign = [...labelCommits.entries()].filter(
    ([, c]) => c !== bandCommit
  );
  if (foreign.length === 0) return [];
  return [
    `  bands: MEASURED AT ${bandCommit}, which is NOT the commit of ` +
      `${foreign.map(([m, c]) => `${m}@${c}`).join(", ")}.`,
    "    A band is a property of a regime, not of a repo: if the layout or " +
      "the pipeline changed between those commits, these ± figures are not " +
      "this run's floor and a delta 'outside the band' may be unmeasured.",
    "    Re-measure with `npm run eval -- bands <r1> <r2>` on same-commit " +
      "cold repeats of the tree you are judging."
  ];
}

function main() {
  const resultsDir = path.join(import.meta.dirname, "results");
  let models = process.argv.slice(2);
  if (models.length === 0) {
    models = fs.existsSync(resultsDir)
      ? fs
          .readdirSync(resultsDir)
          .filter((m) =>
            fs.existsSync(path.join(resultsDir, m, "summary.json"))
          )
      : [];
  }
  if (models.length === 0) {
    console.log("no models with a summary.json (run run.sh <model> first)");
    return;
  }

  const summaries: Summary[] = models.map((m) =>
    JSON.parse(
      fs.readFileSync(path.join(resultsDir, m, "summary.json"), "utf8")
    )
  );
  const base = summaries[0].totals;
  const cols = kpisNamed(COLUMNS);

  // Band-aware delta: a difference the instrument cannot resolve renders
  // as `~0±band`, not a signed number — rule 11 made structural. `±?`
  // means the column's floor was NEVER measured; that is a claim of
  // ignorance, not of stability.
  const cell = (
    v: number | undefined,
    b: number | undefined,
    isBase: boolean,
    kpiKey: string
  ): string => {
    if (v === undefined) return "-";
    if (isBase || b === undefined) return String(v);
    const d = v - b;
    if (d === 0) return `${v} (=)`;
    const band = bandFor(kpiKey);
    if (band === null) return `${v} (${d > 0 ? `+${d}` : d}±?)`;
    if (Math.abs(d) <= band) return `${v} (~0±${band})`;
    return `${v} (${d > 0 ? `+${d}` : d})`;
  };

  console.log("\n=== eval leaderboard (totals across pairs) ===");
  const w = 18;
  console.log(
    [
      "model".padEnd(20),
      ...cols.map((k) => `${k.key}${MARK[k.direction]}`.padStart(w))
    ].join(" ")
  );
  console.log("-".repeat(20 + cols.length * (w + 1)));
  summaries.forEach((s, i) => {
    console.log(
      [
        s.model.padEnd(20),
        ...cols.map((k) =>
          cell(s.totals[k.total], base[k.total], i === 0, k.key).padStart(w)
        )
      ].join(" ")
    );
  });
  console.log(
    `\nbaseline = ${summaries[0].model} (first listed). ` +
      "↓ drive to zero · = REAL CODE CHANGE, must not move in either " +
      "direction · ~ a move means nothing on its own. A column is '-' on " +
      "models scored before that KPI existed."
  );
  for (const line of caveatLines(cols)) console.log(line);
  const bands = loadNoiseBands();
  for (const line of bandCommitWarning(resultsDir, models, bands)) {
    console.log(line);
  }
  if (bands?.provenance.provisional) {
    console.log(
      "  bands: PROVISIONAL (seeded from recorded measurements) — produce a " +
        "measured file with `npm run eval -- bands <r1> <r2> [r3]` over " +
        "same-commit cold repeats."
    );
  } else if (!bands) {
    console.log(
      "  bands: NONE — every nonzero delta prints ±?; no sub-floor claim is possible."
    );
  }
}

main();
