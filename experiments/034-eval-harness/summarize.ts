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
    relocations: {
      sameNameMovedFile: number;
      novelNames: number;
      freshNames: number;
    };
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

/** "count (pct%)" of a denominator, right-padded to width w. */
function cp(n: number, total: number, w: number): string {
  const pct = total ? ((100 * n) / total).toFixed(1) : "0.0";
  return `${n} (${pct}%)`.padStart(w);
}

function main() {
  const model = process.argv[2];
  if (!model) throw new Error("usage: summarize.ts <model-label>");
  const dir = path.join(import.meta.dirname, "results", model);
  const cards = loadScorecards(dir);
  if (cards.length === 0) throw new Error(`no scorecards in ${dir}`);

  const totals = {
    stmts: 0,
    unchangedClean: 0,
    unchangedChurned: 0,
    namingNoiseLines: 0,
    novel: 0,
    realLines: 0,
    sameNameMovedFile: 0,
    novelNames: 0,
    freshNames: 0,
    mintedLeftovers: 0
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
  }

  const summary = { model, pairs: cards, totals };
  fs.writeFileSync(
    path.join(dir, "summary.json"),
    JSON.stringify(summary, null, 2)
  );

  // Console table. clean/noise/novel are shown as `count (% of stmts)`;
  // reloc/newName as `count (% of the ledger's declared names)`.
  console.log(`\n=== eval: ${model} ===`);
  console.log(
    "clean/noise/novel = % of stmts · reloc/newName = % of names · " +
      "noise+reloc+mints are the reducible KPIs to drive to 0"
  );
  const head = [
    "pair".padEnd(16),
    pad("stmts", 7),
    pad("clean", 15),
    pad("noise", 14),
    pad("novel", 13),
    pad("noiseLn", 8),
    pad("realLn", 8),
    pad("reloc", 13),
    pad("newName", 14),
    pad("mints", 6),
    pad("%det", 6),
    pad("%llm", 6)
  ].join(" ");
  console.log(head);
  console.log("-".repeat(head.length));
  const row = (
    label: string,
    st: number,
    clean: number,
    noise: number,
    novel: number,
    noiseLn: number,
    realLn: number,
    reloc: number,
    newName: number,
    names: number,
    mints: number,
    pdet: string,
    pllm: string
  ) =>
    [
      label.padEnd(16),
      pad(st || "", 7),
      cp(clean, st, 15),
      cp(noise, st, 14),
      cp(novel, st, 13),
      pad(noiseLn, 8),
      pad(realLn, 8),
      cp(reloc, names, 13),
      cp(newName, names, 14),
      pad(mints, 6),
      pad(pdet, 6),
      pad(pllm, 6)
    ].join(" ");
  for (const c of cards) {
    const s = c.churn.statements;
    const r = c.churn.relocations;
    const d = c.determinism.functions;
    console.log(
      row(
        c.pair,
        s.total,
        s.unchangedClean,
        s.unchangedChurned,
        s.novel,
        c.churn.lines.namingNoiseLines,
        c.churn.lines.realLines,
        r.sameNameMovedFile,
        r.novelNames,
        r.freshNames,
        c.determinism.mintedLeftovers,
        String(d.pctDeterministic),
        String(d.pctReachingLLM)
      )
    );
  }
  console.log("-".repeat(head.length));
  console.log(
    row(
      "TOTAL",
      totals.stmts,
      totals.unchangedClean,
      totals.unchangedChurned,
      totals.novel,
      totals.namingNoiseLines,
      totals.realLines,
      totals.sameNameMovedFile,
      totals.novelNames,
      totals.freshNames,
      totals.mintedLeftovers,
      "",
      ""
    )
  );
  console.log(`\nwrote ${path.join(dir, "summary.json")}`);
}

main();
