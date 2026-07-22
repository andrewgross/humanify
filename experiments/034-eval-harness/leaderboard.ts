/**
 * Compare every model's eval scores side by side — the "which idea won" view.
 * Reads results/<model>/summary.json for each model (or the ones named).
 *
 *   npx tsx experiments/034-eval-harness/leaderboard.ts [model ...]
 *
 * With no args, lists every model that has a summary. Lower is better on every
 * column; a delta vs the FIRST-listed model (the baseline) is shown in parens.
 */
import * as fs from "node:fs";
import * as path from "node:path";

interface Summary {
  model: string;
  totals: {
    unchangedChurned: number;
    namingNoiseLines: number;
    novel: number;
    realLines: number;
    sameNameMovedFile: number;
    novelNames: number;
    mintedLeftovers: number;
  };
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

  const cols: Array<[string, keyof Summary["totals"]]> = [
    ["noise", "unchangedChurned"],
    ["noiseLn", "namingNoiseLines"],
    ["reloc", "sameNameMovedFile"],
    ["newName", "novelNames"],
    ["mints", "mintedLeftovers"]
  ];

  const cell = (v: number, b: number, isBase: boolean): string => {
    if (isBase) return String(v);
    const d = v - b;
    const sign = d > 0 ? `+${d}` : `${d}`;
    return `${v} (${d === 0 ? "=" : sign})`;
  };

  console.log(
    "\n=== eval leaderboard (totals across pairs; lower is better) ==="
  );
  const w = 22;
  console.log(
    ["model".padEnd(20), ...cols.map(([h]) => h.padStart(w))].join(" ")
  );
  console.log("-".repeat(20 + cols.length * (w + 1)));
  summaries.forEach((s, i) => {
    console.log(
      [
        s.model.padEnd(20),
        ...cols.map(([, k]) => cell(s.totals[k], base[k], i === 0).padStart(w))
      ].join(" ")
    );
  });
  console.log(
    `\nbaseline = ${summaries[0].model} (first listed). noiseLn carries the LLM floor.`
  );
}

main();
