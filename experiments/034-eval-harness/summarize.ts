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
import {
  loadPairVerdicts,
  loadRunStatuses,
  runStatusBanner,
  verdictBanner
} from "../lib/invariants.js";
import { isScorecardShape, loadManifests } from "../lib/run-manifest.js";

/** What produced a label's numbers, gathered from its per-pair manifests.
 * Sorted and de-duplicated so the JSON is stable and a mixed label is
 * obvious at a glance. Empty arrays for a label scored before manifests
 * existed — absent, not "the same as yours". */
export interface LabelProvenance {
  models: string[];
  endpoints: string[];
  reasoningEfforts: string[];
}

function labelProvenance(dir: string): LabelProvenance {
  const manifests = loadManifests(dir);
  const uniq = (xs: string[]) => [...new Set(xs)].sort();
  return {
    models: uniq(manifests.map((m) => m.config.model)),
    endpoints: uniq(manifests.map((m) => m.config.endpoint)),
    reasoningEfforts: uniq(manifests.map((m) => m.config.reasoningEffort))
  };
}

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

/**
 * Fold scorecards into the published totals.
 *
 * EXPORTED and pure so a guard can drive it with a fully-populated card and
 * check that every scored field reaches a total (`recorded-facts.test.ts`).
 * It lived inside `main` and was therefore untestable, which is how a field
 * can be measured every run and quietly totalled nowhere.
 */
export function summarizeCards(cards: Scorecard[]): {
  totals: SummaryTotals;
  treeChurnCards: number;
} {
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
    layoutBuildConstantLines: 0,
    layoutNameOnlyLines: 0,
    layoutReal: 0,
    layoutNoise: 0,
    layoutNaming: 0,
    layoutAlias: 0,
    layoutReorder: 0,
    vendorChurnLines: 0,
    vendorNoise: 0,
    vendorReal: 0
  };
  let treeChurnCards = 0;
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
    // `?? 0` makes an ABSENT tree-churn indistinguishable from a measured
    // zero, on the one column that reads "no statement moved file" — the
    // most reassuring thing this summary can say. Contributors are counted
    // so a partial total cannot pass as a complete one.
    if (c.churn.tree) {
      totals.relocatedStatements += c.churn.tree.relocatedStatements;
      treeChurnCards++;
    }
    if (c.churn.layout) {
      totals.layoutChurnLines += c.churn.layout.churnLines;
      totals.layoutBuildConstantLines += c.churn.layout.buildConstantLines ?? 0;
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
  return { totals, treeChurnCards };
}

function main() {
  const model = process.argv[2];
  if (!model) throw new Error("usage: summarize.ts <model-label>");
  const dir = path.join(import.meta.dirname, "results", model);
  const cards = loadScorecards(dir);
  if (cards.length === 0) throw new Error(`no scorecards in ${dir}`);
  const { totals, treeChurnCards } = summarizeCards(cards);

  // Whether the pipeline declared each run VALID, recorded by run.sh. Absent
  // for every result set produced before this existed — absent, not clean.
  const runStatuses = loadRunStatuses(dir);
  // Boot and self-hop verdicts were write-only until 2026-08-09 — the
  // reference labelled valid carried an unread self-hop violation. Every
  // recorded verdict now reaches the banner and the summary JSON.
  const verdicts = loadPairVerdicts(dir);
  const banner = [...runStatusBanner(runStatuses), ...verdictBanner(verdicts)];
  if (treeChurnCards !== cards.length) {
    banner.push(
      `NOTE: relocSt totals ${treeChurnCards} of ${cards.length} pairs — the ` +
        "rest recorded no tree churn, and their statements are missing from " +
        "the total rather than counted as zero."
    );
  }

  // WHAT PRODUCED these numbers, carried where a cross-label reader can see
  // it. The model and endpoint were recorded per pair and read by nothing
  // (recorded-facts.test.ts, 2026-08-15), so the leaderboard would compare a
  // label scored on one model against a label scored on another and print
  // confident deltas — the same failure as applying bands from a foreign
  // commit, one level up. A SET, not a value: a label whose pairs disagree is
  // itself mixed, which is worth seeing.
  const summary = {
    model,
    provenance: labelProvenance(dir),
    pairs: cards,
    totals,
    runStatuses,
    verdicts
  };
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
 *
 * `nameOnly` is the LINE-level naming truth; `naming` beside it only counts
 * renames in statements whose hash did not flip, so it reads ~6x lower.
 *
 * **`churnEXB` IS THE NUMBER TO JUDGE A LEVER ON**, not `churn`. `bldConst` is
 * a build-metadata literal the bundler inlined at 216 sites in 83 files; three
 * of its fields change every release, so ~1,300 lines of every diff are one
 * fact repeated. On a CALM release that is 82% of the whole diff. It is
 * correctly charged to `real` (the values did change), which is exactly why it
 * hides — no noise KPI can see it, and no lever will ever move it.
 */
function printLayout(cards: Scorecard[], totals: SummaryTotals): void {
  const scored = cards.filter((c) => c.churn.layout);
  if (scored.length === 0) return;
  console.log("\n=== on-disk diff composition (git lines; EVAL_LAYOUT) ===");
  const head = [
    "pair".padEnd(16),
    pad("churn", 9),
    pad("churnEXB", 9),
    pad("bldConst", 9),
    pad("real", 9),
    pad("noise", 9),
    pad("naming", 8),
    pad("nameOnly", 9),
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
      pad(l.churnLinesExBuild ?? l.churnLines, 9),
      pad(l.buildConstantLines ?? 0, 9),
      pad(l.real, 9),
      pad(l.noise, 9),
      pad(l.naming, 8),
      pad(l.nameOnlyLines ?? 0, 9),
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
        churnLinesExBuild:
          totals.layoutChurnLines - totals.layoutBuildConstantLines,
        buildConstantLines: totals.layoutBuildConstantLines,
        real: totals.layoutReal,
        noise: totals.layoutNoise,
        naming: totals.layoutNaming,
        nameOnlyLines: totals.layoutNameOnlyLines,
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

// Only when RUN, not when imported. `summarizeCards` is exported so a guard
// can drive the totalling with a synthetic card, and a bare `main()` here
// meant importing this file executed the CLI and threw on the missing
// argument — the module could be depended on only by not depending on it.
// Same idiom as invariants.ts.
if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(import.meta.filename)
) {
  main();
}
