/**
 * Exp021 offline harness: apply the class/function-expression inner-id
 * derivation (naming-floor WS1) to an existing humanified output, verify
 * the pure-rename invariant, and re-census — no LLM, no bundle run.
 *
 *   npx tsx experiments/021-naming-floor/run-floor.ts \
 *     /tmp/exp020-chain-on/cc-119-lineage/runtime.js
 */

import fs from "node:fs";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { collectEvalWithTaint } from "../../src/analysis/soundness.js";
import { generate } from "../../src/babel-utils.js";
import {
  captureSemanticBaseline,
  checkStructuralInvariant,
  validateOutput
} from "../../src/output-validation.js";
import { deriveExpressionInnerNames } from "../../src/rename/class-id-floor.js";
import { retryDecoratedNames } from "../../src/rename/decoration-retry.js";
import {
  collectMintedBindings,
  summarizeCensus
} from "../../src/rename/minted-census.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

function main(): void {
  const file = process.argv[2];
  if (!file) {
    console.error("usage: run-floor.ts <output.js> [--out <file>]");
    process.exit(1);
  }
  const isEligible = createIsEligible("bun", "bun");
  const code = fs.readFileSync(file, "utf-8");

  let t0 = Date.now();
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File | null;
  if (!ast) throw new Error(`failed to parse ${file}`);
  console.log(`parse: ${Date.now() - t0}ms`);

  const before = summarizeCensus(collectMintedBindings(ast, isEligible));
  const baseline = captureSemanticBaseline(ast);

  t0 = Date.now();
  const taint = collectEvalWithTaint(ast);
  const result = deriveExpressionInnerNames(ast, isEligible, taint);
  const decoration = retryDecoratedNames(ast, isEligible, taint);
  console.log(`floor: ${Date.now() - t0}ms`);
  console.log(
    `derived: ${result.derived}  undecorated: ${decoration.undecorated}  ` +
      `(decoration skipped: ${decoration.skipped})`
  );

  const structuralFailure = checkStructuralInvariant(ast, baseline);
  if (structuralFailure) {
    console.error(
      `STRUCTURAL INVARIANT VIOLATED: ${structuralFailure.message}`
    );
    process.exit(2);
  }
  console.log("structural invariant: clean");

  const after = summarizeCensus(collectMintedBindings(ast, isEligible));

  console.log(
    `\nderived: ${result.derived}  skipped: ${result.skipped.length}`
  );
  console.log(
    `class-expr id census: ${before.byFamily.classExprId} → ${after.byFamily.classExprId}`
  );
  console.log(
    `fn-expr id census:    ${before.byFamily.fnExprId} → ${after.byFamily.fnExprId}`
  );
  console.log(`total minted: ${before.total} → ${after.total}`);

  const byReason = new Map<string, number>();
  for (const skip of result.skipped) {
    byReason.set(skip.reason, (byReason.get(skip.reason) ?? 0) + 1);
  }
  console.log("\nskip reasons:");
  for (const [reason, count] of [...byReason].sort((a, b) => b[1] - a[1])) {
    console.log(`  ${reason}: ${count}`);
  }

  const outIdx = process.argv.indexOf("--out");
  if (outIdx !== -1) {
    const output = generate(ast, { compact: false }).code;
    const validation = validateOutput(output, baseline);
    if (validation.parseFailure || validation.semanticFailure) {
      console.error("OUTPUT VALIDATION FAILED");
      process.exit(2);
    }
    fs.writeFileSync(process.argv[outIdx + 1], output);
    console.log(`\nwrote: ${process.argv[outIdx + 1]} (validation clean)`);
  }
}

main();
