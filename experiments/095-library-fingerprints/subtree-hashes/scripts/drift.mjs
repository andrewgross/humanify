// How many unit hashes of a library's LATEST version survive in its patch / minor / major / old-CJS version?
// Same bundler (Bun 1.4.0, minified), same entry mode. node scripts/drift.mjs > results/drift.txt
import { readFileSync } from 'node:fs';
import { labels, units, LAB, pct } from '../lib/load.mjs';
const L = labels();
const versions = JSON.parse(readFileSync(LAB + 'versions.json', 'utf8'));
const CFG = 'bun-1.4.0/min';
const MIN = +(process.argv[2] || 20);
function pkgUnits(pkg, ver) {
  for (const mode of ['req', 'imp', 'dyn']) {
    const b = `${CFG}/${pkg.replace('/', '__')}@${ver}/${mode}`;
    const rows = L.get(b); if (!rows) continue;
    const us = units('loose', b); if (!us) continue;
    const set = new Set(), roots = new Map(); let bytes = 0;
    for (const r of rows) if (r && r.pkg === pkg) bytes += r.bytes;
    for (const u of us) {
      const r = rows[u.row]; if (!r || r.pkg !== pkg) continue;
      if (u.type === 'root') roots.set(r.file, u.mask);
      else if (u.nodes >= MIN) set.add(u.mask);
    }
    return { set, roots, mode, bytes };
  }
  return null;
}
const agg = {};
const lines = [`unit size >= ${MIN} nodes; Bun 1.4.0 minified. Columns per older version: share of LATEST's unit hashes found in it | share of latest's files whose whole-module hash is identical`];
for (const p of versions) {
  const lat = pkgUnits(p.name, p.versions.latest); if (!lat || !lat.set.size) continue;
  const cells = [];
  for (const kind of ['patch', 'minor', 'major', 'extra']) {
    const v = p.versions[kind]; if (!v) continue;
    const o = pkgUnits(p.name, v); if (!o) continue;
    let hit = 0; for (const h of lat.set) if (o.set.has(h)) hit++;
    let rf = 0, rs = 0; for (const [f, h] of lat.roots) { if (o.roots.has(f)) { rf++; if (o.roots.get(f) === h) rs++; } }
    const share = hit / lat.set.size;
    (agg[kind] ??= []).push(share);
    cells.push(`${kind} ${v}: ${pct(hit, lat.set.size)} units${lat.roots.size ? `, ${rs}/${lat.roots.size} modules identical` : ''}`);
  }
  lines.push(`${p.name}@${p.versions.latest} (${lat.set.size} units, ${(lat.bytes / 1024).toFixed(0)} KB, mode ${lat.mode}): ${cells.join(' | ')}`);
}
lines.push('\nSUMMARY (share of latest-version units also present in the older version):');
for (const [k, a] of Object.entries(agg)) {
  a.sort((x, y) => x - y);
  const q = f => (100 * a[Math.floor(f * (a.length - 1))]).toFixed(0) + '%';
  lines.push(`  ${k.padEnd(6)} n=${a.length}  min ${q(0)}  p25 ${q(0.25)}  median ${q(0.5)}  p75 ${q(0.75)}  max ${q(1)}`);
}
console.log(lines.join('\n'));
