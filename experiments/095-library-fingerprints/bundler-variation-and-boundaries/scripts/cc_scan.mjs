// Fingerprint Claude Code's raw bundle with the same unit model as the lab, and attach the lab's real-world truth:
// positives (vendor factories with a declared identity) -> their package, negatives (app module groups) -> <app>,
// everything else -> <none> (unknown; reported, never scored).
// usage: node --max-old-space-size=30000 scripts/cc_scan.mjs <ver>
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseAny, unitsOf } from './scan.mjs';
import { canonUnit, longStrings, hashStr } from './canon.mjs';

const ver = process.argv[2];
const LAB = '/work/exp095-library-fingerprints/lab';
const OUT = process.env.CC_UNITS_OUT || path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/units/cc');
const bundle = `/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-${ver}/binary-decompiled/src/entrypoints/index.js`;
const src = fs.readFileSync(bundle, 'utf8');

const labels = [];
function addSet(file, mk) {
  // the lab's real-world truth was deleted 2026-10-25: without it the units are still the population (exp150)
  if (!fs.existsSync(`${LAB}/realworld/cc-${ver}/${file}`)) return console.error(file, 'absent: units without lab labels');
  const rows = fs.readFileSync(`${LAB}/realworld/cc-${ver}/${file}`, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
  let miss = 0;
  for (const r of rows) {
    const text = fs.readFileSync(`${LAB}/${r.textPath}`, 'utf8');
    const at = src.indexOf(text);
    if (at < 0) {
      miss++;
      continue;
    }
    labels.push({ ...mk(r), startChar: at, endChar: at + text.length });
  }
  console.error(file, rows.length, 'located', rows.length - miss);
}
addSet('positives.jsonl', (r) => ({ pkg: r.pkg, pkgVersion: r.pkgVersion, file: r.member || r.identitySource, kind: 'cjs' }));
addSet('negatives.jsonl', (r) => ({ pkg: '<app>', pkgVersion: null, file: r.treeFile, kind: r.kind }));
labels.sort((a, b) => a.startChar - b.startChar);
labels.forEach((l, i) => (l.idx = i));
const findLabel = (c) => {
  // labels may nest (none expected); linear-ish binary search on start
  let lo = 0,
    hi = labels.length - 1,
    ans = -1;
  while (lo <= hi) {
    const m = (lo + hi) >> 1;
    if (labels[m].startChar <= c) {
      ans = m;
      lo = m + 1;
    } else hi = m - 1;
  }
  return ans >= 0 && c < labels[ans].endChar ? ans : -1;
};

const t0 = Date.now();
const ast = parseAny(src);
console.error('parsed', ((Date.now() - t0) / 1000).toFixed(1), 's');
const rows = [];
unitsOf(ast, src, (node, depth, parent, i, inner) => {
  const c = canonUnit(node, src);
  let fns = c.fns;
  if (inner && inner.length) {
    const a = inner[0].start,
      b = inner[inner.length - 1].end;
    fns = fns.filter((f) => !(f.s >= a && f.e <= b));
  }
  let wk = null;
  if (inner) {
    const call = node.type === 'VariableDeclaration' ? node.declarations[0].init : node.type === 'VariableDeclarator' ? node.init : node.expression;
    const fn = call.arguments[0];
    wk = fn.type === 'FunctionExpression' || fn.params.length ? 'cjs' : 'esm';
  }
  rows.push({
    wk, i, d: depth, p: parent, w: inner ? 1 : 0, s: node.start, e: node.end, lab: findLabel(node.start),
    h0: c.h0, h1: c.h1, h2: c.h2, h3: c.h3, h4: c.h4, sk: c.sk, bag: c.bag, n: c.n, skl: c.skLen, skd: c.skd,
    fns: fns.map((f) => [f.s, f.e, f.n, f.h1, f.h2, f.h3, f.h4, f.skd]),
    ls: inner ? [] : longStrings(node).map((s) => hashStr(s).slice(0, 12)),
  });
});
fs.mkdirSync(OUT + '/min', { recursive: true });
const base = `${OUT}/min/cc-${ver}`;
fs.mkdirSync(base, { recursive: true });
fs.writeFileSync(`${base}/raw.jsonl`, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
const lab = {};
for (const l of labels) lab[l.idx] = [l.pkg, l.pkgVersion, l.file, l.kind, null, 1, l.startChar, l.endChar];
fs.writeFileSync(`${base}/raw.labels.json`, JSON.stringify(lab));
console.error('units', rows.length, 'in', ((Date.now() - t0) / 1000).toFixed(1), 's');
