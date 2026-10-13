// Which library VERSION does Claude Code ship? For each rwlab package (many versions built with Bun 1.4.0), take
// the CC units that the merged index attributes to that package (statement stream, gap 0) and measure, per
// version, the share of those units the version contains. Also: what a latest-only index would have found.
// node scripts/cc-versions.mjs <ccVersion> > results/cc-versions-<v>.txt
import { readFileSync } from 'node:fs';
import { useCorpus, labels, units, forEachLibUnit, ROOT, isLib } from '../lib/load.mjs';
import { buildIndex, mergeIndex } from '../lib/index.mjs';
import { labelStream, statementsOf } from '../lib/stream.mjs';
const V = process.argv[2] || '2.1.216', MIN = 20;
useCorpus('lab'); const a = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: MIN });
useCorpus('rw'); const b = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: MIN });
const idx = mergeIndex(a, b);
// per package@version unit sets from rwlab (the package's own files only)
const sets = new Map(); // pkg -> Map(ver -> Set)
const rwPkgs = new Set(JSON.parse(readFileSync(ROOT + 'rwlab/versions.json', 'utf8')).map(p => p.name));
const appVer = new Map();
forEachLibUnit('loose', ['bun-1.4.0/min'], (u, r, build) => {
  if (u.type === 'root' || u.nodes < MIN || !rwPkgs.has(r.pkg)) return;
  const app = build.split('/')[2]; // only the app that IS this package (not the same package pulled in by another)
  if (!app.startsWith(r.pkg.replace('/', '__') + '@')) return;
  let m = sets.get(r.pkg); if (!m) sets.set(r.pkg, m = new Map());
  let s = m.get(r.pkgVersion); if (!s) m.set(r.pkgVersion, s = new Set());
  s.add(u.mask);
});
const vers = JSON.parse(readFileSync(ROOT + 'rwlab/versions.json', 'utf8'));
const us = units('loose', null, `${ROOT}data/units-loose/realworld/bundle-${V}.tsv`);
const st = statementsOf(us);
const lab = labelStream(idx, st, { gap: 0 });
const ccUnits = new Map(); // pkg -> Set(hash) of CC units (>=MIN) inside statements labelled pkg
st.forEach((s, i) => { const p = lab[i].pkg; if (!p || !sets.has(p)) return; let x = ccUnits.get(p); if (!x) ccUnits.set(p, x = new Set()); for (const u of s.units) if (u.nodes >= MIN && u.type !== 'root') x.add(u.mask); });
console.log(`# Claude Code ${V}: per package, share of CC's units (attributed to it) found in each built version`);
for (const p of vers) {
  const cc = ccUnits.get(p.name), m = sets.get(p.name); if (!cc || !m) { console.log(`${p.name}: no CC units attributed`); continue; }
  const rows = [...m].map(([v, s]) => { let h = 0; for (const x of cc) if (s.has(x)) h++; return [v, h]; }).sort((x, y) => y[1] - x[1]);
  const latest = p.versions[p.versions.length - 1];
  const union = new Set(); for (const s of m.values()) for (const x of s) union.add(x);
  let hu = 0; for (const x of cc) if (union.has(x)) hu++;
  const best = rows[0][1];
  const ties = rows.filter(r => r[1] === best).map(r => r[0]);
  // minimal set of versions to cover everything the union covers (greedy)
  const left = new Set([...cc].filter(x => union.has(x))); const pick = [];
  while (left.size && pick.length < 6) { let bv = null, bc = 0; for (const [v, s] of m) { let c = 0; for (const x of left) if (s.has(x)) c++; if (c > bc) { bc = c; bv = v; } } if (!bv) break; pick.push(`${bv}(+${bc})`); for (const x of [...left]) if (m.get(bv).has(x)) left.delete(x); }
  console.log(`${p.name}: ${cc.size} CC units attributed; ALL ${m.size} versions together contain ${hu}; best single version ${ties.slice(0, 4).join('/')}${ties.length > 4 ? ` (+${ties.length - 4} tied)` : ''} = ${best}; latest ${latest} = ${(m.get(latest) ? [...cc].filter(x => m.get(latest).has(x)).length : 'n/a')}; greedy cover: ${pick.join(' ')}`);
  console.log(`    by version (oldest->newest): ` + p.versions.filter(v => m.has(v)).map(v => `${v}:${rows.find(r => r[0] === v)[1]}`).join(' '));
}
