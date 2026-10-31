/**
 * exp170: re-score a label's EXISTING trees with the split-aware analyze.ts
 * (scorer only — nothing re-runs) and prove the old fields byte-identical.
 *
 *   npx tsx experiments/170-app-vendor-split/rescore.ts <label> \
 *     --cards <dir holding the label's cards>   (read-only)
 *     [--work /tmp/eval-work/<label>]           (the trees; default shown)
 *     [--stats-from-diag]                        (no stats.json: use the
 *                                                 diag's identical coverage)
 *     [--pairs 2.1.85->2.1.86,...]               (no cards: score these)
 *
 * Writes the re-scored cards (and copies of the label's run records, so
 * `summarize.ts` reads it) to THIS checkout's
 * `experiments/034-eval-harness/results/<label>/`. For every pair with an
 * old card it prints `OLD-FIELDS IDENTICAL` iff the new card, with
 * `churn.surfaces` removed, serializes to the old card's exact bytes; and it
 * checks every partition (app + vendor + cross = the old field).
 */
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import type { Scorecard } from "../034-eval-harness/kpis.js";

const HARNESS = path.join(import.meta.dirname, "..", "034-eval-harness");

function parseArgs() {
  const argv = process.argv.slice(2);
  const label = argv[0];
  const o: Record<string, string | boolean> = {};
  for (let i = 1; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--stats-from-diag") o.statsFromDiag = true;
    else if (a === "--cards" || a === "--work" || a === "--pairs")
      o[a.slice(2)] = argv[++i];
    else throw new Error(`unknown argument ${a}`);
  }
  if (!label) throw new Error("usage: rescore.ts <label> --cards <dir> ...");
  return {
    label,
    cards: o.cards as string | undefined,
    work: (o.work as string) ?? `/tmp/eval-work/${label}`,
    statsFromDiag: o.statsFromDiag === true,
    pairs: (o.pairs as string | undefined)?.split(",")
  };
}

const isCard = (x: unknown): x is Scorecard =>
  !!x &&
  typeof x === "object" &&
  typeof (x as Scorecard).pair === "string" &&
  !!(x as Scorecard).churn?.statements;

/** Every partition the split promises, checked against the frozen fields. */
export function partitionFailures(c: Scorecard): string[] {
  const s = c.churn.surfaces;
  if (!s) return ["no surfaces block"];
  const bad: string[] = [];
  const eq = (what: string, got: number, want: number) => {
    if (got !== want) bad.push(`${what}: split ${got} != old ${want}`);
  };
  const sum3 = <T>(b: { app: T; vendor: T; crossBoundary: T }, k: keyof T) =>
    (b.app[k] as number) +
    (b.vendor[k] as number) +
    (b.crossBoundary[k] as number);
  if (s.statements) {
    eq("stmts", sum3(s.statements, "total"), c.churn.statements.total);
    eq(
      "clean",
      sum3(s.statements, "unchangedClean"),
      c.churn.statements.unchangedClean
    );
    eq(
      "noise",
      sum3(s.statements, "unchangedChurned"),
      c.churn.statements.unchangedChurned
    );
    eq("novel", sum3(s.statements, "novel"), c.churn.statements.novel);
    eq(
      "noiseLn",
      sum3(s.statements, "namingNoiseLines"),
      c.churn.lines.namingNoiseLines
    );
    eq("realLn", sum3(s.statements, "realLines"), c.churn.lines.realLines);
  } else bad.push("no statement split");
  eq(
    "reloc",
    sum3(s.names, "sameNameMovedFile"),
    c.churn.relocations.sameNameMovedFile
  );
  eq("newName", sum3(s.names, "novelNames"), c.churn.relocations.novelNames);
  eq("freshNames", sum3(s.names, "freshNames"), c.churn.relocations.freshNames);
  if (c.churn.tree) {
    eq(
      "relocSt",
      sum3(s.tree, "relocatedStatements"),
      c.churn.tree.relocatedStatements
    );
    eq(
      "compared",
      sum3(s.tree, "statementsCompared"),
      c.churn.tree.statementsCompared
    );
  }
  if (s.mints) {
    eq(
      "mints",
      s.mints.app + s.mints.vendor + s.mints.unattributed,
      c.determinism.mintedLeftovers
    );
  }
  const l = s.lines;
  if (l && c.churn.layout && c.churn.vendor) {
    eq(
      "treeLn+vendor",
      l.app.churnLines + l.vendor.churnLines + l.crossBoundary.churnLines,
      c.churn.layout.churnLines + c.churn.vendor.churnLines
    );
    eq(
      "real+vendorReal",
      l.app.real +
        l.vendor.real +
        l.crossBoundary.churnLines -
        l.crossBoundary.appSideFromFiles,
      c.churn.layout.real + c.churn.vendor.real
    );
    eq(
      "fileAddRemove",
      l.app.fileAddRemove + l.crossBoundary.appSideFromFiles,
      c.churn.layout.fileAddRemove ?? -1
    );
    eq("app noise", l.app.noise, c.churn.layout.noise);
    eq("vendor noise", l.vendor.noise, c.churn.vendor.noise);
  }
  return bad;
}

function main() {
  const a = parseArgs();
  const out = path.join(HARNESS, "results", a.label);
  fs.mkdirSync(out, { recursive: true });
  type Job = { to: string; pair: string; old?: string; manifest?: string };
  const jobs: Job[] = [];
  if (a.cards) {
    for (const f of fs.readdirSync(a.cards).sort()) {
      const p = path.join(a.cards, f);
      if (!fs.statSync(p).isFile()) continue;
      // Copy the run records the summary reads (statuses, verdicts,
      // manifests, provenance); never the logs.
      if (/\.(json|txt)$/.test(f) && f !== "summary.json") {
        fs.copyFileSync(p, path.join(out, f));
      }
      if (!f.endsWith(".json") || /-|stats|summary|pipeline/.test(f)) continue;
      const text = fs.readFileSync(p, "utf8");
      const parsed = JSON.parse(text);
      if (!isCard(parsed)) continue;
      jobs.push({
        to: f.replace(/\.json$/, ""),
        pair: parsed.pair,
        old: text,
        manifest: path.join(a.cards, f.replace(/\.json$/, "-run.json"))
      });
    }
  }
  for (const pair of a.pairs ?? []) {
    if (jobs.some((j) => j.pair === pair)) continue;
    jobs.push({ to: pair.split("->")[1], pair });
  }
  let failed = 0;
  for (const j of jobs) {
    const from = j.pair.split("->")[0];
    const manifestPrior =
      j.manifest && fs.existsSync(j.manifest)
        ? (JSON.parse(fs.readFileSync(j.manifest, "utf8")).inputs
            ?.prior as string)
        : undefined;
    const prior =
      manifestPrior ??
      path.join(a.work, `${from}-rebased/.humanify/humanified.js`);
    const priorTree = path.dirname(path.dirname(prior));
    const fresh = path.join(a.work, j.to);
    let stats = a.cards ? path.join(a.cards, `${j.to}.stats.json`) : "";
    if (a.statsFromDiag || !stats || !fs.existsSync(stats)) {
      // The diag's coverage block is the stats' coverage block, byte for
      // byte (checked on exp164: functions + mintedCensus identical).
      const diag = JSON.parse(
        fs.readFileSync(path.join(a.work, `${j.to}.diag.json`), "utf8")
      );
      stats = path.join(out, `${j.to}.stats-from-diag.json`);
      fs.writeFileSync(stats, JSON.stringify({ coverage: diag.coverage }));
    }
    const args = [
      "tsx",
      path.join(HARNESS, "analyze.ts"),
      path.join(fresh, ".humanify/humanified.js"),
      prior,
      path.join(fresh, ".humanify/split-ledger.json"),
      path.join(priorTree, ".humanify/split-ledger.json"),
      stats,
      j.pair,
      path.join(fresh, "src"),
      path.join(priorTree, "src"),
      path.join(fresh, "vendor"),
      path.join(priorTree, "vendor")
    ];
    const t0 = Date.now();
    const r = spawnSync("npx", args, {
      encoding: "utf8",
      maxBuffer: 1 << 30,
      env: { ...process.env, NODE_OPTIONS: "--max-old-space-size=65536" }
    });
    if (r.status !== 0) {
      console.log(`${a.label} ${j.pair}: ANALYZE FAILED\n${r.stderr}`);
      failed++;
      continue;
    }
    fs.writeFileSync(path.join(out, `${j.to}.json`), r.stdout);
    const card = JSON.parse(r.stdout) as Scorecard;
    const secs = Math.round((Date.now() - t0) / 1000);
    if (j.old !== undefined) {
      const stripped = JSON.parse(r.stdout);
      delete stripped.churn.surfaces;
      const same = `${JSON.stringify(stripped, null, 2)}\n` === j.old;
      console.log(
        `${a.label} ${j.pair}: ${same ? "OLD-FIELDS IDENTICAL" : "OLD-FIELDS DIFFER"} (${secs}s)`
      );
      if (!same) failed++;
    } else {
      console.log(`${a.label} ${j.pair}: scored (no old card) (${secs}s)`);
    }
    const bad = partitionFailures(card);
    console.log(
      bad.length
        ? `  PARTITION FAILED: ${bad.join("; ")}`
        : "  partitions: all add up"
    );
    if (bad.length) failed++;
  }
  process.exit(failed ? 1 : 0);
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(import.meta.filename)
) {
  main();
}
