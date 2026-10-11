// How often does a unit hash computed in build config A reappear in the SAME module (app, mode, pkg, version, file)
// built by config B?  node scripts/stability.mjs > results/stability.txt
import { labels, units, isLib, BUCKETS, bucketOf, bucketName, pct } from '../lib/load.mjs';
import { writeFileSync } from 'node:fs';
const L = labels();
const builds = [...L.keys()];
const VARIANTS = ['mask', 'alpha', 'nolit'];

function moduleSets(mode, build) {
  const us = units(mode, build); if (!us) return null;
  const rows = L.get(build);
  const m = new Map();
  for (const u of us) {
    if (u.row < 0) continue;
    const r = rows[u.row];
    if (!isLib(r) || (r.purity !== null && r.purity < 0.9)) continue;
    const key = `${r.pkg}@${r.pkgVersion}/${r.file}`;
    let e = m.get(key); if (!e) m.set(key, e = { units: [], kind: r.kind });
    e.units.push(u);
  }
  return m;
}

// pairs: [label, configA, modeA-filter, configB, modeMap]
const PAIRS = [
  ['bun-1.4.0 vs bun-1.4.3', 'bun-1.4.0/min', 'bun-1.4.3/min'],
  ['bun-1.3.14 vs bun-1.4.0', 'bun-1.3.14/min', 'bun-1.4.0/min'],
  ['bun-1.4.0 vs esbuild', 'bun-1.4.0/min', 'esbuild-0.28.2/min'],
  ['bun-1.4.0 vs rollup+terser', 'bun-1.4.0/min', 'rollup-4.64.5/min'],
  ['bun-1.4.0 min vs unmin', 'bun-1.4.0/min', 'bun-1.4.0/unmin'],
  ['esbuild min vs unmin', 'esbuild-0.28.2/min', 'esbuild-0.28.2/unmin'],
  ['bun-1.4.0 req vs imp (same app)', 'bun-1.4.0/min', 'bun-1.4.0/min', 'req', 'imp'],
  ['bun-1.4.0 imp vs some (tree-shaken)', 'bun-1.4.0/min', 'bun-1.4.0/min', 'imp', 'some'],
  ['bun-1.4.0 req vs dyn', 'bun-1.4.0/min', 'bun-1.4.0/min', 'req', 'dyn'],
];
const out = [];
for (const mode of ['loose', 'strict']) {
  for (const [label, A, B, mA, mB] of PAIRS) {
    // stats[variant][type][bucket] = [found, total]
    const st = {}; for (const v of VARIANTS) { st[v] = {}; }
    const root = { same: 0, total: 0 }, rootByKind = {};
    let modules = 0;
    for (const b of builds) {
      if (!b.startsWith(A + '/')) continue;
      const parts = b.split('/'); const app = parts[parts.length - 2], md = parts[parts.length - 1];
      if (mA && md !== mA) continue;
      if (app.startsWith('combo') && mA) continue;
      const bb = B + '/' + app + '/' + (mB || md);
      if (!L.has(bb)) continue;
      const sa = moduleSets(mode, b), sb = moduleSets(mode, bb);
      if (!sa || !sb) continue;
      for (const [key, ea] of sa) {
        const eb = sb.get(key); if (!eb) continue;
        modules++;
        for (const v of VARIANTS) {
          const hb = new Set(eb.units.map(u => u[v]));
          const seen = new Set();
          for (const u of ea.units) {
            if (seen.has(u[v] + u.type)) continue; seen.add(u[v] + u.type);
            const t = u.type === 'stmt' || u.type === 'root' ? u.type : 'fn/class';
            const bk = bucketOf(u.nodes);
            const s = (st[v][t] ??= BUCKETS.map(() => [0, 0]));
            s[bk][1]++; if (hb.has(u[v])) s[bk][0]++;
            const a = (st[v]['ALL>=20'] ??= [[0, 0]]); if (u.nodes >= 20 && u.type !== 'root') { a[0][1]++; if (hb.has(u[v])) a[0][0]++; }
          }
        }
        const ra = ea.units.find(u => u.type === 'root'), rb = eb.units.find(u => u.type === 'root');
        if (ra && rb) { root.total++; if (ra.mask === rb.mask) root.same++; }
      }
    }
    out.push(`\n=== [${mode}] ${label}: ${modules} shared module instances; whole-module (root) mask hash equal: ${root.same}/${root.total} = ${pct(root.same, root.total)}`);
    for (const v of VARIANTS) {
      out.push(`  variant ${v}:  ALL non-root units >=20 nodes found: ${pct(...st[v]['ALL>=20'][0])} (${st[v]['ALL>=20'][0][1]})`);
      for (const t of ['stmt', 'fn/class']) {
        const s = st[v][t]; if (!s) continue;
        out.push(`    ${t.padEnd(9)} ` + s.map((x, i) => `${bucketName(i)}:${pct(x[0], x[1])}(${x[1]})`).join('  '));
      }
    }
  }
}
writeFileSync(new URL('../results/stability.txt', import.meta.url), out.join('\n') + '\n');
console.log(out.join('\n'));
