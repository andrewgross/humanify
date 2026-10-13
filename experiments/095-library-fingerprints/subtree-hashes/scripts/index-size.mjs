// How big is the index? Distinct unit hashes per package, growth as versions are added, bytes per entry.
// node scripts/index-size.mjs > results/index-size.txt
import { useCorpus, forEachLibUnit, labels, isLib } from '../lib/load.mjs';
const out = [];
for (const MIN of [20, 30, 50]) for (const corpus of ['lab', 'rw']) {
  useCorpus(corpus);
  // pkg -> ver -> Set(hash);  pkg -> ver -> bytes (distinct files)
  const S = new Map(), B = new Map(), seenFile = new Set();
  forEachLibUnit('loose', ['bun-1.4.0/min'], (u, r) => {
    if (!isLib(r)) return;
    const fk = `${r.pkg}@${r.pkgVersion}/${r.file}/${r.part}`;
    if (!seenFile.has(fk)) { seenFile.add(fk); let m = B.get(r.pkg); if (!m) B.set(r.pkg, m = new Map()); m.set(r.pkgVersion, (m.get(r.pkgVersion) || 0) + r.bytes); }
    if (u.type === 'root' || u.nodes < MIN) return;
    let m = S.get(r.pkg); if (!m) S.set(r.pkg, m = new Map());
    let s = m.get(r.pkgVersion); if (!s) m.set(r.pkgVersion, s = new Set());
    s.add(u.mask);
  });
  let pv = 0, sumPV = 0, all = new Set(), bytesPV = 0, union = 0;
  const growth = [];
  for (const [p, m] of S) {
    const u = new Set();
    for (const [v, s] of m) { pv++; sumPV += s.size; for (const h of s) { u.add(h); all.add(h); } bytesPV += (B.get(p)?.get(v) || 0); }
    union += u.size;
    if (corpus === 'rw') {
      const vs = [...m.keys()]; const one = Math.max(...vs.map(v => m.get(v).size));
      growth.push(`${p}: ${vs.length} versions; largest single version ${one} hashes; all versions ${u.size} (x${(u.size / one).toFixed(2)})`);
    }
  }
  out.push(`\n## corpus ${corpus}, unit size >= ${MIN} nodes (Bun 1.4.0 minified)`);
  out.push(`package versions: ${pv}; mean hashes per package-version: ${(sumPV / pv).toFixed(0)}; minified code per package-version: ${(bytesPV / pv / 1024).toFixed(0)} KB -> ${(sumPV / (bytesPV / 1024)).toFixed(2)} hashes per KB`);
  out.push(`distinct hashes: sum over package-versions ${sumPV}; deduplicated within each package ${union}; across everything ${all.size}`);
  out.push(`index bytes at 8-byte hash + 4-byte package/version-range id: ${(all.size * 12 / 1e6).toFixed(2)} MB (all packages and versions here)`);
  if (growth.length) out.push(...growth.map(g => '  ' + g));
}
console.log(out.join('\n'));
