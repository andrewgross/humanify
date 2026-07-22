/**
 * Per-pair eval scorecard: deterministic breakdown + real/noise churn split.
 *
 * Combines two deterministic signals for one version transition (v-1 -> v):
 *
 *  1. DETERMINISM (from the pipeline's --stats-json): how each identifier got
 *     its name — cached/exact-transfer (stable) vs close-match (has a prior but
 *     re-named by the LLM) vs cold LLM (genuinely new) vs minted. This answers
 *     "what SHOULD be deterministic, and how much actually reaches the LLM".
 *
 *  2. CHURN (this script, rename-invariant): diff the fresh humanified output
 *     against the prior at the statement level using the split's own
 *     `statementHash` (identifier-blind). A statement whose hash exists in both
 *     is structurally UNCHANGED, so any text difference is pure NAMING NOISE
 *     (captures function-local flips too — the hash ignores names). A novel hash
 *     is REAL change. Plus relocation churn from the split ledgers (a binding
 *     whose home file moved drags every importer's require-alias).
 *
 * All of this is deterministic run-to-run EXCEPT the naming-noise magnitude,
 * which carries the LLM floor — that is exactly the number the determinism
 * breakdown contextualizes.
 *
 * Run:
 *   npx tsx experiments/034-eval-harness/analyze.ts \
 *     <freshHumanified.js> <priorHumanified.js> \
 *     <freshLedger.json> <priorLedger.json> <statsJson> <pairLabel>
 */
import * as fs from "node:fs";
import { statementsOf } from "./statements.js";

function readJson<T>(path: string): T {
  return JSON.parse(fs.readFileSync(path, "utf8")) as T;
}

interface Ledger {
  nameToFiles: Record<string, string[]>;
}

/** Same-name bindings whose home file changed (the require-alias-churn driver)
 * and novel names absent from the prior ledger. */
function relocationChurn(fresh: Ledger, prior: Ledger) {
  const ff = new Map(
    Object.entries(fresh.nameToFiles).map(([k, v]) => [k, v[0]])
  );
  const pf = new Map(
    Object.entries(prior.nameToFiles).map(([k, v]) => [k, v[0]])
  );
  let sameNameMovedFile = 0;
  let novelNames = 0;
  for (const [name, file] of ff) {
    const priorFile = pf.get(name);
    if (priorFile === undefined) novelNames++;
    else if (priorFile !== file) sameNameMovedFile++;
  }
  return { sameNameMovedFile, novelNames, freshNames: ff.size };
}

function churn(freshCode: string, priorCode: string) {
  const fresh = statementsOf(freshCode);
  const prior = statementsOf(priorCode);
  // Prior text set per hash — a fresh statement is CLEAN iff its exact text is
  // already present under its hash (structure unchanged AND names reproduced).
  const priorByHash = new Map<string, Set<string>>();
  for (const s of prior) {
    let set = priorByHash.get(s.hash);
    if (!set) priorByHash.set(s.hash, (set = new Set()));
    set.add(s.text);
  }
  let unchangedClean = 0;
  let unchangedChurned = 0;
  let novel = 0;
  let namingNoiseLines = 0;
  let realLines = 0;
  for (const s of fresh) {
    const priorTexts = priorByHash.get(s.hash);
    if (!priorTexts) {
      novel++;
      realLines += s.lines;
    } else if (priorTexts.has(s.text)) {
      unchangedClean++;
    } else {
      unchangedChurned++;
      namingNoiseLines += s.lines;
    }
  }
  return {
    statements: {
      total: fresh.length,
      unchangedClean,
      unchangedChurned,
      novel
    },
    lines: { namingNoiseLines, realLines }
  };
}

/** Determinism breakdown from the pipeline's --stats-json coverage block. */
// biome-ignore lint/suspicious/noExplicitAny: external stats JSON shape
function determinism(stats: any) {
  const f = stats?.coverage?.functions ?? {};
  const mb = stats?.coverage?.moduleBindings ?? {};
  const deterministic =
    (f.cached ?? 0) + (f.alreadyNamed ?? 0) + (f.nothingToRename ?? 0);
  const llm = (f.llm ?? 0) + (f.closeMatch ?? 0);
  return {
    functions: {
      total: f.total ?? 0,
      deterministic,
      closeMatchLLM: f.closeMatch ?? 0,
      coldLLM: f.llm ?? 0,
      failed: f.failed ?? 0,
      pctDeterministic: f.total
        ? +((100 * deterministic) / f.total).toFixed(2)
        : 0,
      pctReachingLLM: f.total ? +((100 * llm) / f.total).toFixed(2) : 0
    },
    moduleBindings: {
      total: mb.total ?? 0,
      cached: mb.cached ?? 0,
      llm: mb.llm ?? 0
    },
    mintedLeftovers: stats?.coverage?.mintedCensus?.total ?? 0
  };
}

function main() {
  const [freshHum, priorHum, freshLed, priorLed, statsPath, pair] =
    process.argv.slice(2);
  if (!freshHum || !priorHum || !freshLed || !priorLed || !statsPath || !pair) {
    throw new Error(
      "usage: analyze.ts <freshHum> <priorHum> <freshLedger> <priorLedger> <statsJson> <pairLabel>"
    );
  }
  const scorecard = {
    pair,
    determinism: determinism(readJson(statsPath)),
    churn: {
      ...churn(
        fs.readFileSync(freshHum, "utf8"),
        fs.readFileSync(priorHum, "utf8")
      ),
      relocations: relocationChurn(readJson(freshLed), readJson(priorLed))
    }
  };
  console.log(JSON.stringify(scorecard, null, 2));
}

main();
