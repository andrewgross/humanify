/**
 * Exp020 offline harness: run the text-diff reconciliation as a post-pass
 * over two existing humanified outputs (no LLM, no re-humanify).
 *
 *   npx tsx experiments/020-tail-polish/run-reconcile.ts \
 *     --new /tmp/exp019-chain/cc-119-lineage/runtime.js \
 *     --prior /tmp/exp016-r1/cc-120/runtime.js \
 *     [--apply] [--descriptive] [--out <reconciled.js>] [--dump <renames.json>]
 *
 * Dry-run (default) only prints/dumps candidates. --apply mutates, checks
 * the structural invariant against the pre-pass baseline, validates the
 * regenerated output, and writes it to --out.
 */

import fs from "node:fs";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { generate } from "../../src/babel-utils.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant,
  validateOutput
} from "../../src/output-validation.js";
import {
  computeNormalDiff,
  reconcileDiffNoise,
  type ReconcileResult
} from "../../src/rename/diff-reconcile.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

function arg(name: string): string | undefined {
  const idx = process.argv.indexOf(`--${name}`);
  return idx === -1 ? undefined : process.argv[idx + 1];
}
const hasFlag = (name: string) => process.argv.includes(`--${name}`);

function summarize(result: ReconcileResult): void {
  console.log("\n=== hunks ===");
  console.log(result.hunks);

  const byKind = new Map<string, { bindings: number; votes: number }>();
  for (const rename of result.renames) {
    const entry = byKind.get(rename.kind) ?? { bindings: 0, votes: 0 };
    entry.bindings++;
    entry.votes += rename.votes;
    byKind.set(rename.kind, entry);
  }
  console.log("\n=== renames (bindings | occurrence votes) ===");
  for (const [kind, entry] of byKind) {
    console.log(`  ${kind}: ${entry.bindings} bindings | ${entry.votes} votes`);
  }

  const byReason = new Map<string, number>();
  for (const skip of result.skipped) {
    byReason.set(skip.reason, (byReason.get(skip.reason) ?? 0) + 1);
  }
  console.log("\n=== skips (bindings) ===");
  for (const [reason, count] of [...byReason].sort((a, b) => b[1] - a[1])) {
    console.log(`  ${reason}: ${count}`);
  }

  console.log("\n=== sample renames (top 30 by votes) ===");
  const sorted = [...result.renames].sort((a, b) => b.votes - a.votes);
  for (const rename of sorted.slice(0, 30)) {
    console.log(
      `  ${String(rename.votes).padStart(4)}  ${rename.fromName} -> ${rename.toName} [${rename.kind}] @L${rename.declLine}`
    );
  }
}

function main(): void {
  const newPath = arg("new");
  const priorPath = arg("prior");
  if (!newPath || !priorPath) {
    console.error("required: --new <file> --prior <file>");
    process.exit(1);
  }
  const apply = hasFlag("apply");
  const descriptiveTier = hasFlag("descriptive");

  console.log(`new:   ${newPath}`);
  console.log(`prior: ${priorPath}`);
  console.log(
    `mode:  ${apply ? "APPLY" : "dry-run"}${descriptiveTier ? " + descriptive tier" : " (asymmetric only)"}`
  );

  const newText = fs.readFileSync(newPath, "utf-8");
  const priorText = fs.readFileSync(priorPath, "utf-8");

  let t0 = Date.now();
  const ast = parseSync(newText, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File;
  if (!ast) throw new Error("failed to parse new output");
  console.log(`parse: ${Date.now() - t0}ms`);

  t0 = Date.now();
  const baseline = captureSemanticBaseline(ast);
  console.log(`baseline: ${Date.now() - t0}ms`);

  t0 = Date.now();
  const diffText = computeNormalDiff(priorText, newText);
  console.log(`diff: ${Date.now() - t0}ms (${diffText.length} bytes)`);

  t0 = Date.now();
  const result = reconcileDiffNoise(ast, diffText, {
    apply,
    descriptiveTier,
    isEligible: createIsEligible("bun", "bun"),
    priorLineCount: priorText.split("\n").length
  });
  console.log(`reconcile: ${Date.now() - t0}ms`);

  summarize(result);

  const dumpPath = arg("dump");
  if (dumpPath) {
    fs.writeFileSync(
      dumpPath,
      JSON.stringify(
        {
          renames: result.renames,
          skipped: result.skipped,
          hunks: result.hunks
        },
        null,
        2
      )
    );
    console.log(`\ndumped: ${dumpPath}`);
  }

  if (!apply) return;

  t0 = Date.now();
  const structuralFailure = checkStructuralInvariant(ast, baseline);
  if (structuralFailure) {
    console.error(
      `\nSTRUCTURAL INVARIANT VIOLATED: ${structuralFailure.message}`
    );
    process.exit(2);
  }
  console.log(`\nstructural invariant: clean (${Date.now() - t0}ms)`);

  t0 = Date.now();
  const output = generate(ast, { compact: false }).code;
  console.log(`generate: ${Date.now() - t0}ms`);

  t0 = Date.now();
  const validation = validateOutput(output, baseline);
  if (validation.parseFailure) {
    console.error(`OUTPUT PARSE FAILURE: ${validation.parseFailure.message}`);
    process.exit(2);
  }
  if (validation.semanticFailure) {
    console.error(
      `OUTPUT SEMANTIC FAILURE: ${validation.semanticFailure.message}`
    );
    process.exit(2);
  }
  console.log(`output validation: clean (${Date.now() - t0}ms)`);

  const outPath = arg("out");
  if (outPath) {
    fs.writeFileSync(outPath, output);
    console.log(`wrote: ${outPath}`);
  }
}

main();
