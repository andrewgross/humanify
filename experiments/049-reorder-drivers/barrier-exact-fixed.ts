/**
 * 049 — the constrained share of reorder churn, with the purity exemption
 * ACTUALLY APPLIED.
 *
 *   npx tsx experiments/049-reorder-drivers/barrier-exact-fixed.ts <priorSrc> <freshSrc> <label>
 *
 * ## Why this exists — exp045's "exact" ceiling measured the wrong model
 *
 * `045/barrier-exact.ts` replaced Task A's syntactic proxy with
 * `bundleLoadOrderFacts`, correctly, and its comment says it consults "the same
 * facts the emitter's aligner consults". It does not.
 *
 * `bundleLoadOrderFacts` admits the bundler's lazy-init wrapper as pure by
 * finding its DEFINITION — the `x && (y = x(x = 0))` shape — in the code it is
 * handed. The emitter hands it the whole BUNDLE, where the helper is defined, so
 * the exemption applies. exp045 hands it ONE SPLIT FILE, and the helper is
 * defined in exactly one file (`array-builder/resource-lifecycle.js`); everywhere
 * else it is only CALLED, as `(0, resourceLifecycle.lazyInitializer)(...)`. So
 * `identifyBunLazyInit` returns null, `pureCallNames` is empty, and every
 * lazy-init registration is counted as an effect barrier — the exact block
 * exp038 built the exemption for, and which `load-order.ts` calls "the largest
 * pinned block of reorder churn".
 *
 * This is the same failure exp045 itself documented in Task A (measurement
 * pitfalls rule 4: reasoning about a proxy instead of running the real model),
 * one level up: it ran the real model, in the wrong scope.
 *
 * So: same logic, but the wrapper name is resolved ACROSS the tree and passed in,
 * reproducing what the emitter sees. Prints both numbers so the gap is explicit.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { identifyBunLazyInit } from "../../src/shared/bun-helpers.js";
import {
  analyzeLoadOrder,
  bundleLoadOrderFacts
} from "../../src/split/load-order.js";
import { statementHash } from "../../src/split/statement-hash.js";
import { onLcs } from "../037-noise-source-decomposition/diff-composition.js";

const [PRIOR, FRESH, LABEL] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: barrier-exact-fixed.ts <priorSrc> <freshSrc> [label]");
  process.exit(1);
}

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/**
 * The lazy-init wrapper's name, resolved across the whole tree.
 *
 * Post-split the definition sits in one file, so this scans every file for the
 * shape and returns the single name it finds. Structural, exactly as
 * `bundleLoadOrderFacts` does it on the bundle — never a hardcoded identifier,
 * because the name is LLM-chosen and differs run to run.
 */
function lazyInitNameOf(root: string, files: string[]): string | null {
  for (const f of files) {
    const n = identifyBunLazyInit(fs.readFileSync(path.join(root, f), "utf8"));
    if (n) return n;
  }
  return null;
}

interface Stmt {
  hash: string;
  text: string;
  lines: number;
  effects: boolean;
}

function statementsOf(code: string, pure: string | null): Stmt[] {
  let ast: ReturnType<typeof parseSync>;
  try {
    ast = parseSync(code, { sourceType: "unambiguous" });
  } catch {
    return [];
  }
  if (!ast || ast.type !== "File") return [];
  const body = ast.program.body as t.Statement[];
  // The whole point: supply the wrapper name the file itself cannot reveal.
  const names = [pure, extraPure].filter((x): x is string => !!x);
  const facts = names.length
    ? analyzeLoadOrder(body, { pureCallNames: new Set(names) })
    : bundleLoadOrderFacts(body, code);
  return body.map((s, i) => {
    const text =
      s.start != null && s.end != null ? code.slice(s.start, s.end) : "";
    return {
      hash: statementHash(s),
      text,
      lines: text ? text.split("\n").length : 0,
      effects: facts[i]?.effects ?? true
    };
  });
}

/** Reorder charge split into constrained vs recoverable, for one purity setting. */
function measureBoth(a: string | null, b: string) {
  const saved = extraPure;
  extraPure = b;
  const r = measure(a);
  extraPure = saved;
  return r;
}
let extraPure: string | null = null;

function measure(pure: string | null) {
  let total = 0;
  let barrierItself = 0;
  let blocked = 0;
  const recoverable: Array<{ file: string; ln: number; head: string }> = [];

  for (const f of walk(FRESH)) {
    const pf = path.join(PRIOR, f);
    if (!fs.existsSync(pf)) continue;
    const prior = statementsOf(fs.readFileSync(pf, "utf8"), pure);
    const fresh = statementsOf(
      fs.readFileSync(path.join(FRESH, f), "utf8"),
      pure
    );
    const key = (s: Stmt) => `${s.hash} ${s.text}`;

    const avail = new Map<string, number>();
    for (const s of prior) avail.set(key(s), (avail.get(key(s)) ?? 0) + 1);
    const fm: Stmt[] = [];
    for (const s of fresh) {
      const n = avail.get(key(s)) ?? 0;
      if (n > 0) {
        avail.set(key(s), n - 1);
        fm.push(s);
      }
    }
    const still = new Map(avail);
    const pm: Stmt[] = [];
    for (const s of prior) {
      const n = still.get(key(s)) ?? 0;
      if (n > 0) still.set(key(s), n - 1);
      else pm.push(s);
    }
    const inOrder = onLcs(pm.map(key), fm.map(key));
    // Index of each matched fresh statement in the prior's matched sequence.
    const priorIdx = new Map<string, number[]>();
    pm.forEach((s, i) => {
      const l = priorIdx.get(key(s)) ?? [];
      l.push(i);
      priorIdx.set(key(s), l);
    });

    fm.forEach((s, i) => {
      if (inOrder.has(i)) return;
      const ln = s.lines * 2;
      total += ln;
      if (s.effects) {
        barrierItself += ln;
        return;
      }
      // Would restoring it to its prior slot cross a barrier?
      const at = priorIdx.get(key(s));
      const from = at?.shift();
      const lo = Math.min(from ?? i, i);
      const hi = Math.max(from ?? i, i);
      let crosses = false;
      for (let j = lo; j <= hi && j < fm.length; j++) {
        if (j !== i && fm[j].effects) {
          crosses = true;
          break;
        }
      }
      if (crosses) blocked += ln;
      else
        recoverable.push({
          file: f,
          ln,
          head: s.text.split("\n")[0].slice(0, 76).replace(/\s+/g, " ")
        });
    });
  }
  const rec = recoverable.reduce((a, r) => a + r.ln, 0);
  return { total, barrierItself, blocked, rec, recoverable };
}

const files = walk(FRESH);
const pure = lazyInitNameOf(FRESH, files);
console.log(`=== 049 CORRECTED BARRIER ANALYSIS — ${LABEL ?? ""} ===`);
console.log(
  `  lazy-init wrapper resolved across the tree: ${pure ?? "NOT FOUND"}`
);

const before = measure(null);
const after = measure(pure);
// STRATEGY PROBE: what if the export-registration helper were also admitted?
// Its body installs LAZY getters (`get: source[k]` over a literal of arrow
// thunks), so nothing it is given is evaluated at registration — only the target
// object must already exist. Sizing only; production would have to verify the
// shape structurally, as identifyBunLazyInit does.
const EXPORTS_HELPER = process.env.EXPORTS_HELPER ?? "defineModuleExports";
const alsoExports = measureBoth(pure, EXPORTS_HELPER);
const pct = (n: number, d: number) =>
  `${((100 * n) / d).toFixed(1)}%`.padStart(6);

console.log(`\n  reorder charge: ${before.total} git lines\n`);
console.log(
  `  exp045's setting (wrapper NOT admitted — per-file detection fails):`
);
console.log(
  `    CONSTRAINED ${before.barrierItself + before.blocked} ln (${pct(before.barrierItself + before.blocked, before.total)})  = barrier itself ${before.barrierItself} + blocked ${before.blocked}`
);
console.log(
  `    RECOVERABLE ${before.rec} ln (${pct(before.rec, before.total)}) in ${before.recoverable.length} statements`
);
console.log(`\n  what the EMITTER actually sees (wrapper admitted as pure):`);
console.log(
  `    CONSTRAINED ${after.barrierItself + after.blocked} ln (${pct(after.barrierItself + after.blocked, after.total)})  = barrier itself ${after.barrierItself} + blocked ${after.blocked}`
);
console.log(
  `    RECOVERABLE ${after.rec} ln (${pct(after.rec, after.total)}) in ${after.recoverable.length} statements`
);
console.log(
  `\n  difference the exemption makes: ${after.rec - before.rec} git lines`
);
console.log(
  `\n  IF the export-registration helper were also admitted (strategy probe):`
);
console.log(
  `    CONSTRAINED ${alsoExports.barrierItself + alsoExports.blocked} ln (${pct(alsoExports.barrierItself + alsoExports.blocked, alsoExports.total)})`
);
console.log(
  `    RECOVERABLE ${alsoExports.rec} ln (${pct(alsoExports.rec, alsoExports.total)}) in ${alsoExports.recoverable.length} statements`
);
console.log(`\n  largest recoverable under the corrected model:`);
for (const r of after.recoverable.sort((a, b) => b.ln - a.ln).slice(0, 12)) {
  console.log(`    ${String(r.ln).padStart(4)}ln  ${r.file}  ${r.head}`);
}
