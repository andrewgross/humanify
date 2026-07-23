/**
 * Combined run report from a --diagnostics JSON: identifier accounting
 * (TOTAL at top, REMAINING at bottom), the per-strategy attempt funnel
 * with top refusal reasons, clobber flags, the LLM endgame — and, when
 * the fresh/prior outputs are supplied, the diff ledger (what drove
 * every diff line: real change vs naming-noise shapes vs file moves).
 *
 *   npx tsx trail-report.ts <diag.json> [out.html] \
 *     [fresh.js prior.js [freshLedger.json priorLedger.json]]
 *   open out.html
 */
import * as fs from "node:fs";
import { computeDiffLedger, type DiffLedger } from "./diff-ledger.js";

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

interface FunnelRow {
  strategy: string;
  applied: number;
  rejected: number;
  abstained: number;
  vote: number;
  reasons: string;
}

function buildFunnelRows(trails: TrailEntry[]): FunnelRow[] {
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
  return [...funnel.entries()]
    .sort(
      (a, b) =>
        (b[1].counts.get("applied") ?? 0) - (a[1].counts.get("applied") ?? 0)
    )
    .map(([strategy, f]) => ({
      strategy,
      applied: f.counts.get("applied") ?? 0,
      rejected: f.counts.get("rejected") ?? 0,
      abstained: f.counts.get("abstained") ?? 0,
      vote: f.counts.get("vote") ?? 0,
      reasons: [...f.reasons.entries()]
        .sort((a, b) => b[1] - a[1])
        .slice(0, 3)
        .map(([r, n]) => `${r} ${n}`)
        .join(", ")
    }));
}

function main() {
  const diag = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
  const [, , , htmlOut, freshPath, priorPath, freshLedger, priorLedger] =
    process.argv;
  const diffLedger =
    freshPath && priorPath
      ? computeDiffLedger(freshPath, priorPath, freshLedger, priorLedger)
      : undefined;
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

  if (diffLedger) {
    console.log("\n=== diff ledger (fresh-side lines; removals from prior) ===");
    console.log(`TOTAL diff line mass: ${fmt(diffLedger.totalDiff)}`);
    console.log(
      `  real change: added ${fmt(diffLedger.addedLn)} + removed ${fmt(diffLedger.removedLn)} = ${fmt(diffLedger.addedLn + diffLedger.removedLn)}`
    );
    console.log(`  naming noise: ${fmt(diffLedger.noiseLn)}`);
    for (const s of diffLedger.shapes) {
      console.log(`    ${s.shape.padEnd(16)} ${fmt(s.ln).padStart(9)} ln (${fmt(s.st)} st)`);
    }
    console.log(
      `    ${"family-bucket".padEnd(16)} ${fmt(diffLedger.familyLn).padStart(9)} ln (${fmt(diffLedger.familySt)} st)`
    );
    console.log(`  file moves: ${fmt(diffLedger.movedSt)} statement(s)`);
  }

  if (htmlOut) {
    fs.writeFileSync(
      htmlOut,
      renderHtml(diag, trails, total, settled, diffLedger)
    );
    console.log(`\nwrote ${htmlOut} — open it with: open ${htmlOut}`);
  }
}

function renderHtml(
  diag: { identifierLedger: Record<string, unknown> } & Record<string, unknown>,
  trails: TrailEntry[],
  total: number,
  settled: [string, number][],
  diffLedger?: DiffLedger
): string {
  const ledger = diag.identifierLedger as {
    llmNamed: number;
    notRenamed: number;
    remainingMinted?: number;
  };
  const transferSubtotal = settled.reduce((sum, [, n]) => sum + n, 0);
  const funnelRows = buildFunnelRows(trails);
  const clobbers = trails.filter((t) => t.postSettleAttempts > 0);
  const timestamp = (diag.timestamp as string) ?? "";
  const ledgerRows = settled
    .map(
      ([s, n]) =>
        `<tr><td>${s}</td><td class=n>${fmt(n)}</td><td class=n>${pct(n, total)}</td></tr>`
    )
    .join("");
  const funnelHtml = funnelRows
    .map(
      (r) =>
        `<tr><td>${r.strategy}</td><td class=n>${fmt(r.applied)}</td><td class=n>${fmt(r.rejected)}</td><td class=n>${fmt(r.abstained)}</td><td class=n>${fmt(r.vote)}</td><td class=reasons>${r.reasons}</td></tr>`
    )
    .join("");
  const clobberHtml = clobbers
    .slice(0, 20)
    .map(
      (c) =>
        `<tr><td>${c.oldName}</td><td>${c.loc}</td><td>${c.settledBy}</td><td class=n>${c.postSettleAttempts}</td></tr>`
    )
    .join("");
  const un = diag.unrenamed as Record<string, unknown[]> | undefined;
  return `<!doctype html>
<meta charset="utf-8">
<title>humanify identifier report</title>
<style>
  body { font: 14px/1.5 -apple-system, sans-serif; margin: 2rem auto; max-width: 70rem; padding: 0 1rem; color: #1a1a1a; }
  @media (prefers-color-scheme: dark) { body { background: #14161a; color: #d6d6d6; } th { background: #22252b; } tr:nth-child(even) td { background: #1a1d22; } .strip { background: #22252b; } }
  h1 { font-size: 1.3rem; } h2 { font-size: 1.05rem; margin-top: 2rem; }
  table { border-collapse: collapse; width: 100%; }
  th, td { text-align: left; padding: .3rem .6rem; border-bottom: 1px solid #8884; }
  th { background: #f0f0f0; }
  td.n { text-align: right; font-variant-numeric: tabular-nums; }
  td.reasons { color: #888; font-size: .85em; }
  .strip { display: flex; gap: 2rem; background: #f5f5f5; padding: .8rem 1rem; border-radius: 8px; margin: 1rem 0; flex-wrap: wrap; }
  .strip b { display: block; font-size: 1.3rem; }
  .total b { color: #2563eb; } .remaining b { color: #dc2626; }
  pre.mermaid { overflow-x: auto; }
</style>
<h1>Identifier accounting <small style="color:#888">${timestamp}</small></h1>
<div class="strip">
  <div class="total"><b>${fmt(total)}</b>TOTAL bindings</div>
  <div><b>${fmt(transferSubtotal)}</b>transfer-settled (${pct(transferSubtotal, total)})</div>
  <div><b>${fmt(ledger.llmNamed)}</b>LLM-named (${pct(ledger.llmNamed, total)})</div>
  <div class="remaining"><b>${fmt(ledger.remainingMinted ?? 0)}</b>REMAINING minted (${pct(ledger.remainingMinted ?? 0, total)})</div>
</div>
<h2>Ledger — who settled what</h2>
<table><tr><th>strategy</th><th>settled</th><th>of total</th></tr>${ledgerRows}
<tr><td><b>transfer subtotal</b></td><td class=n><b>${fmt(transferSubtotal)}</b></td><td class=n>${pct(transferSubtotal, total)}</td></tr>
<tr><td>llm</td><td class=n>${fmt(ledger.llmNamed)}</td><td class=n>${pct(ledger.llmNamed, total)}</td></tr>
<tr><td>llm-unrenamed</td><td class=n>${fmt(ledger.notRenamed)}</td><td class=n></td></tr>
<tr><td><b>REMAINING still-minted</b></td><td class=n><b>${fmt(ledger.remainingMinted ?? 0)}</b></td><td class=n>${pct(ledger.remainingMinted ?? 0, total)}</td></tr></table>
<h2>Attempt funnel</h2>
<table><tr><th>strategy</th><th>applied</th><th>rejected</th><th>abstained</th><th>vote-routed</th><th>top refusal reasons</th></tr>${funnelHtml}</table>
${renderDiffSection(diffLedger)}<h2>Post-settle flags (${clobbers.length} bindings)</h2>
<table><tr><th>name</th><th>loc</th><th>settled by</th><th>later attempts</th></tr>${clobberHtml}</table>
<h2>LLM endgame</h2>
<p>renamed ${fmt((diag.renamed as unknown[])?.length ?? 0)} · unchanged ${fmt(un?.unchanged?.length ?? 0)} · missing ${fmt(un?.missing?.length ?? 0)} · duplicate ${fmt(un?.duplicate?.length ?? 0)} · invalid ${fmt(un?.invalid?.length ?? 0)}</p>
`;
}

function renderDiffSection(ledger?: DiffLedger): string {
  if (!ledger) return "";
  const shapeRows = ledger.shapes
    .map(
      (s) =>
        `<tr><td>${s.shape}</td><td class=n>${fmt(s.ln)}</td><td class=n>${fmt(s.st)}</td><td class=n>${pct(s.ln, ledger.totalDiff)}</td></tr>`
    )
    .join("");
  return `<h2>Diff ledger — what drove the cross-version diff</h2>
<div class="strip">
  <div class="total"><b>${fmt(ledger.totalDiff)}</b>TOTAL diff line mass</div>
  <div><b>${fmt(ledger.addedLn + ledger.removedLn)}</b>real change (${pct(ledger.addedLn + ledger.removedLn, ledger.totalDiff)})</div>
  <div class="remaining"><b>${fmt(ledger.noiseLn)}</b>naming noise (${pct(ledger.noiseLn, ledger.totalDiff)})</div>
  <div><b>${fmt(ledger.movedSt)}</b>file moves</div>
</div>
<table><tr><th>bucket</th><th>lines</th><th>statements</th><th>of diff</th></tr>
<tr><td>real: added</td><td class=n>${fmt(ledger.addedLn)}</td><td class=n>${fmt(ledger.addedSt)}</td><td class=n>${pct(ledger.addedLn, ledger.totalDiff)}</td></tr>
<tr><td>real: removed</td><td class=n>${fmt(ledger.removedLn)}</td><td class=n>${fmt(ledger.removedSt)}</td><td class=n>${pct(ledger.removedLn, ledger.totalDiff)}</td></tr>
${shapeRows}
<tr><td>noise: family-bucket</td><td class=n>${fmt(ledger.familyLn)}</td><td class=n>${fmt(ledger.familySt)}</td><td class=n>${pct(ledger.familyLn, ledger.totalDiff)}</td></tr>
<tr><td>file moves</td><td class=n>—</td><td class=n>${fmt(ledger.movedSt)}</td><td class=n></td></tr></table>
<p style="color:#888">${fmt(ledger.cleanLn)} clean lines sit outside the diff. Statement accounting is exhaustive — nothing unattributed.</p>`;
}

main();
