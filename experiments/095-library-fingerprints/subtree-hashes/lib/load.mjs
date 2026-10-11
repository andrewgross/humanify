import { readFileSync, existsSync } from 'node:fs';
export const LAB = '/work/exp095-library-fingerprints/lab/';
let _labels = null;
export const RWLAB = '/work/exp095-library-fingerprints/subtree-hashes/rwlab/';
// which corpus labels()/units() read: 'lab' (shared lab) or 'rw' (rwlab: CC-relevant packages, many versions)
let CORPUS = 'lab';
export function useCorpus(c) { CORPUS = c; _labels = null; }
export const ROOT = '/work/exp095-library-fingerprints/subtree-hashes/';
// build -> array indexed by idx
export function labels() {
  if (_labels) return _labels;
  _labels = new Map();
  for (const l of readFileSync((CORPUS === 'rw' ? RWLAB : LAB) + 'labels/all.jsonl', 'utf8').split('\n')) {
    if (!l) continue;
    const r = JSON.parse(l);
    let a = _labels.get(r.build); if (!a) _labels.set(r.build, a = []);
    a[r.idx] = r;
  }
  return _labels;
}
export const isLib = r => r && !r.pkg.startsWith('<');
export function units(mode, build, path = null) {
  const p = path || `${ROOT}data/units-${mode}${CORPUS === 'rw' ? '-rw' : ''}/builds/${build}.tsv`;
  if (!existsSync(p)) return null;
  const out = [];
  for (const l of readFileSync(p, 'utf8').split('\n')) {
    if (!l) continue;
    const f = l.split('\t');
    out.push({ row: +f[0], type: f[1], start: +f[2], end: +f[3], nodes: +f[4], nTok: +f[5], strChars: +f[6], mask: f[7], alpha: f[8], nolit: f[9], crosses: f[10] === '1' });
  }
  return out;
}
export const BUCKETS = [[1, 9], [10, 19], [20, 49], [50, 99], [100, 199], [200, 499], [500, 1e12]];
export const bucketOf = n => BUCKETS.findIndex(([a, b]) => n >= a && n <= b);
export const bucketName = i => { const [a, b] = BUCKETS[i]; return b > 1e9 ? `${a}+` : `${a}-${b}`; };
export const pct = (a, b) => b ? (100 * a / b).toFixed(1) + '%' : '-';

// every library unit of the given build configs: callback(u, row)
export function forEachLibUnit(mode, configs, cb, { modes = null, apps = null } = {}) {
  const L = labels();
  for (const [b, rows] of L) {
    const cfg = b.split('/').slice(0, 2).join('/');
    if (!configs.includes(cfg)) continue;
    const parts = b.split('/'); const app = parts[2], md = parts[3];
    if (modes && !modes.includes(md)) continue;
    if (apps && !apps(app)) continue;
    const us = units(mode, b); if (!us) continue;
    for (const u of us) {
      if (u.row < 0) continue;
      const r = rows[u.row];
      if (!r) continue;
      if (r.purity !== null && r.purity < 0.9) continue;
      cb(u, r, b);
    }
  }
}
// Claude Code real-world unit files: set = positives | negatives | negative-functions
import { readdirSync } from 'node:fs';
export function realworld(mode, v, set) {
  const dir = `${ROOT}data/units-${mode}/realworld/cc-${v}/${set}/`;
  const meta = readFileSync(`${LAB}realworld/cc-${v}/${set}.jsonl`, 'utf8').trim().split('\n').map(l => JSON.parse(l));
  const out = [];
  for (const m of meta) {
    const id = m.textPath.split('/').pop().replace('.min.js', '');
    const p = dir + id + '.tsv';
    if (!existsSync(p)) continue;
    out.push({ meta: m, id, units: units(mode, null, p) });
  }
  return out;
}
