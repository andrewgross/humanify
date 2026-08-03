/**
 * SIZE the private-name matching defect before believing it (task #18).
 *
 *   npx tsx experiments/lib/size-private-name-match.ts <prior.js> <new.js>
 *
 * ## The claim being sized
 *
 * `serializeNode` emits a class-private name VERBATIM (`P=#f`), so
 * `structuralHash` is rename-VARIANT with respect to private names — the exact
 * property the hash exists to avoid for every other binding. A minifier that
 * re-letters `#f` to `#a` between releases should therefore change the hash of
 * every function containing one, breaking the cross-version match.
 *
 * ## Why size it rather than fix it
 *
 * Fixing it changes `structuralHash`, therefore matching, therefore naming,
 * therefore emitted output — a full 4-pair cold eval. And rule 11 says the
 * src/ per-hop draw band is +/-2,800 lines, so if the population affected is
 * small the whole lever is BELOW THE NOISE FLOOR and the honest answer is to
 * leave it and say so. That verdict is only available before the run.
 *
 * This script computes the population and the actual failure rate. It runs no
 * LLM and writes nothing.
 *
 * ## What it deliberately does NOT claim
 *
 * A function that contains a private name AND fails to match has not been
 * shown to fail BECAUSE of the private name — it may differ for other reasons.
 * The two numbers to compare are the match-failure rate among private-name
 * functions and among all others. If they are the same, the private name is
 * not the cause, whatever the raw count looks like.
 *
 * ## MEASURED 2026-08-03 — and the answer is DO NOT SHIP IT
 *
 *                          2.1.85->2.1.86      2.1.197->2.1.198
 *   prior functions              35,663              57,419
 *   with a private name             147 (0.41%)        228 (0.40%)
 *   match failure, with          74.83%              66.23%
 *   match failure, without       60.52%              58.40%
 *   CEILING (recoverable)           110 (0.31%)        151 (0.26%)
 *
 * The mechanism is REAL — private-name functions fail to match noticeably more
 * often than the rest, consistently on both pairs. The MAGNITUDE is not worth
 * an eval: the ceiling is ~0.3% of functions, and rule 11 puts the src/ per-hop
 * draw band at +/-2,800 lines. For scale, exp048's true effect was -335 lines
 * and the gate could not resolve it, crediting the change with -2,864 and
 * charging another hop with a regression. A lever with a 110-function ceiling
 * is in that regime or below it, so a 4-pair cold A/B would produce a confident
 * number that means nothing.
 *
 * A CONFOUND worth stating, because it would have to be ruled out first:
 * functions containing private names are class bodies, which are larger and
 * plausibly likelier to change between releases for reasons that have nothing
 * to do with the hash. The elevated failure rate is consistent with the
 * serialization defect but does not establish it as the cause.
 *
 * If this is ever revisited: fix it, then measure the MATCH COUNT directly
 * (deterministic, no LLM, not noise-limited) rather than reading KPIs — the
 * exp048 lesson. And watch for NEW ambiguity: making the hash private-name
 * invariant can collide two classes that previously differed only there.
 */
import * as fs from "node:fs";
import type * as t from "@babel/types";
import { parseSourceAst } from "../../src/babel-utils.js";
import {
  buildFingerprintIndex,
  matchFunctions
} from "../../src/analysis/fingerprint-index.js";
import { buildFunctionGraph } from "../../src/analysis/function-graph.js";
import type { FunctionNode } from "../../src/analysis/types.js";

function graphOf(code: string, label: string): Map<string, FunctionNode> {
  const ast = parseSourceAst(code);
  if (!ast) throw new Error(`could not parse ${label}`);
  const fns = buildFunctionGraph(ast as t.File, label);
  return new Map(fns.map((f) => [f.sessionId, f]));
}

/** Does this function's own body contain a class-private name? */
function hasPrivateName(fn: FunctionNode): boolean {
  let found = false;
  fn.path.traverse({
    PrivateName() {
      found = true;
    }
  });
  return found;
}

function pct(n: number, d: number): string {
  return d === 0 ? "n/a" : `${((100 * n) / d).toFixed(2)}%`;
}

function main(): void {
  const [priorPath, newPath] = process.argv.slice(2);
  if (!priorPath || !newPath) {
    console.error("usage: size-private-name-match.ts <prior.js> <new.js>");
    process.exit(2);
  }

  const priorFns = graphOf(fs.readFileSync(priorPath, "utf8"), "prior");
  const newFns = graphOf(fs.readFileSync(newPath, "utf8"), "new");

  const result = matchFunctions(
    buildFingerprintIndex(priorFns),
    buildFingerprintIndex(newFns)
  );
  const matched = new Set(result.matches.keys());

  let withPrivate = 0;
  let withPrivateUnmatched = 0;
  let withoutPrivate = 0;
  let withoutPrivateUnmatched = 0;

  for (const [id, fn] of priorFns) {
    const isMatched = matched.has(id);
    if (hasPrivateName(fn)) {
      withPrivate++;
      if (!isMatched) withPrivateUnmatched++;
    } else {
      withoutPrivate++;
      if (!isMatched) withoutPrivateUnmatched++;
    }
  }

  const total = priorFns.size;
  console.log(`prior functions:            ${total}`);
  console.log(
    `  containing a private name: ${withPrivate} (${pct(withPrivate, total)} of the population)`
  );
  console.log("");
  console.log("MATCH FAILURE RATE — the comparison that decides this:");
  console.log(
    `  with a private name:    ${withPrivateUnmatched}/${withPrivate} = ${pct(withPrivateUnmatched, withPrivate)}`
  );
  console.log(
    `  without:                ${withoutPrivateUnmatched}/${withoutPrivate} = ${pct(withoutPrivateUnmatched, withoutPrivate)}`
  );
  console.log("");
  // The ceiling, stated as a count rather than a hope: even if EVERY
  // private-name match failure were caused by the verbatim serialization,
  // this is the most functions a fix could recover.
  console.log(
    `CEILING: at most ${withPrivateUnmatched} function(s) could be recovered — ` +
      `${pct(withPrivateUnmatched, total)} of the prior population.`
  );
  if (withPrivate === 0) {
    console.log(
      "\nVERDICT: no prior function contains a private name on this pair. " +
        "The lever cannot do anything here; do not run an eval for it."
    );
  } else if (withPrivateUnmatched === 0) {
    console.log(
      "\nVERDICT: every private-name function already matches. The verbatim " +
        "serialization is costing nothing on this pair."
    );
  }
}

main();
