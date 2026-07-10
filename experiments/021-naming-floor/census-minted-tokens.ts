/**
 * Exp021 census: count surviving minted (Bun-token-shaped) identifier
 * bindings in a humanified output, classified by family. This is the
 * experiment's before/after metric — run it on any lineage-leg output to
 * regenerate the numbers (no /tmp artifact required).
 *
 *   npx tsx experiments/021-naming-floor/census-minted-tokens.ts \
 *     /tmp/exp020-chain-on/cc-119-lineage/runtime.js [--samples 12]
 *
 * Families:
 *   classExprId — inner id of a named class expression
 *                 (`BaseError = class uq extends Error {}` — `uq`)
 *   fnExprId    — inner id of a named function expression
 *   param       — function parameter (binding.kind === "param")
 *   fnDecl      — whole function/class declaration name
 *   varOther    — everything else (var/let/const, catch params, imports)
 *
 * For classExprId/fnExprId the census also reports how many have a
 * DERIVABLE name (assignment target / declarator id / property key) —
 * the reachable population for the deterministic derivation pass.
 *
 * KNOWN FALSE POSITIVES (the token shape is a heuristic, biased toward
 * over-counting — treat varOther as an upper bound):
 *   - `ec2MetadataServiceEndpointSelector` matches the letterhead+digit
 *     rule (`ec2...`) but is a fine descriptive name.
 *   - `OS_MODULE`-style SCREAMING_SNAKE constants match the same rule.
 *   - `initializeApp_` is a descriptive name wearing a collision
 *     decoration (trailing underscore) — counted separately in the
 *     decoratedDescriptive overlay; it wants an undecorate retry, not a
 *     fresh name.
 *   - `RP_ConstructorKey`-style minified-stem+suffix names are counted
 *     in the halfNamedSuffix overlay.
 */

import fs from "node:fs";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import {
  collectMintedBindings,
  isDecoratedDescriptive,
  isHalfNamedSuffix,
  type MintedBinding,
  type MintedFamily
} from "../../src/rename/minted-census.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

// The census core lives in src/rename/minted-census.ts so the pipeline's
// end-of-run counter and this experiment script measure the exact same
// population. This script is the human-readable / offline view.
type Family = MintedFamily;
type CensusEntry = MintedBinding;

function collectCensus(ast: t.File): CensusEntry[] {
  return collectMintedBindings(ast, createIsEligible("bun", "bun"));
}

function printFamily(
  family: Family,
  entries: CensusEntry[],
  sampleCount: number
): void {
  const fam = entries.filter((e) => e.family === family);
  const samples = fam
    .slice(0, sampleCount)
    .map((e) => {
      const derived = e.derivedFrom ? `→${e.derivedFrom}` : "";
      return `${e.name}@L${e.line ?? "?"}${derived}`;
    })
    .join("  ");
  console.log(
    `  ${family.padEnd(12)} ${String(fam.length).padStart(5)}   ${samples}`
  );
}

function printDerivationStats(entries: CensusEntry[]): void {
  const exprIds = entries.filter(
    (e) => e.family === "classExprId" || e.family === "fnExprId"
  );
  if (exprIds.length === 0) return;
  const derivable = exprIds.filter((e) => e.derivedFrom !== null).length;
  const zeroRef = exprIds.filter((e) => e.refCount === 0).length;
  console.log(
    `\nexpression inner ids: ${exprIds.length} total, ` +
      `${derivable} with a derivable non-minted source name, ` +
      `${zeroRef} with zero references`
  );
}

function main(): void {
  const file = process.argv[2];
  if (!file) {
    console.error(
      "usage: npx tsx experiments/021-naming-floor/census-minted-tokens.ts <output.js> [--samples N]"
    );
    process.exit(1);
  }
  const samplesIdx = process.argv.indexOf("--samples");
  const sampleCount =
    samplesIdx === -1 ? 8 : Number(process.argv[samplesIdx + 1]);

  const code = fs.readFileSync(file, "utf-8");
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File | null;
  if (!ast) throw new Error(`failed to parse ${file}`);

  const entries = collectCensus(ast);

  console.log(`=== minted-token census: ${file} ===`);
  console.log(`eligible minted bindings: ${entries.length}\n`);
  console.log(`  family        count   samples`);
  const families: Family[] = [
    "classExprId",
    "fnExprId",
    "param",
    "fnDecl",
    "varOther"
  ];
  for (const family of families) printFamily(family, entries, sampleCount);

  const halfNamed = entries.filter((e) => isHalfNamedSuffix(e.name));
  const decorated = entries.filter((e) => isDecoratedDescriptive(e.name));
  console.log(`\noverlays (subsets of the families above):`);
  console.log(
    `  halfNamedSuffix (minified stem + descriptive tail): ${halfNamed.length}` +
      (halfNamed.length
        ? `   ${halfNamed
            .slice(0, sampleCount)
            .map((e) => e.name)
            .join("  ")}`
        : "")
  );
  console.log(
    `  decoratedDescriptive (descriptive + trailing _):    ${decorated.length}` +
      (decorated.length
        ? `   ${decorated
            .slice(0, sampleCount)
            .map((e) => e.name)
            .join("  ")}`
        : "")
  );

  printDerivationStats(entries);
}

main();
