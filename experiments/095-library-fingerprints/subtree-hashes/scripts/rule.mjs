// Decision rule comparison: "k units" vs "k units OR one big unit". node scripts/rule.mjs > results/rule.txt
import { readFileSync } from 'node:fs';
import { useCorpus, labels, units, realworld, isLib, LAB, pct } from '../lib/load.mjs';
import { buildIndex, classify } from '../lib/index.mjs';
const L = labels();
function spans(cfg) { const out = []; for (const [b, rows] of L) { if (!b.startsWith(cfg + '/')) continue; const us = units('loose', b); if (!us) continue; const by = new Map(); for (const u of us) { if (u.row < 0) continue; let a = by.get(u.row); if (!a) by.set(u.row, a = []); a.push(u); } for (const [r, l] of by) if (isLib(rows[r]) && (rows[r].purity === null || rows[r].purity >= 0.9)) out.push({ r: rows[r], units: l }); } return out; }
const idx = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: 20 });
const pos = []; for (const v of ['2.1.198', '2.1.216']) for (const f of realworld('loose', v, 'positives')) pos.push(f);
const neg = []; for (const v of ['2.1.198', '2.1.216']) for (const s of ['negatives', 'negative-functions']) for (const f of realworld('loose', v, s)) neg.push(f);
for (const cfg of ['bun-1.4.3/min', 'esbuild-0.28.2/min']) {
  const q = spans(cfg);
  console.log(`\n# index Bun 1.4.0 (>=20 nodes) -> ${cfg}: ${q.length} library spans`);
  for (const [name, o] of [['k>=1', { k: 1 }], ['k>=2', { k: 2 }], ['k>=2 or one unit>=50', { k: 2, big: 50 }], ['k>=2 or one unit>=100', { k: 2, big: 100 }], ['k>=3 or one unit>=100', { k: 3, big: 100 }]]) {
    let n = 0, r = 0, by = 0, tb = 0; for (const x of q) { tb += x.r.bytes; const c = classify(idx, x.units, o); if (c.pkg) { n++; by += x.r.bytes; if (c.pkg === x.r.pkg) r++; } }
    let pn = 0, pr = 0; for (const f of pos) { const c = classify(idx, f.units, o); if (c.pkg) { pn++; if (c.pkg === f.meta.pkg) pr++; } }
    let fp = 0; for (const f of neg) if (classify(idx, f.units, o).pkg) fp++;
    console.log(`${name.padEnd(24)} named ${pct(n, q.length)} of spans, ${pct(by, tb)} of bytes, precision ${pct(r, n)} (${n - r} wrong) | CC positives ${pn}/${pos.length} (${pr} right) | CC app FPs ${fp}/${neg.length}`);
  }
}
