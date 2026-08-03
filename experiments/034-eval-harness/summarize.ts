/**
 * Aggregate one model's per-pair scorecards into results/<model>/summary.json
 * and print a table. Reads every scorecard analyze.ts wrote for the model.
 *
 *   npx tsx experiments/034-eval-harness/summarize.ts <model-label>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import {
  caveatLines,
  type Kpi,
  kpisNamed,
  type Scorecard,
  type SummaryTotals
} from "./kpis.js";
import { loadRunStatuses, runStatusBanner } from "../lib/invariants.js";
import { isScorecardShape } from "../lib/run-manifest.js";

function loadScorecards(dir: string): Scorecard[] {
  const cards: Scorecard[] = [];
  for (const f of fs.readdirSync(dir)) {
    if (!f.endsWith(".json") || f === "summary.json") continue;
    // The try covers ONLY the parse. It used to wrap the classification too,
    // so when a missing import made the predicate throw, every card was
    // silently rejected and the summary reported "no scorecards" — a
    // programming error disguised as an empty results directory.
    let parsed: unknown;
    try {
      parsed = JSON.parse(fs.readFileSync(path.join(dir, f), "utf8"));
    } catch {
      continue; /* not JSON at all */
    }
    // Shape, not filename. This used to accept "any .json with a string
    // `pair`", which the run manifest also satisfies — the summarizer then
    // read `churn.statements` off a manifest and crashed the whole eval
    // AFTER a 421-second pipeline run had already succeeded.
    if (isScorecardShape(parsed)) cards.push(parsed as Scorecard);
  }
  return cards.sort((a, b) =>
    a.pair.localeCompare(b.pair, undefined, { numeric: true })
  );
}

function pad(s: string | number, w: number): string {
  return String(s).padStart(w);
}

/** "count (pct%)" of a denominator, right-padded to width w. */
function cp(n: number, total: number, w: number): string {
  const pct = total ? ((100 * n) / total).toFixed(1) : "0.0";
  return `${n} (${pct}%)`.padStart(w);
}

/**
 * The churn table's columns, in display order, with the width each has always
 * been rendered at. The KPI itself — where it comes from, which way is good,
 * what misleads about it — lives in `kpis.ts`; this only says how wide.
 */
const CHURN_COLUMNS: Array<[key: string, width: number]> = [
  ["clean", 15],
  ["noise", 14],
  ["novel", 13],
  ["noiseLn", 8],
  ["realLn", 8],
  ["reloc", 13],
  ["newName", 14],
  ["mints", 6]
];

/**
 * One table row. Each cell comes from the KPI's own descriptor — its
 * denominator decides `count (pct%)` versus a bare count — so a KPI added to
 * `kpis.ts` and named in CHURN_COLUMNS renders without touching this function.
 */
function churnRow(
  label: string,
  shown: Kpi[],
  value: (k: Kpi) => number | undefined,
  ctx: {
    statements: number;
    names: number;
    stmts: number;
    det: string;
    llm: string;
  }
): string {
  const cells = shown.map((k, i) => {
    const w = CHURN_COLUMNS[i][1];
    const v = value(k);
    if (v === undefined) return pad("-", w);
    return k.denominator ? cp(v, ctx[k.denominator], w) : pad(v, w);
  });
  return [
    label.padEnd(16),
    pad(ctx.stmts || "", 7),
    ...cells,
    pad(ctx.det, 6),
    pad(ctx.llm, 6)
  ].join(" ");
}

function main() {
  const model = process.argv[2];
  if (!model) throw new Error("usage: summarize.ts <model-label>");
  const dir = path.join(import.meta.dirname, "results", model);
  const cards = loadScorecards(dir);
  if (cards.length === 0) throw new Error(`no scorecards in ${dir}`);

  const totals: SummaryTotals = {
    stmts: 0,
    unchangedClean: 0,
    unchangedChurned: 0,
    namingNoiseLines: 0,
    novel: 0,
    realLines: 0,
    sameNameMovedFile: 0,
    novelNames: 0,
    freshNames: 0,
    mintedLeftovers: 0,
    relocatedStatements: 0,
    layoutChurnLines: 0,
    layoutReal: 0,
    layoutNoise: 0,
    layoutNaming: 0,
    layoutAlias: 0,
    layoutReorder: 0,
    vendorChurnLines: 0,
    vendorNoise: 0,
    vendorReal: 0
  };
  for (const c of cards) {
    totals.stmts += c.churn.statements.total;
    totals.unchangedClean += c.churn.statements.unchangedClean;
    totals.unchangedChurned += c.churn.statements.unchangedChurned;
    totals.namingNoiseLines += c.churn.lines.namingNoiseLines;
    totals.novel += c.churn.statements.novel;
    totals.realLines += c.churn.lines.realLines;
    totals.sameNameMovedFile += c.churn.relocations.sameNameMovedFile;
    totals.novelNames += c.churn.relocations.novelNames;
    totals.freshNames += c.churn.relocations.freshNames;
    totals.mintedLeftovers += c.determinism.mintedLeftovers;
    totals.relocatedStatements += c.churn.tree?.relocatedStatements ?? 0;
    if (c.churn.layout) {
      totals.layoutChurnLines += c.churn.layout.churnLines;
      totals.layoutReal += c.churn.layout.real;
      totals.layoutNoise += c.churn.layout.noise;
      totals.layoutNaming += c.churn.layout.naming;
      totals.layoutAlias += c.churn.layout.alias;
      totals.layoutReorder += c.churn.layout.reorder;
    }
    if (c.churn.vendor) {
      totals.vendorChurnLines += c.churn.vendor.churnLines;
      totals.vendorNoise += c.churn.vendor.noise;
      totals.vendorReal += c.churn.vendor.real;
    }
  }

  // Whether the pipeline declared each run VALID, recorded by run.sh. Absent
  // for every result set produced before this existed — absent, not clean.
  const runStatuses = loadRunStatuses(dir);
  const banner = runStatusBanner(runStatuses);

  const summary = { model, pairs: cards, totals, runStatuses };
  fs.writeFileSync(
    path.join(dir, "summary.json"),
    JSON.stringify(summary, null, 2)
  );

  // Console table. clean/noise/novel are shown as `count (% of stmts)`;
  // reloc/newName as `count (% of the ledger's declared names)`.
  console.log(`\n=== eval: ${model} ===`);
  // Before the numbers, because a caveat printed after them is a caveat read
  // after the reader has already believed them.
  for (const line of banner) console.log(line);
  if (banner.length > 0) console.log("");
  console.log(
    "clean/noise/novel = % of stmts · reloc/newName = % of names · " +
      "noise+reloc+mints are the reducible KPIs to drive to 0"
  );
  const shown = kpisNamed(CHURN_COLUMNS.map(([k]) => k));
  const head = [
    "pair".padEnd(16),
    pad("stmts", 7),
    ...CHURN_COLUMNS.map(([k, w]) => pad(k, w)),
    pad("%det", 6),
    pad("%llm", 6)
  ].join(" ");
  console.log(head);
  console.log("-".repeat(head.length));
  for (const c of cards) {
    const d = c.determinism.functions;
    console.log(
      churnRow(c.pair, shown, (k) => k.fromCard(c), {
        statements: c.churn.statements.total,
        names: c.churn.relocations.freshNames,
        stmts: c.churn.statements.total,
        det: String(d.pctDeterministic),
        llm: String(d.pctReachingLLM)
      })
    );
  }
  console.log("-".repeat(head.length));
  console.log(
    churnRow("TOTAL", shown, (k) => totals[k.total], {
      statements: totals.stmts,
      names: totals.freshNames,
      stmts: totals.stmts,
      det: "",
      llm: ""
    })
  );
  for (const line of caveatLines(shown)) console.log(line);
  printLayout(cards, totals);
  console.log(`\nwrote ${path.join(dir, "summary.json")}`);
  // And again last, so it is the final thing on screen as well as the first.
  // A run whose pipeline rejected its own output must not be summarised by a
  // table of numbers and nothing else.
  if (banner.length > 0) {
    console.log("");
    for (const line of banner) console.log(line);
  } else if (runStatuses.length === 0) {
    console.log(
      "\nNOTE: no per-pair run status was recorded — this predates the check, " +
        "so whether the pipeline accepted its own output is UNKNOWN, not clean."
    );
  }
}

/**
 * What the on-disk diff of the split tree is made of, in GIT LINES — the view
 * the statement-level table above is blind to, because it matches by hash and
 * so cannot see a byte-identical statement emitted somewhere else. REORDER is
 * the column nothing else measures. TOTAL first.
 */
function printLayout(cards: Scorecard[], totals: SummaryTotals): void {
  const scored = cards.filter((c) => c.churn.layout);
  if (scored.length === 0) return;
  console.log("\n=== on-disk diff composition (git lines; EVAL_LAYOUT) ===");
  const head = [
    "pair".padEnd(16),
    pad("churn", 9),
    pad("real", 9),
    pad("noise", 9),
    pad("naming", 8),
    pad("alias", 7),
    pad("reorder", 9),
    pad("relocSt", 8)
  ].join(" ");
  console.log(head);
  console.log("-".repeat(head.length));
  const pct = (n: number, d: number) =>
    d ? `${((100 * n) / d).toFixed(1)}%` : "-";
  const row = (
    label: string,
    l: Scorecard["churn"]["layout"],
    relocSt: number
  ) => {
    if (!l) return "";
    return [
      label.padEnd(16),
      pad(l.churnLines, 9),
      pad(l.real, 9),
      pad(l.noise, 9),
      pad(l.naming, 8),
      pad(l.alias, 7),
      pad(`${l.reorder} ${pct(l.reorder, l.churnLines)}`, 9),
      pad(relocSt, 8)
    ].join(" ");
  };
  console.log(
    row(
      "TOTAL",
      {
        churnLines: totals.layoutChurnLines,
        real: totals.layoutReal,
        noise: totals.layoutNoise,
        naming: totals.layoutNaming,
        alias: totals.layoutAlias,
        reorder: totals.layoutReorder
      },
      totals.relocatedStatements
    )
  );
  console.log("-".repeat(head.length));
  for (const c of scored) {
    console.log(
      row(c.pair, c.churn.layout, c.churn.tree?.relocatedStatements ?? 0)
    );
  }
  console.log(
    "reorder = byte-identical statements emitted at a different position; " +
      "relocSt = statements that changed FILE (order-independent)"
  );
}

main();
