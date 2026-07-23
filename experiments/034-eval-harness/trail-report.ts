/**
 * Full identifier-accounting table from a --diagnostics JSON: the ledger
 * (TOTAL at top, REMAINING at bottom), the per-strategy attempt funnel
 * with top refusal reasons, clobber flags, and the LLM endgame.
 *
 *   npx tsx trail-report.ts <diagnostics.json>
 */
import * as fs from "node:fs";

interface Attempt {
  strategy: string;
  outcome: string;
  reason?: string;
}
interface TrailEntry {
  oldName: string;
  loc: string;
  trail: Attempt[];
  settledBy?: string;
  postSettleAttempts: number;
}

const fmt = (n: number) => n.toLocaleString("en-US");
const pct = (n: number, total: number) =>
  total > 0 ? `${((n / total) * 100).toFixed(2)}%` : "-";

function main() {
  const diag = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
  const ledger = diag.identifierLedger;
  const trails: TrailEntry[] = diag.strategyTrails?.trails ?? [];
  if (!ledger) {
    console.error("no identifierLedger — re-run the pipeline with --diagnostics");
    process.exit(1);
  }

  const total: number = ledger.totalBindings ?? 0;
  const settled = Object.entries(ledger.transferSettled as Record<string, number>)
    .sort((a, b) => b[1] - a[1]);
  const transferSubtotal = settled.reduce((sum, [, n]) => sum + n, 0);

  console.log("=== identifier ledger ===");
  console.log(`TOTAL bindings in output: ${fmt(total)}`);
  for (const [strategy, n] of settled) {
    console.log(
      `  ${strategy.padEnd(22)} ${fmt(n).padStart(9)}  ${pct(n, total).padStart(7)}`
    );
  }
  console.log(
    `  ${"transfer subtotal".padEnd(22)} ${fmt(transferSubtotal).padStart(9)}  ${pct(transferSubtotal, total).padStart(7)}`
  );
  console.log(
    `  ${"llm".padEnd(22)} ${fmt(ledger.llmNamed).padStart(9)}  ${pct(ledger.llmNamed, total).padStart(7)}`
  );
  if (ledger.libraryPrefix)
    console.log(`  ${"library-prefix".padEnd(22)} ${fmt(ledger.libraryPrefix).padStart(9)}`);
  if (ledger.fallback)
    console.log(`  ${"fallback".padEnd(22)} ${fmt(ledger.fallback).padStart(9)}`);
  console.log(
    `  ${"llm-unrenamed".padEnd(22)} ${fmt(ledger.notRenamed).padStart(9)}`
  );
  console.log(
    `REMAINING still-minted: ${fmt(ledger.remainingMinted ?? 0)}  (${pct(ledger.remainingMinted ?? 0, total)})`
  );

  if (trails.length > 0) {
    // Funnel with per-strategy outcome counts + top refusal reasons.
    const funnel = new Map<
      string,
      { counts: Map<string, number>; reasons: Map<string, number> }
    >();
    for (const entry of trails) {
      for (const a of entry.trail) {
        let f = funnel.get(a.strategy);
        if (!f) {
          f = { counts: new Map(), reasons: new Map() };
          funnel.set(a.strategy, f);
        }
        f.counts.set(a.outcome, (f.counts.get(a.outcome) ?? 0) + 1);
        if (a.reason && a.outcome !== "vote") {
          f.reasons.set(a.reason, (f.reasons.get(a.reason) ?? 0) + 1);
        }
      }
    }
    console.log("\n=== attempt funnel (per strategy) ===");
    console.log(
      `${"strategy".padEnd(18)} ${"applied".padStart(9)} ${"rejected".padStart(9)} ${"abstained".padStart(10)} ${"vote-routed".padStart(12)}  top refusal reasons`
    );
    const order = [...funnel.entries()].sort(
      (a, b) =>
        (b[1].counts.get("applied") ?? 0) - (a[1].counts.get("applied") ?? 0)
    );
    for (const [strategy, f] of order) {
      const reasons = [...f.reasons.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, 3)
        .map(([r, n]) => `${r} ${n}`)
        .join(", ");
      console.log(
        `${strategy.padEnd(18)} ${fmt(f.counts.get("applied") ?? 0).padStart(9)} ` +
          `${fmt(f.counts.get("rejected") ?? 0).padStart(9)} ` +
          `${fmt(f.counts.get("abstained") ?? 0).padStart(10)} ` +
          `${fmt(f.counts.get("vote") ?? 0).padStart(12)}  ${reasons}`
      );
    }

    const clobbers = trails.filter((t) => t.postSettleAttempts > 0);
    console.log(
      `\npost-settle rename attempts (phase-order flags): ${clobbers.length} bindings`
    );
    for (const c of clobbers.slice(0, 5)) {
      console.log(
        `  ${c.oldName} @${c.loc} settled by ${c.settledBy}, +${c.postSettleAttempts} later attempt(s)`
      );
    }
  }

  const un = diag.unrenamed;
  if (un) {
    console.log("\n=== llm endgame (identifiers that reached the LLM) ===");
    console.log(`  renamed: ${fmt(diag.renamed?.length ?? 0)}`);
    console.log(
      `  unchanged ${fmt(un.unchanged?.length ?? 0)} · missing ${fmt(un.missing?.length ?? 0)} · duplicate ${fmt(un.duplicate?.length ?? 0)} · invalid ${fmt(un.invalid?.length ?? 0)}`
    );
  }
}

main();
