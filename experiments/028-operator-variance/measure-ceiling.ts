/**
 * Operator-variance ceiling measurement.
 *
 * Question: how many v1 functions that currently have NO same-structural-hash
 * twin in v2 would gain one if the hash were invariant to semantics-preserving
 * operator forms (`!0`/`true`, `!1`/`false`, `void 0`/`undefined`,
 * `a["lit"]`/`a.lit`)? That count is the absolute ceiling on what operator
 * normalization could buy the cross-version matcher (RAW parse — so it also
 * counts void0/member forms the production beautifier already fixes; a small
 * ceiling here therefore bounds production even tighter).
 *
 * Usage:
 *   node --max-old-space-size=8192 --expose-gc --import tsx/esm \
 *     experiments/028-operator-variance/measure-ceiling.ts <v1.js> <v2.js>
 */

import { readFileSync } from "node:fs";
import * as t from "@babel/types";
import { parseFileAst, traverse } from "../../src/babel-utils.js";
import { computeStructuralHash } from "../../src/analysis/structural-hash.js";

interface Rewrites {
  boolTrue: number;
  boolFalse: number;
  voidUndef: number;
  memberDot: number;
}

/** Rewrite the 4 semantics-preserving operator forms in place. */
function normalizeOperators(ast: t.File): Rewrites {
  const r: Rewrites = { boolTrue: 0, boolFalse: 0, voidUndef: 0, memberDot: 0 };
  traverse(ast, {
    UnaryExpression(p) {
      const n = p.node;
      if (n.operator === "!" && t.isNumericLiteral(n.argument)) {
        if (n.argument.value === 0) {
          p.replaceWith(t.booleanLiteral(true));
          r.boolTrue++;
        } else if (n.argument.value === 1) {
          p.replaceWith(t.booleanLiteral(false));
          r.boolFalse++;
        }
      } else if (
        n.operator === "void" &&
        t.isNumericLiteral(n.argument) &&
        n.argument.value === 0
      ) {
        p.replaceWith(t.identifier("undefined"));
        r.voidUndef++;
      }
    },
    MemberExpression(p) {
      const n = p.node;
      if (
        n.computed &&
        t.isStringLiteral(n.property) &&
        t.isValidIdentifier(n.property.value)
      ) {
        n.computed = false;
        n.property = t.identifier(n.property.value);
        r.memberDot++;
      }
    }
  });
  return r;
}

/** Baseline + operator-normalized structural hash for every function, aligned
 * by traversal order (normalization adds/removes no functions). */
function hashesOf(code: string): {
  base: string[];
  norm: string[];
  rewrites: Rewrites;
} {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  const fns: Array<import("@babel/traverse").NodePath<t.Function>> = [];
  traverse(ast, {
    Function(p) {
      fns.push(p);
    }
  });
  const base = fns.map((p) => computeStructuralHash(p));
  const rewrites = normalizeOperators(ast);
  const norm = fns.map((p) => computeStructuralHash(p));
  return { base, norm, rewrites };
}

const [f1, f2] = process.argv.slice(2);
if (!f1 || !f2) throw new Error("usage: measure-ceiling.ts <v1.js> <v2.js>");

console.error(`hashing v2: ${f2}`);
const v2 = hashesOf(readFileSync(f2, "utf8"));
const v2Base = new Set(v2.base);
const v2Norm = new Set(v2.norm);
console.error(`  v2 functions: ${v2.base.length}, rewrites:`, v2.rewrites);
global.gc?.();

console.error(`hashing v1: ${f1}`);
const v1 = hashesOf(readFileSync(f1, "utf8"));
console.error(`  v1 functions: ${v1.base.length}, rewrites:`, v1.rewrites);

let baseMatch = 0;
let normMatch = 0;
let recovered = 0;
for (let i = 0; i < v1.base.length; i++) {
  const hasBase = v2Base.has(v1.base[i]);
  const hasNorm = v2Norm.has(v1.norm[i]);
  if (hasBase) baseMatch++;
  if (hasNorm) normMatch++;
  if (!hasBase && hasNorm) recovered++;
}

const n = v1.base.length;
const pct = (x: number) => `${((100 * x) / n).toFixed(2)}%`;
console.log("\n=== operator-variance ceiling (v1 → v2, same minifier) ===");
console.log(`v1 functions:                    ${n}`);
console.log(
  `base-matchable (v2 same-hash):   ${baseMatch} (${pct(baseMatch)})`
);
console.log(
  `norm-matchable:                  ${normMatch} (${pct(normMatch)})`
);
console.log(
  `RECOVERED by operator-normalize: ${recovered} (${pct(recovered)})`
);
console.log(
  `  → ceiling: operator normalization could newly hash-match ${recovered} v1 functions`
);
