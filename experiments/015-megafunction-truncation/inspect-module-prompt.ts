/**
 * Reconstruct the module-binding prompt for a set of identifiers exactly as
 * the pipeline builds it, and print its size + per-section breakdown. Used
 * to autopsy the one module batch that 400-fails at ~45K tokens in every
 * run (exp015).
 *
 * Usage:
 *   NODE_OPTIONS=--max-old-space-size=16384 npx tsx \
 *     experiments/015-megafunction-truncation/inspect-module-prompt.ts \
 *     <prepared-runtime.js> IF7 tW8 PF5 ...
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildModuleLevelRenamePrompt } from "../../src/llm/prompts.js";
import {
  collectAssignmentContext,
  collectUsageExamples,
  getModuleLevelBindings
} from "../../src/rename/plugin.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

async function main(): Promise<void> {
  const [inputPath, ...ids] = process.argv.slice(2);
  const code = fs.readFileSync(inputPath, "utf-8");
  const ast = parseSync(code, { sourceType: "unambiguous" }) as t.File;
  if (!ast) throw new Error("parse failed");
  const isEligible = createIsEligible("bun", "bun");

  const result = getModuleLevelBindings(ast, isEligible, code);
  if (!result) throw new Error("no module bindings");
  const wanted = new Set(ids);
  const bindings = result.bindings.filter((b) => wanted.has(b.name));
  console.log(`bindings found: ${bindings.map((b) => b.name).join(", ")}`);
  for (const b of bindings) {
    console.log(
      `  ${b.name.padEnd(6)} declaration: ${b.declaration.length} chars / ${b.declaration.split("\n").length} lines | ${b.declaration.slice(0, 70).replace(/\n/g, " ")}`
    );
  }

  const idSet = new Set(bindings.map((b) => b.name));
  const assignments = collectAssignmentContext(ast, idSet);
  const usages = collectUsageExamples(
    ast,
    idSet,
    Object.fromEntries([...idSet].map((n) => [n, assignments[n]?.length ?? 0]))
  );

  for (const id of idSet) {
    const a = (assignments[id] ?? []).reduce((s, x) => s + x.length, 0);
    const u = (usages[id] ?? []).reduce((s, x) => s + x.length, 0);
    console.log(
      `${id.padEnd(6)} assignments: ${assignments[id]?.length ?? 0} snippets/${a} chars   usages: ${usages[id]?.length ?? 0}/${u} chars`
    );
  }

  const prompt = buildModuleLevelRenamePrompt(
    bindings.map((b) => b.declaration),
    assignments,
    usages,
    bindings.map((b) => b.name),
    new Set<string>(),
    isEligible
  );
  console.log(
    `\nprompt chars: ${prompt.length} (~${Math.round(prompt.length / 3.5)} tokens)`
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
