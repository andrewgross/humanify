/**
 * Aggregate one model's per-pair scorecards into results/<model>/summary.json
 * and print a table. Reads every scorecard analyze.ts wrote for the model.
 *
 *   npx tsx experiments/034-eval-harness/summarize.ts <model-label>
 */
import * as fs from "node:fs";
import * as path from "node:path";

interface Scorecard {
  pair: string;
  determinism: {
    functions: {
      total: number;
      deterministic: number;
      closeMatchLLM: number;
      coldLLM: number;
      pctDeterministic: number;
      pctReachingLLM: number;
    };
    mintedLeftovers: number;
  };
  churn: {
    statements: {
      total: number;
      unchangedClean: number;
      unchangedChurned: number;
      novel: number;
    };
    lines: { namingNoiseLines: number; realLines: number };
    relocations: { sameNameMovedFile: number; novelNames: number };
  };
}

function loadScorecards(dir: string): Scorecard[] {
  const cards: Scorecard[] = [];
  for (const f of fs.readdirSync(dir)) {
    if (
      !f.endsWith(".json") ||
      f.endsWith(".stats.json") ||
      f === "summary.json"
    )
      continue;
    try {
      const j = JSON.parse(fs.readFileSync(path.join(dir, f), "utf8"));
      if (j && typeof j.pair === "string") cards.push(j);
    } catch {
      /* not a scorecard */
    }
  }
  return cards.sort((a, b) =>
    a.pair.localeCompare(b.pair, undefined, { numeric: true })
  );
}

function pad(s: string | number, w: number): string {
  return String(s).padStart(w);
}

function main() {
  const model = process.argv[2];
  if (!model) throw new Error("usage: summarize.ts <model-label>");
  const dir = path.join(import.meta.dirname, "results", model);
  const cards = loadScorecards(dir);
  if (cards.length === 0) throw new Error(`no scorecards in ${dir}`);

  const totals = {
    unchangedChurned: 0,
    namingNoiseLines: 0,
    novel: 0,
    realLines: 0,
    sameNameMovedFile: 0,
    novelNames: 0,
    mintedLeftovers: 0
  };
  for (const c of cards) {
    totals.unchangedChurned += c.churn.statements.unchangedChurned;
    totals.namingNoiseLines += c.churn.lines.namingNoiseLines;
    totals.novel += c.churn.statements.novel;
    totals.realLines += c.churn.lines.realLines;
    totals.sameNameMovedFile += c.churn.relocations.sameNameMovedFile;
    totals.novelNames += c.churn.relocations.novelNames;
    totals.mintedLeftovers += c.determinism.mintedLeftovers;
  }

  const summary = { model, pairs: cards, totals };
  fs.writeFileSync(
    path.join(dir, "summary.json"),
    JSON.stringify(summary, null, 2)
  );

  // Console table. NOISE columns (churn/noiseLn/reloc/mints) are the KPIs to
  // drive down; %det/%llm explain the determinism ceiling.
  console.log(`\n=== eval: ${model} ===`);
  const head = [
    "pair".padEnd(16),
    pad("stmts", 7),
    pad("noise", 6),
    pad("noiseLn", 8),
    pad("novel", 6),
    pad("realLn", 8),
    pad("reloc", 6),
    pad("newName", 8),
    pad("mints", 6),
    pad("%det", 6),
    pad("%llm", 6)
  ].join(" ");
  console.log(head);
  console.log("-".repeat(head.length));
  for (const c of cards) {
    console.log(
      [
        c.pair.padEnd(16),
        pad(c.churn.statements.total, 7),
        pad(c.churn.statements.unchangedChurned, 6),
        pad(c.churn.lines.namingNoiseLines, 8),
        pad(c.churn.statements.novel, 6),
        pad(c.churn.lines.realLines, 8),
        pad(c.churn.relocations.sameNameMovedFile, 6),
        pad(c.churn.relocations.novelNames, 8),
        pad(c.determinism.mintedLeftovers, 6),
        pad(c.determinism.functions.pctDeterministic, 6),
        pad(c.determinism.functions.pctReachingLLM, 6)
      ].join(" ")
    );
  }
  console.log("-".repeat(head.length));
  console.log(
    [
      "TOTAL".padEnd(16),
      pad("", 7),
      pad(totals.unchangedChurned, 6),
      pad(totals.namingNoiseLines, 8),
      pad(totals.novel, 6),
      pad(totals.realLines, 8),
      pad(totals.sameNameMovedFile, 6),
      pad(totals.novelNames, 8),
      pad(totals.mintedLeftovers, 6),
      pad("", 6),
      pad("", 6)
    ].join(" ")
  );
  console.log(`\nwrote ${path.join(dir, "summary.json")}`);
  console.log(
    "KPIs to drive to 0: noise (churned stmts) · reloc (same-name file moves) · " +
      "mints. noiseLn carries the LLM floor — read it against %llm."
  );
}

main();
