/**
 * Compare two perturbation-lab result JSONs side-by-side.
 *
 * Usage:
 *   npx tsx experiments/011-perturbation-lab/compare.ts <baseline> <experiment>
 *
 * Reads from experiments/011-perturbation-lab/results/<name>.json
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { ExperimentResult, ExperimentRow } from "./types.js";

function loadResult(name: string): ExperimentResult {
  const path = join(import.meta.dirname, "results", `${name}.json`);
  return JSON.parse(readFileSync(path, "utf-8"));
}

function rowKey(r: ExperimentRow): string {
  return `${r.corpus}::${r.perturbation}::${r.minifier}`;
}

function pct(n: number): string {
  return `${(n * 100).toFixed(0)}%`;
}

function delta(a: number, b: number): string {
  const d = b - a;
  if (d === 0) return "  0";
  const sign = d > 0 ? "+" : "";
  return `${sign}${d}`;
}

function deltaPct(a: number, b: number): string {
  const d = (b - a) * 100;
  if (Math.abs(d) < 0.5) return "   0%";
  const sign = d > 0 ? "+" : "";
  return `${sign}${d.toFixed(0)}%`;
}

function main(): void {
  const args = process.argv.slice(2);
  if (args.length < 2) {
    console.error(
      "Usage: npx tsx experiments/011-perturbation-lab/compare.ts <baseline> <experiment>"
    );
    process.exit(1);
  }

  const [baselineName, experimentName] = args;
  const baseline = loadResult(baselineName);
  const experiment = loadResult(experimentName);

  const baseMap = new Map<string, ExperimentRow>();
  for (const r of baseline.rows) baseMap.set(rowKey(r), r);

  const expMap = new Map<string, ExperimentRow>();
  for (const r of experiment.rows) expMap.set(rowKey(r), r);

  const pad = (v: string | number, w: number) => String(v).padEnd(w);

  // Header
  console.log(`\nComparing: ${baselineName} → ${experimentName}\n`);
  console.log(
    pad("CORPUS", 16) +
      pad("PERTURBATION", 30) +
      pad("MINIFIER", 14) +
      pad("ACC", 8) +
      pad("dACC", 8) +
      pad("F1", 8) +
      pad("dF1", 8) +
      pad("dTP", 6) +
      pad("dFP", 6) +
      pad("dFN", 6)
  );
  console.log("─".repeat(110));

  const flippedToGood: string[] = [];
  const flippedToBad: string[] = [];

  // All keys from both sets
  const allKeys = new Set([...baseMap.keys(), ...expMap.keys()]);
  for (const key of allKeys) {
    const b = baseMap.get(key);
    const e = expMap.get(key);

    if (!b && e) {
      console.log(
        pad(e.corpus, 16) +
          pad(e.perturbation, 30) +
          pad(e.minifier, 14) +
          pad(pct(e.score.accuracy), 8) +
          pad("NEW", 8) +
          pad(pct(e.score.f1), 8) +
          pad("NEW", 8)
      );
      continue;
    }
    if (b && !e) {
      console.log(
        pad(b.corpus, 16) +
          pad(b.perturbation, 30) +
          pad(b.minifier, 14) +
          pad(pct(b.score.accuracy), 8) +
          pad("GONE", 8)
      );
      continue;
    }
    if (!b || !e) continue;

    const label = `${b.corpus} / ${b.perturbation} / ${b.minifier}`;
    if (b.score.fn > 0 && e.score.fn === 0) flippedToGood.push(label);
    if (b.score.fn === 0 && e.score.fn > 0) flippedToBad.push(label);

    console.log(
      pad(b.corpus, 16) +
        pad(b.perturbation, 30) +
        pad(b.minifier, 14) +
        pad(pct(e.score.accuracy), 8) +
        pad(deltaPct(b.score.accuracy, e.score.accuracy), 8) +
        pad(pct(e.score.f1), 8) +
        pad(deltaPct(b.score.f1, e.score.f1), 8) +
        pad(delta(b.score.tp, e.score.tp), 6) +
        pad(delta(b.score.fp, e.score.fp), 6) +
        pad(delta(b.score.fn, e.score.fn), 6)
    );
  }

  // Flipped runs
  if (flippedToGood.length > 0) {
    console.log(`\nFixed (FN>0 → FN=0): ${flippedToGood.length}`);
    for (const label of flippedToGood) console.log(`  + ${label}`);
  }
  if (flippedToBad.length > 0) {
    console.log(`\nRegressed (FN=0 → FN>0): ${flippedToBad.length}`);
    for (const label of flippedToBad) console.log(`  - ${label}`);
  }

  // Aggregate summary delta
  console.log("\nAggregate summary:");
  console.log(
    `  ${baselineName}: acc=${pct(baseline.summary.avgAccuracy)} f1=${pct(baseline.summary.avgF1)} recall=${pct(baseline.summary.avgRecall)} precision=${pct(baseline.summary.avgPrecision)}`
  );
  console.log(
    `  ${experimentName}: acc=${pct(experiment.summary.avgAccuracy)} f1=${pct(experiment.summary.avgF1)} recall=${pct(experiment.summary.avgRecall)} precision=${pct(experiment.summary.avgPrecision)}`
  );
  console.log(
    `  delta: acc=${deltaPct(baseline.summary.avgAccuracy, experiment.summary.avgAccuracy)} f1=${deltaPct(baseline.summary.avgF1, experiment.summary.avgF1)} recall=${deltaPct(baseline.summary.avgRecall, experiment.summary.avgRecall)} precision=${deltaPct(baseline.summary.avgPrecision, experiment.summary.avgPrecision)}`
  );
  console.log("");
}

main();
