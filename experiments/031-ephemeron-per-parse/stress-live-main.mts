/**
 * Faithful repro of the exp030 post-naming stall: the pipeline holds the
 * MAIN bundle's AST (and its millions of live cache keys) while the
 * validate/reconcile/sweep/split passes each re-parse another full-bundle
 * string on top. The first stress harness (stress.mts) dropped every AST
 * per cycle — ephemeron tables shed cleanly and it only measured GC
 * sawtooth, the same before and after the fix.
 *
 * Shape here:
 *   1. parse MAIN (2.1.207) and fill the caches (structural signature) —
 *      the naming-era state, held LIVE for the whole run;
 *   2. loop: re-parse the sibling bundle (2.1.208), signature it, drop it —
 *      the validate/reconcile/sweep/split-analog re-parses.
 *
 * Arms:
 *   --raw   parse with @babel/core parseSync directly = the OLD call sites
 *           (pre-funnel). Expected: iteration times degrade severely — the
 *           46s/82s/hang class of exp030.
 *   (none)  parse through parseFileAst = the funnel. Each big parse starts a
 *           fresh cache era. Expected: flat iterations.
 *
 * NODE_OPTIONS=--max-old-space-size=14336 npx tsx stress-live-main.mts [iters] [--raw]
 * Run under shell `timeout`; a quadratic iteration cannot self-interrupt.
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { computeStructuralSignature } from "../../src/analysis/structural-hash.js";
import { parseFileAst, traverse } from "../../src/babel-utils.js";

const V =
  process.env.VERSIONS_ROOT ??
  "/Users/andrewgross/Development/unpacked-claude-code-run-2026-07-17/versions";
const mainCode = fs.readFileSync(
  `${V}/claude-code-2.1.207/.humanify/humanified.js`,
  "utf8"
);
const otherCode = fs.readFileSync(
  `${V}/claude-code-2.1.208/.humanify/humanified.js`,
  "utf8"
);
const iters = Number(process.argv[2] ?? 5);
const raw = process.argv.includes("--raw");

const parse = (code: string): t.File => {
  const ast = raw
    ? (parseSync(code, {
        sourceType: "unambiguous",
        configFile: false,
        babelrc: false
      }) as t.File | null)
    : parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  return ast;
};

function signature(ast: t.File): string {
  let sig = "";
  traverse(ast, {
    Program(path) {
      sig = computeStructuralSignature(path).slice(0, 8);
      path.stop();
    }
  });
  return sig;
}

console.log(
  `arm: ${raw ? "RAW (old call sites)" : "FUNNEL (fix)"}  ` +
    `main ${(mainCode.length / 1e6).toFixed(1)}MB held live, ` +
    `${iters} re-parse iterations of ${(otherCode.length / 1e6).toFixed(1)}MB`
);

const t0 = performance.now();
const mainAst = parse(mainCode); // HELD LIVE for the whole run
const mainSig = signature(mainCode.length > 0 ? mainAst : mainAst);
console.log(
  `main era: ${Math.round(performance.now() - t0)}ms (sig ${mainSig})`
);

for (let i = 0; i < iters; i++) {
  const t1 = performance.now();
  const ast = parse(otherCode);
  const sig = signature(ast);
  const ms = Math.round(performance.now() - t1);
  const rss = Math.round(process.memoryUsage().rss / 1048576);
  console.log(`iter ${i + 1}: ${ms}ms  rss ${rss}MB  (sig ${sig})`);
}
// Keep mainAst demonstrably alive past the loop.
console.log(`done (main still live: ${mainAst.type})`);
