// Residual differences after full normalisation: for module pairs (same app/mode/pkg/file) in two bundler configs,
// find functions of A with no exact L3 match in B, pair each with its most similar B function, and dump both
// canonical token streams for diffing (scripts/residual_diff.py).
//
// usage: node scripts/residual.mjs <configA> <configB> <maxModules> <out.jsonl> [offList,comma,separated]
import fs from 'node:fs';
import path from 'node:path';
import { parseAny } from './scan.mjs';
import { ablateUnit, alphaTokens } from './canon.mjs';

const LAB = '/work/exp095-library-fingerprints/lab';
const [specA, specB, maxMods, outFile, offArg] = process.argv.slice(2);
const [cfgA, modeA] = specA.split(':');
const [cfgB, modeB] = specB.split(':');
const byMode = !!modeA;
const off = offArg ? offArg.split(',') : [];

const rows = fs.readFileSync(LAB + '/labels/all.jsonl', 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
const keyOf = (r) => (byMode ? [r.app, r.pkg, r.pkgVersion, r.file] : [r.app, r.mode, r.pkg, r.pkgVersion, r.file]).join('|');
const A = new Map(),
  B = new Map();
for (const r of rows) {
  if (!r.minified || r.pkg.startsWith('<') || r.parts !== 1 || (r.purity ?? 1) < 0.9) continue;
  const cfg = r.build.split('/')[0];
  if (cfg === cfgA && (!modeA || r.mode === modeA)) A.set(keyOf(r), r);
  if (cfg === cfgB && (!modeB || r.mode === modeB)) B.set(keyOf(r), r);
}
// deterministic sample: every k-th shared key
const shared = [...A.keys()].filter((k) => B.has(k)).sort();
const step = Math.max(1, Math.floor(shared.length / +maxMods));
const sample = shared.filter((_, i) => i % step === 0).slice(0, +maxMods);

function fnsOf(r) {
  const src = fs.readFileSync(path.join(LAB, r.textPath), 'utf8');
  const ast = parseAny(src);
  const out = [];
  for (const st of ast.body) {
    const a = ablateUnit(st, src, off, 30);
    for (const f of a.fns) {
      // re-derive the function's own token stream
      out.push({ h: f[3], n: f[2], toks: null, st, s: f[0], e: f[1], src });
    }
  }
  return out;
}
function tokensOfFn(f) {
  // re-serialize just this function node by finding it in the statement
  let node = null;
  (function walk(n) {
    if (node || !n || typeof n !== 'object') return;
    if (Array.isArray(n)) return n.forEach(walk);
    if (n.start === f.s && n.end === f.e && /Function|Class/.test(n.type)) {
      node = n;
      return;
    }
    for (const k in n) if (k !== 'start' && k !== 'end' && k !== 'type') walk(n[k]);
  })(f.st);
  const a = ablateUnit(node, f.src, off, 1);
  return alphaTokens(a.toks);
}
function bagSim(x, y) {
  const c = new Map();
  for (const t of x) c.set(t, (c.get(t) || 0) + 1);
  let inter = 0;
  for (const t of y) {
    const v = c.get(t);
    if (v) {
      inter++;
      c.set(t, v - 1);
    }
  }
  return (2 * inter) / (x.length + y.length);
}
const out = fs.createWriteStream(outFile);
let stats = { modules: 0, fnsA: 0, matched: 0, unmatched: 0 };
for (const k of sample) {
  let fa, fb;
  try {
    fa = fnsOf(A.get(k));
    fb = fnsOf(B.get(k));
  } catch (e) {
    continue;
  }
  stats.modules++;
  const hb = new Map();
  for (const f of fb) hb.set(f.h, (hb.get(f.h) || 0) + 1);
  const unB = fb.filter((f) => true);
  for (const f of fa) {
    stats.fnsA++;
    if (hb.get(f.h)) {
      hb.set(f.h, hb.get(f.h) - 1);
      stats.matched++;
      continue;
    }
    stats.unmatched++;
    // only report "leaf-ish" functions (< 400 tokens) so the diff is readable
    if (f.n > 400) continue;
    const ta = tokensOfFn(f);
    let best = null,
      bs = 0;
    for (const g of unB) {
      if (Math.abs(g.n - f.n) > f.n * 0.5) continue;
      const tb = g.toks ?? (g.toks = tokensOfFn(g));
      const s = bagSim(ta, tb);
      if (s > bs) {
        bs = s;
        best = tb;
      }
    }
    if (best && bs > 0.6) out.write(JSON.stringify({ key: k, sim: +bs.toFixed(3), a: ta, b: best }) + '\n');
  }
}
out.end();
console.log(JSON.stringify(stats));
