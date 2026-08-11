/** Read-only: classify name-only pairs in 85->86 REAL statements by mechanism. */
import {
  composeDiff,
  type NoiseSample
} from "../037-noise-source-decomposition/diff-composition.js";
import { tokenizeLine } from "../../src/rename/diff-reconcile.js";

const PRIOR = "/tmp/eval-work/noise-band-r1/2.1.85-rebased/src";
const FRESH = "/tmp/eval-work/noise-band-r1/2.1.86/src";

function pairsIn(a: string, b: string): [string, string][] {
  const pa = a.split("\n"),
    pb = b.split("\n");
  const sa = new Set(pa),
    sb = new Set(pb);
  const rem = pa.filter((l) => !sb.has(l)),
    add = pb.filter((l) => !sa.has(l));
  const k = Math.min(rem.length, add.length);
  const out: [string, string][] = [];
  for (let i = 0; i < k; i++) out.push([rem[i], add[i]]);
  return out;
}

const samples: NoiseSample[] = [];
composeDiff(PRIOR, FRESH, { samples, cap: 500_000 });

const mech = new Map<string, { lines: number; pairs: number; ex: string[] }>();
function bump(m: string, ex: string) {
  const e = mech.get(m) ?? { lines: 0, pairs: 0, ex: [] };
  e.lines += 2;
  e.pairs++;
  if (e.ex.length < 4) e.ex.push(ex);
  mech.set(m, e);
}

const ORD = /^(.*?)(\d+)$/;
for (const s of samples.filter((x) => x.kind === "real")) {
  if (s.priorText === undefined || s.freshText === undefined) continue;
  for (const [a, b] of pairsIn(s.priorText, s.freshText)) {
    const ta = tokenizeLine(a),
      tb = tokenizeLine(b);
    if (!ta || !tb || ta.length !== tb.length) continue;
    let ok = true;
    const diffs: [string, string][] = [];
    for (let i = 0; i < ta.length; i++) {
      if (
        ta[i].kind !== tb[i].kind ||
        (ta[i].text !== tb[i].text && ta[i].kind !== "ident")
      ) {
        ok = false;
        break;
      }
      if (ta[i].text !== tb[i].text) diffs.push([ta[i].text, tb[i].text]);
    }
    if (!ok || diffs.length === 0) continue;
    // Classify by the WORST (least similar) identifier change on the line.
    let cls = "ordinal-only";
    for (const [x, y] of diffs) {
      const mx = ORD.exec(x),
        my = ORD.exec(y);
      if (mx && my && mx[1] === my[1] && mx[1] !== "") continue; // same stem, ordinal changed
      if (mx && mx[1] === y) continue; // ordinal dropped
      if (my && my[1] === x) continue; // ordinal added
      const sx = x.toLowerCase(),
        sy = y.toLowerCase();
      if (sx.includes(sy) || sy.includes(sx)) {
        if (cls === "ordinal-only") cls = "stem-contained";
        continue;
      }
      cls = "different-word";
    }
    bump(cls, `${diffs[0][0]} -> ${diffs[0][1]}`);
  }
}

let total = 0;
for (const [, v] of mech) total += v.lines;
console.log(`total name-only lines classified: ${total}`);
for (const [k, v] of [...mech.entries()].sort(
  (p, q) => q[1].lines - p[1].lines
)) {
  console.log(
    `  ${k.padEnd(16)} ${String(v.lines).padStart(6)} ln  (${v.pairs} pairs)  e.g. ${v.ex.slice(0, 3).join(" | ")}`
  );
}

// How concentrated: unique identifier pairs vs total occurrences.
const idPairs = new Map<string, number>();
for (const s of samples.filter((x) => x.kind === "real")) {
  if (s.priorText === undefined || s.freshText === undefined) continue;
  for (const [a, b] of pairsIn(s.priorText, s.freshText)) {
    const ta = tokenizeLine(a),
      tb = tokenizeLine(b);
    if (!ta || !tb || ta.length !== tb.length) continue;
    let ok = true;
    const diffs: [string, string][] = [];
    for (let i = 0; i < ta.length; i++) {
      if (
        ta[i].kind !== tb[i].kind ||
        (ta[i].text !== tb[i].text && ta[i].kind !== "ident")
      ) {
        ok = false;
        break;
      }
      if (ta[i].text !== tb[i].text) diffs.push([ta[i].text, tb[i].text]);
    }
    if (!ok) continue;
    for (const [x, y] of diffs)
      idPairs.set(`${x} -> ${y}`, (idPairs.get(`${x} -> ${y}`) ?? 0) + 1);
  }
}
console.log(`\nunique identifier pairs: ${idPairs.size}`);
const sorted = [...idPairs.entries()].sort((a, b) => b[1] - a[1]);
let cum = 0;
const totalOcc = sorted.reduce((s, [, n]) => s + n, 0);
for (let i = 0; i < sorted.length && cum < totalOcc * 0.5; i++)
  cum += sorted[i][1];
console.log(
  `occurrences: ${totalOcc}; top pairs covering 50%: ${
    sorted.filter((_, i) => {
      let c = 0;
      for (let j = 0; j <= i; j++) c += sorted[j][1];
      return c <= totalOcc * 0.5 + sorted[i][1];
    }).length
  }`
);
console.log("top 10:");
for (const [k, n] of sorted.slice(0, 10))
  console.log(`  ${String(n).padStart(4)}x  ${k}`);
