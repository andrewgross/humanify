// Ground truth for lazy ESM library code in CC's src/: MSAL modules carry a /*! @azure/msal-xxx vN */ banner
// inside their init wrapper. How much of them (bytes) does module classification / the statement stream find?
// node scripts/cc-msal.mjs <v>
import { readFileSync } from 'node:fs';
import { useCorpus, units, LAB, ROOT, pct } from '../lib/load.mjs';
import { buildIndex, mergeIndex, classify } from '../lib/index.mjs';
import { labelStream, statementsOf } from '../lib/stream.mjs';
const V = process.argv[2] || '2.1.216';
useCorpus('lab'); const a = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: 20 });
useCorpus('rw'); const b = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: 20 });
const idx = mergeIndex(a, b);
const code = readFileSync(`/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-${V}/binary-decompiled/src/entrypoints/index.js`, 'utf8');
const layout = JSON.parse(readFileSync(`${LAB}realworld/layout-${V}.json`, 'utf8')).modules;
const us = units('loose', null, `${ROOT}data/units-loose/realworld/bundle-${V}.tsv`);
const truth = new Map(); layout.forEach((m, i) => { const x = code.slice(m.startChar, m.endChar).match(/\/\*! @azure\/(msal-[a-z]+) v([\d.]+)/); if (x) truth.set(i, '@azure/' + x[1]); });
const byRow = new Map(); for (const u of us) { if (!truth.has(u.row)) continue; let x = byRow.get(u.row); if (!x) byRow.set(u.row, x = []); x.push(u); }
let tb = 0, nb = 0, nbAny = 0, tiny = 0, tinyB = 0;
for (const [i, p] of truth) { const m = layout[i]; const bytes = m.end - m.start; tb += bytes; const c = classify(idx, byRow.get(i) || [], { k: 1 }); if (c.pkg) { nbAny += bytes; if (c.pkg.startsWith('@azure/msal')) nb += bytes; } if (!(byRow.get(i) || []).some(u => u.nodes >= 20 && u.type !== 'root')) { tiny++; tinyB += bytes; } }
const st = statementsOf(us); const lab = labelStream(idx, st, { gap: 3 });
let sb = 0, sMsal = 0, sOther = 0;
st.forEach((s, j) => { if (!truth.has(s.row)) return; const bytes = s.end - s.start; sb += bytes; if (lab[j].pkg) { if (lab[j].pkg.startsWith('@azure/msal')) sMsal += bytes; else sOther += bytes; } });
console.log(`CC ${V}: ${truth.size} MSAL banner modules, ${(tb / 1024).toFixed(0)} KB. Module-level (k=1): named msal ${pct(nb, tb)} of bytes (any package ${pct(nbAny, tb)}). Modules with no unit >= 20 nodes: ${tiny} (${(tinyB / 1024).toFixed(0)} KB).`);
console.log(`Statement stream (gap 3): ${(sb / 1024).toFixed(0)} KB of statements in MSAL modules; labelled msal ${pct(sMsal, sb)}, labelled another package ${pct(sOther, sb)}`);
