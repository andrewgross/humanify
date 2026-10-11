// Unit-level precision on Claude Code app code: every index hit inside a lazy-ESM group that contains a CC
// product string. Each hit is printed with its text for manual judgement (library code embedded in the group,
// or a generic snippet the app happens to share?).  node scripts/cc-app-hits.mjs <v> <minNodes>
import { readFileSync } from 'node:fs';
import { useCorpus, units, LAB, ROOT } from '../lib/load.mjs';
import { buildIndex, mergeIndex } from '../lib/index.mjs';
const V = process.argv[2] || '2.1.216', MIN = +(process.argv[3] || 20);
useCorpus('lab'); const a = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: MIN });
useCorpus('rw'); const b = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: MIN });
const idx = mergeIndex(a, b);
const code = readFileSync(`/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-${V}/binary-decompiled/src/entrypoints/index.js`, 'utf8');
const layout = JSON.parse(readFileSync(`${LAB}realworld/layout-${V}.json`, 'utf8')).modules;
const us = units('loose', null, `${ROOT}data/units-loose/realworld/bundle-${V}.tsv`);
const PRODUCT = /tengu_|CLAUDE\.md|\.claude\/|claude\.ai\/code/;
const prodRows = new Set(); let prodBytes = 0;
layout.forEach((m, i) => { if (m.kind === 'lazy-esm' && PRODUCT.test(code.slice(m.startChar, m.endChar))) { prodRows.add(i); prodBytes += m.end - m.start; } });
let considered = 0, hits = 0; const ex = [];
const seen = new Set();
for (const u of us) {
  if (!prodRows.has(u.row) || u.type === 'root' || u.nodes < MIN) continue;
  considered++;
  const e = idx.H.get(u.mask); if (!e || e.pkgs.size > 1) continue;
  hits++;
  const key = u.mask; if (seen.has(key)) continue; seen.add(key);
  ex.push(`[${u.type} ${u.nodes} nodes] ${[...e.files][0]} :: ${JSON.stringify(code.slice(u.start, Math.min(u.end, u.start + 160)))}`);
}
console.log(`# CC ${V}: ${prodRows.size} lazy-ESM groups with a product string (${(prodBytes / 1e6).toFixed(2)} MB); units >= ${MIN} nodes in them: ${considered}; index hits: ${hits} (${seen.size} distinct)`);
console.log(ex.join('\n'));
