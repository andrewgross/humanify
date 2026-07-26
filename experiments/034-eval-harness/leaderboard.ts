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
  "noise",
  "noiseLn",
  "novel",
  "realLn",
  "reloc",
  "relocSt",
  "newName",
  "mints",
  "reorderLn"
];

/** Suffix marking which way is good, so the header is self-describing. */
const MARK: Record<Kpi["direction"], string> = {
  lower: "↓",
  higher: "↑",
  hold: "=",
  context: "~"
};

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

  const cell = (
    v: number | undefined,
    b: number | undefined,
    isBase: boolean
  ): string => {
    if (v === undefined) return "-";
    if (isBase || b === undefined) return String(v);
    const d = v - b;
    const sign = d > 0 ? `+${d}` : `${d}`;
    return `${v} (${d === 0 ? "=" : sign})`;
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
          cell(s.totals[k.total], base[k.total], i === 0).padStart(w)
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
}

main();
