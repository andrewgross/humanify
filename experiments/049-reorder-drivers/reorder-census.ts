/**
 * 049 — what the reorder charge is made of, over a whole release pair.
 *
 *   npx tsx experiments/049-reorder-drivers/reorder-census.ts <priorSrc> <freshSrc>
 *
 * Totals the KPI's own reorder charge by the KIND of statement that got
 * displaced, so a strategy can be aimed at the shape that actually costs money
 * instead of at the most numerous one. Those differ here by two orders of
 * magnitude: one displaced `lazyInitializer` block costs more than a hundred
 * displaced one-line accessors.
 *
 * Uses `statementsOf` / `onLcs` from the scoring classifier, so the census sums
 * to the `reorder` column rather than to a lookalike of it.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import {
  onLcs,
  statementsOf,
  type Stmt
} from "../037-noise-source-decomposition/diff-composition.js";

const [PRIOR, FRESH] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: reorder-census.ts <priorSrc> <freshSrc>");
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

/** What kind of top-level statement this is — the axis a fix would target. */
function kind(s: Stmt): string {
  const t = s.text;
  if (/^Object\.defineProperty\(module\.exports/.test(t))
    return "export-accessor";
  if (/^\s*(?:async\s+)?function\s/.test(t)) return "function-decl";
  if (/^\s*class\s/.test(t)) return "class-decl";
  if (/^\s*(?:var|let|const)\s+[\w$]+\s*=\s*require\(/.test(t))
    return "require";
  if (/lazyInitializer\)?\(/.test(t)) return "lazyInitializer";
  if (/^\s*(?:var|let|const)\s+[\w$]+\s*=\s*(?:\(0,\s*)?[\w.$]+\)?\(/.test(t))
    return "other init-CALL";
  if (/^\s*(?:var|let|const)\s[^=]*;\s*$/.test(t)) return "var-decl-only";
  if (/^\s*(?:var|let|const)\s/.test(t)) return "var-init (literal)";
  if (/^\s*\(?\s*\(0,/.test(t) || /^\s*[\w.$]+\(/.test(t)) return "bare-CALL";
  return "other";
}

const byKind = new Map<string, { n: number; ln: number }>();
const worst: Array<{ file: string; ln: number; kind: string; head: string }> =
  [];
let total = 0;

for (const f of walk(FRESH)) {
  const pf = path.join(PRIOR, f);
  if (!fs.existsSync(pf)) continue;
  const prior = statementsOf(fs.readFileSync(pf, "utf8"));
  const fresh = statementsOf(fs.readFileSync(path.join(FRESH, f), "utf8"));
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
  fm.forEach((s, i) => {
    if (inOrder.has(i)) return;
    const ln = s.lines.length * 2;
    total += ln;
    const k = kind(s);
    const e = byKind.get(k) ?? { n: 0, ln: 0 };
    byKind.set(k, { n: e.n + 1, ln: e.ln + ln });
    worst.push({
      file: f,
      ln,
      kind: k,
      head: s.text.split("\n")[0].slice(0, 74).replace(/\s+/g, " ")
    });
  });
}

console.log(`TOTAL reorder charge: ${total} git lines\n`);
console.log("  by kind of displaced statement:");
for (const [k, v] of [...byKind.entries()].sort((a, b) => b[1].ln - a[1].ln)) {
  const pct = ((100 * v.ln) / total).toFixed(1).padStart(5);
  console.log(
    `  ${String(v.ln).padStart(5)}ln ${pct}%  ${String(v.n).padStart(4)} instance(s)  avg ${String(Math.round(v.ln / v.n)).padStart(3)}ln  ${k}`
  );
}
console.log("\n  the 12 most expensive single displacements:");
for (const w of worst.sort((a, b) => b.ln - a.ln).slice(0, 12)) {
  console.log(
    `  ${String(w.ln).padStart(4)}ln  ${w.kind.padEnd(16)} ${w.file}`
  );
}
