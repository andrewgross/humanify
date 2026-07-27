/**
 * 049 — does the noise decomposition match git PER FILE, or only in aggregate?
 *
 *   npx tsx experiments/049-reorder-drivers/per-file-fidelity.ts <priorSrc> <freshSrc>
 *
 * `diff-composition`'s parts summed to 29,525 git lines on 215->216 where git
 * itself prints 29,701 — 0.6% apart, which reads as "the decomposition is
 * faithful, use its shares". Two files say otherwise: `files-api.js` is charged
 * 332 reorder lines where git prints 4, and `colorizer.js` is charged 52 for a
 * displacement git does not print AT ALL (its 51 printed lines are new exports, a
 * new require, and a version bump).
 *
 * If per-file errors that large cancel in the total, the aggregate agreement is
 * arithmetic luck rather than fidelity, and every per-mechanism share drawn from
 * it inherits the error. This measures that directly: the SIGNED sum is what the
 * aggregate shows, the ABSOLUTE sum is the real per-file disagreement.
 *
 * Caveat on interpretation, in both directions: git minimises a LINE edit script
 * and the decomposition classifies STATEMENTS, so the two are not obliged to
 * agree per file even when both are correct — a statement-level account can be
 * the more faithful description of what changed while printing a different
 * number. What this establishes is the SIZE of the gap, and therefore how much
 * confidence a per-mechanism share deserves.
 */

import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { composeFile } from "../037-noise-source-decomposition/diff-composition.js";

const [PRIOR, FRESH] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: per-file-fidelity.ts <priorSrc> <freshSrc>");
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

function gitLines(a: string, b: string): number {
  try {
    execFileSync("diff", [a, b], { encoding: "utf8" });
    return 0;
  } catch (e) {
    const out = String((e as { stdout?: string }).stdout ?? "");
    return out.split("\n").filter((l) => /^[<>]/.test(l)).length;
  }
}

let signed = 0;
let abs = 0;
let gitTotal = 0;
let chargedTotal = 0;
let files = 0;
let reorderOver = 0;
const rows: Array<{ f: string; c: number; g: number; reorder: number }> = [];

for (const f of walk(FRESH)) {
  const pf = path.join(PRIOR, f);
  if (!fs.existsSync(pf)) continue;
  const ff = path.join(FRESH, f);
  const g = gitLines(pf, ff);
  const t = composeFile(
    fs.readFileSync(pf, "utf8"),
    fs.readFileSync(ff, "utf8")
  );
  const c = t.real + t.naming + t.alias + t.reorder + t.fileAddRemove;
  if (g === 0 && c === 0) continue;
  files++;
  gitTotal += g;
  chargedTotal += c;
  signed += c - g;
  abs += Math.abs(c - g);
  // Over-charge whose bulk is reorder: the shape both known outliers have.
  if (c > g && t.reorder > (c - g) / 2)
    reorderOver += Math.min(t.reorder, c - g);
  rows.push({ f, c, g, reorder: t.reorder });
}

console.log(`files compared: ${files}`);
console.log(`  git prints            ${gitTotal} lines`);
console.log(`  decomposition charges ${chargedTotal} lines`);
console.log(
  `  SIGNED error          ${signed > 0 ? "+" : ""}${signed}  <- what the aggregate shows`
);
console.log(
  `  ABSOLUTE error        ${abs} (${((100 * abs) / gitTotal).toFixed(1)}% of git)  <- the real disagreement`
);
console.log(
  `  cancellation factor   ${(abs / Math.max(1, Math.abs(signed))).toFixed(1)}x`
);
console.log(
  `\n  over-charge whose bulk is reorder: ~${reorderOver} lines (a ceiling on phantom reorder)`
);
console.log(`\n  worst OVER-charged files:`);
for (const w of rows.sort((a, b) => b.c - b.g - (a.c - a.g)).slice(0, 10)) {
  console.log(
    `    charged ${String(w.c).padStart(5)}  git ${String(w.g).padStart(5)}  (reorder ${String(w.reorder).padStart(4)})  ${w.f}`
  );
}
console.log(`\n  worst UNDER-charged files:`);
for (const w of rows.sort((a, b) => a.c - a.g - (b.c - b.g)).slice(0, 6)) {
  console.log(
    `    charged ${String(w.c).padStart(5)}  git ${String(w.g).padStart(5)}  ${w.f}`
  );
}
