/**
 * Perturbation lab: apply known AST transformations to a source file,
 * minify both versions, run matchFunctions, and score the outcome against
 * source-level ground truth.
 *
 * Usage:
 *   npx tsx experiments/011-perturbation-lab/run.ts [--name baseline]
 */

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { MINIFIER_CONFIGS } from "../../test/e2e/harness/minify.js";
import { buildFingerprintData } from "../../test/e2e/harness/validate.js";
import { matchFunctions } from "../../src/analysis/fingerprint-index.js";
import { minifyString } from "./minify-string.js";
import {
  computeSourceGroundTruth,
  buildMinifiedGroundTruth
} from "./ground-truth.js";
import { scoreByCorrespondence } from "./scoring.js";
import { CORPUS, getCorpusItem } from "./corpus.js";
import { buildDefaultPlans } from "./perturbations/index.js";
import type {
  CorpusItem,
  ExperimentResult,
  ExperimentRow,
  Perturbation
} from "./types.js";

async function runOne(
  corpus: CorpusItem,
  perturbation: Perturbation,
  minifierId: string
): Promise<ExperimentRow> {
  const v1Source = readFileSync(corpus.sourcePath, "utf-8");
  const perturbed = perturbation.apply(v1Source);
  const v2Source = perturbed.source;

  const sourceGroundTruth = computeSourceGroundTruth(v1Source, v2Source);

  const minifierConfig = MINIFIER_CONFIGS.find((c) => c.id === minifierId);
  if (!minifierConfig) throw new Error(`Unknown minifier: ${minifierId}`);

  const v1Min = await minifyString(v1Source, minifierConfig, "v1.js");
  const v2Min = await minifyString(v2Source, minifierConfig, "v2.js");

  const v1Data = buildFingerprintData(v1Min.code, v1Min.minifiedPath);
  const v2Data = buildFingerprintData(v2Min.code, v2Min.minifiedPath);

  const matchResult = matchFunctions(v1Data.index, v2Data.index);
  const minGT = buildMinifiedGroundTruth(
    v1Min.code,
    v1Min.minifiedPath,
    v1Data.index,
    v2Min.code,
    v2Min.minifiedPath,
    v2Data.index
  );
  const score = scoreByCorrespondence(minGT, matchResult);

  return {
    corpus: corpus.id,
    perturbation: perturbation.id,
    perturbationDescription: perturbed.description,
    minifier: minifierId,
    sourceGroundTruth,
    matcher: {
      v1MinifiedCount: v1Data.index.fingerprints.size,
      v2MinifiedCount: v2Data.index.fingerprints.size,
      matched: matchResult.matches.size,
      ambiguous: matchResult.ambiguous.size,
      unmatched: matchResult.unmatched.length
    },
    score
  };
}

function summarize(rows: ExperimentRow[]): ExperimentResult["summary"] {
  if (rows.length === 0) {
    return {
      totalRuns: 0,
      avgAccuracy: 0,
      avgF1: 0,
      avgRecall: 0,
      avgPrecision: 0
    };
  }
  const avg = (key: keyof ExperimentRow["score"]) =>
    rows.reduce((s, r) => s + (r.score[key] as number), 0) / rows.length;
  return {
    totalRuns: rows.length,
    avgAccuracy: avg("accuracy"),
    avgF1: avg("f1"),
    avgRecall: avg("recall"),
    avgPrecision: avg("precision")
  };
}

function printTable(rows: ExperimentRow[]): void {
  const pad = (v: string | number, w: number) => String(v).padEnd(w);
  const pct = (n: number) => `${(n * 100).toFixed(0)}%`.padStart(5);

  console.log(
    "\n" +
      pad("CORPUS", 16) +
      pad("PERTURBATION", 30) +
      pad("MINIFIER", 16) +
      pad("V1", 5) +
      pad("V2", 5) +
      pad("MATCH", 7) +
      pad("AMB", 5) +
      pad("UNM", 5) +
      pad("ExpM", 6) +
      pad("ExpU", 6) +
      pad("TP", 5) +
      pad("FN", 5) +
      pad("TN", 5) +
      "ACC  F1"
  );
  console.log("─".repeat(125));

  for (const r of rows) {
    console.log(
      pad(r.corpus, 16) +
        pad(r.perturbation, 30) +
        pad(r.minifier, 16) +
        pad(r.matcher.v1MinifiedCount, 5) +
        pad(r.matcher.v2MinifiedCount, 5) +
        pad(r.matcher.matched, 7) +
        pad(r.matcher.ambiguous, 5) +
        pad(r.matcher.unmatched, 5) +
        pad(r.sourceGroundTruth.expectedMatches, 6) +
        pad(r.sourceGroundTruth.expectedUnmatched, 6) +
        pad(r.score.tp, 5) +
        pad(r.score.fn, 5) +
        pad(r.score.tn, 5) +
        pct(r.score.accuracy) +
        "  " +
        pct(r.score.f1)
    );
  }
}

function printFailures(rows: ExperimentRow[]): void {
  const imperfect = rows.filter((r) => r.score.fn > 0);
  if (imperfect.length === 0) {
    console.log("\nNo matching failures — every hash that could match, did.");
    return;
  }
  console.log(
    `\n${imperfect.length} runs had FN > 0 (matches lost in disambiguation):`
  );
  for (const r of imperfect) {
    console.log(
      `  ${r.corpus} / ${r.perturbation} / ${r.minifier}: ` +
        `${r.score.fn} lost matches (TP=${r.score.tp}, FN=${r.score.fn}, TN=${r.score.tn})`
    );
  }
}

function parseArgs(argv: string[]): { name: string } {
  let name = "baseline";
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--name" && i + 1 < argv.length) {
      name = argv[i + 1];
      i++;
    }
  }
  return { name };
}

async function main(): Promise<void> {
  const { name } = parseArgs(process.argv.slice(2));
  const plans = buildDefaultPlans();
  const rows: ExperimentRow[] = [];

  console.log(
    `Running ${plans.length} perturbations × ${MINIFIER_CONFIGS.length} minifiers = ${plans.length * MINIFIER_CONFIGS.length} runs\n`
  );

  for (const plan of plans) {
    const corpus = getCorpusItem(plan.corpusId);
    for (const minifier of MINIFIER_CONFIGS) {
      process.stdout.write(
        `  ${corpus.id} · ${plan.perturbation.id} · ${minifier.id} ... `
      );
      try {
        const row = await runOne(corpus, plan.perturbation, minifier.id);
        rows.push(row);
        const { tp, fn, accuracy } = row.score;
        console.log(
          `TP=${tp} FN=${fn} acc=${(accuracy * 100).toFixed(0)}%`
        );
      } catch (err) {
        console.log(`ERROR: ${(err as Error).message}`);
      }
    }
  }

  const result: ExperimentResult = {
    name,
    timestamp: new Date().toISOString(),
    config: {
      perturbations: plans.map((p) => ({
        corpus: p.corpusId,
        perturbation: p.perturbation.id
      })),
      corpus: CORPUS.map((c) => c.id),
      minifiers: MINIFIER_CONFIGS.map((m) => m.id)
    },
    rows,
    summary: summarize(rows)
  };

  printTable(rows);
  printFailures(rows);

  console.log("\nSummary:");
  console.log(`  Total runs:    ${result.summary.totalRuns}`);
  console.log(`  Avg accuracy:  ${(result.summary.avgAccuracy * 100).toFixed(1)}%`);
  console.log(`  Avg F1:        ${(result.summary.avgF1 * 100).toFixed(1)}%`);
  console.log(`  Avg recall:    ${(result.summary.avgRecall * 100).toFixed(1)}%`);
  console.log(`  Avg precision: ${(result.summary.avgPrecision * 100).toFixed(1)}%`);

  const outputPath = join(
    import.meta.dirname,
    "results",
    `${name}.json`
  );
  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, JSON.stringify(result, null, 2), "utf-8");
  console.log(`\nWrote ${outputPath}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
