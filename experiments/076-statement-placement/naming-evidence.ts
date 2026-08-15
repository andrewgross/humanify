/**
 * 076 addendum — how much evidence is there to name a file FROM?
 *
 *   npx tsx --max-old-space-size=32768 \
 *     experiments/076-statement-placement/naming-evidence.ts <bundle>
 *
 * Andrew's proposal (2026-08-15): name a file with an LLM from a SAMPLING of
 * its exported function names, and reuse that name across versions via module
 * identity, instead of taking the file stem from the module's first hoisted
 * declaration.
 *
 * The stability case for it rests entirely on BREADTH. Today's stem is a
 * function of ONE identifier, so one drifted name renames the file and moves
 * every statement in it. A name summarising N symbols only drifts when many
 * of them do — IF N is large. If most modules declare one or two things, the
 * sampling degenerates to today's scheme, and the LLM's own re-roll variance
 * (exp052: two cold legs disagree on 33.4% of decisions by a different word)
 * lands on top of it with nothing averaging it out. Then the change makes
 * names better and stability WORSE.
 *
 * So this counts the evidence before anything is built. It reads structure
 * only — declaration and statement counts, and how many other modules import
 * each one — none of which depends on identifier text, so a raw minified
 * bundle answers it exactly.
 */
import * as fs from "node:fs";
import * as t from "@babel/types";
import { parseFileAst } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { extractFossilModules } from "../../src/split/fossil-map.js";
import { statementHash } from "../../src/split/statement-hash.js";

const [BUNDLE] = process.argv.slice(2);
if (!BUNDLE) {
  console.error("usage: naming-evidence.ts <bundle>");
  process.exit(1);
}

const code = fs.readFileSync(BUNDLE, "utf8");
const ast = parseFileAst(code);
if (!ast) throw new Error("parse failed");
const wrapper = findWrapperFunction(ast);
if (!wrapper) throw new Error("no wrapper IIFE");
const bodyNode = wrapper.functionPath.node.body;
if (!t.isBlockStatement(bodyNode)) throw new Error("no block");
const body = bodyNode.body;
const extract = extractFossilModules(body, body.map(statementHash));
const modules = extract.modules;

/** Declarations excluding the module's own init def, which is always last and
 * is bundler plumbing rather than something the file "exports". */
const namable = modules.map((m) => Math.max(0, m.declared.length - 1));
/** Top-level function/class declarations — the strongest naming evidence,
 * and what "exported function names" means in this bundle's shape. */
const funcs = modules.map(
  (m) =>
    m.statements.filter((i) => {
      const s = body[i];
      return (
        (s.type === "FunctionDeclaration" || s.type === "ClassDeclaration") &&
        s.id !== null
      );
    }).length
);
const importerCount = new Map<number, number>();
modules.forEach((m, i) => {
  for (const imp of new Set(m.imports)) {
    if (imp === i) continue;
    importerCount.set(imp, (importerCount.get(imp) ?? 0) + 1);
  }
});

function histogram(label: string, values: number[]): void {
  const buckets: Array<[string, (n: number) => boolean]> = [
    ["0", (n) => n === 0],
    ["1", (n) => n === 1],
    ["2", (n) => n === 2],
    ["3-5", (n) => n >= 3 && n <= 5],
    ["6-10", (n) => n >= 6 && n <= 10],
    ["11+", (n) => n >= 11]
  ];
  console.log(`\n${label} (n=${values.length})`);
  for (const [name, test] of buckets) {
    const n = values.filter(test).length;
    const pct = ((100 * n) / values.length).toFixed(1);
    const bar = "#".repeat(Math.round(Number(pct) / 2));
    console.log(
      `  ${name.padEnd(5)} ${String(n).padStart(5)}  ${pct.padStart(5)}%  ${bar}`
    );
  }
  const sorted = [...values].sort((a, b) => a - b);
  console.log(
    `  median ${sorted[Math.floor(sorted.length / 2)]}   mean ${(
      values.reduce((a, b) => a + b, 0) / values.length
    ).toFixed(2)}`
  );
}

console.log(
  `modules: ${modules.length}, eager statements: ${extract.eagerZone.length}`
);
histogram("NAMABLE DECLARATIONS per module (init def excluded)", namable);
histogram("TOP-LEVEL function/class declarations per module", funcs);
histogram(
  "STATEMENTS per module",
  modules.map((m) => m.statements.length)
);
histogram(
  "IMPORTERS per module (how many files reference it)",
  modules.map((_, i) => importerCount.get(i) ?? 0)
);

const oneSymbol = funcs.filter((n) => n <= 1).length;
const broad = funcs.filter((n) => n >= 3).length;
console.log(
  `\nVERDICT INPUTS:\n` +
    `  modules with <=1 function/class to name from: ${oneSymbol} ` +
    `(${((100 * oneSymbol) / modules.length).toFixed(1)}%) — sampling adds NOTHING here,\n` +
    `    the name is a function of one identifier either way\n` +
    `  modules with >=3: ${broad} (${((100 * broad) / modules.length).toFixed(1)}%) — ` +
    `the population where a sampled name could be more stable than one symbol`
);
