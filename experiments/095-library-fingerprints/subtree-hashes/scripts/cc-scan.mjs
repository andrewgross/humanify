// Reality check on Claude Code: run the unit-hash index (lab catalog, Bun 1.4.0, all versions + the rwlab
// CC-relevant packages at many versions) over the raw CC bundle.
// node scripts/cc-scan.mjs <ccVersion> [minNodes] > results/cc-scan-<v>.txt
import { readFileSync, writeFileSync } from 'node:fs';
import { useCorpus, units, LAB, ROOT, pct } from '../lib/load.mjs';
import { buildIndex, mergeIndex, classify } from '../lib/index.mjs';
import { labelStream, statementsOf } from '../lib/stream.mjs';
const V = process.argv[2] || '2.1.216', MIN = +(process.argv[3] || 20);
const BUNDLE = `/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-${V}/binary-decompiled/src/entrypoints/index.js`;
useCorpus('lab'); const a = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: MIN });
useCorpus('rw'); const b = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: MIN });
const idx = mergeIndex(a, b);
const code = readFileSync(BUNDLE, 'utf8');
const layout = JSON.parse(readFileSync(`${LAB}realworld/layout-${V}.json`, 'utf8')).modules;
const us = units('loose', null, `${ROOT}data/units-loose/realworld/bundle-${V}.tsv`);
const byRow = new Map(); for (const u of us) { if (u.row < 0) continue; let x = byRow.get(u.row); if (!x) byRow.set(u.row, x = []); x.push(u); }
const pos = readFileSync(`${LAB}realworld/cc-${V}/positives.jsonl`, 'utf8').trim().split('\n').map(l => JSON.parse(l));
const neg = readFileSync(`${LAB}realworld/cc-${V}/negatives.jsonl`, 'utf8').trim().split('\n').map(l => JSON.parse(l));
const rowByByteStart = new Map(layout.map((m, i) => [m.start, i]));
const posRow = new Map(pos.map(p => [rowByByteStart.get(p.start), p]));
const negRow = new Map(neg.map(n => [rowByByteStart.get(n.start), n]));
const PRODUCT = /tengu_|CLAUDE\.md|\.claude\/|claude\.ai\/code/;
const text = m => code.slice(m.startChar, m.endChar);
const msalTruth = new Map();
layout.forEach((m, i) => { const t = text(m); const x = t.match(/\/\*! @azure\/(msal-[a-z]+) v([\d.]+)/); if (x) msalTruth.set(i, '@azure/' + x[1]); });
const out = [`# Claude Code ${V}: unit size >= ${MIN}; index = lab (Bun 1.4.0, ${a.stats.hashes} hashes) + rwlab (${b.stats.hashes} hashes) -> ${idx.stats.hashes} hashes, ${idx.stats.ambiguous} ambiguous`,
  `layout: ${layout.length} wrapped modules (${layout.filter(m => m.kind === 'cjs').length} CJS factories, ${layout.filter(m => m.kind === 'lazy-esm').length} lazy-ESM groups); MSAL banner modules: ${msalTruth.size}`];
for (const k of [1, 2, 3]) {
  const per = {}, kinds = {}, sus = []; let posN = 0, posR = 0, negN = 0, msN = 0, msR = 0;
  const pr = [];
  layout.forEach((m, i) => {
    const c = classify(idx, byRow.get(i) || [], { k, margin: 2 });
    const bytes = m.end - m.start;
    const kd = (kinds[m.kind] ??= { n: 0, bytes: 0, named: 0, namedBytes: 0 }); kd.n++; kd.bytes += bytes;
    if (posRow.has(i)) { if (c.pkg) { posN++; if (c.pkg === posRow.get(i).pkg) posR++; } }
    if (negRow.has(i) && c.pkg) negN++;
    if (msalTruth.has(i)) { if (c.pkg) { msN++; if (c.pkg === msalTruth.get(i)) msR++; else pr.push(`msal module ${i} -> ${c.pkg}`); } }
    if (!c.pkg) return;
    kd.named++; kd.namedBytes += bytes;
    const p = (per[c.pkg] ??= { n: 0, bytes: 0, lazy: 0 }); p.n++; p.bytes += bytes; if (m.kind === 'lazy-esm') p.lazy++;
    const t = text(m); const pm = t.match(PRODUCT);
    if (pm) sus.push(`${m.kind} #${i} ${m.id} ${bytes}B -> ${c.pkg} (${c.votes.toFixed(1)} votes, ${c.matched}/${c.considered} units matched) product string '${pm[0]}'`);
  });
  out.push(`\n## k = ${k}`);
  for (const [kd, x] of Object.entries(kinds)) out.push(`${kd}: named ${x.named}/${x.n} modules, ${(x.namedBytes / 1e6).toFixed(2)}/${(x.bytes / 1e6).toFixed(2)} MB`);
  out.push(`positives (vendor identity records): named ${posN}/${pos.length}, right ${posR}`);
  out.push(`negatives (app-code groups): named ${negN}/${neg.length}`);
  out.push(`MSAL banner modules (ground truth for lazy ESM in src/): named ${msN}/${msalTruth.size}, right ${msR}${pr.length ? '  wrong: ' + pr.slice(0, 5).join('; ') : ''}`);
  out.push(`modules containing a Claude Code product string that were named as a library: ${sus.length}`);
  for (const s of sus.slice(0, 15)) out.push('  ' + s);
  out.push('named packages: ' + Object.entries(per).sort((x, y) => y[1].bytes - x[1].bytes).map(([p, x]) => `${p} ${x.n} (${(x.bytes / 1024).toFixed(0)} KB, ${x.lazy} lazy)`).join('; '));
}
// streaming over every module-level statement
for (const gap of [0, 3]) {
  const st = statementsOf(us);
  const lab = labelStream(idx, st, { gap });
  const per = {}; let claimed = 0, total = 0, negClaimed = 0, negBytes = 0, lazyClaimed = 0;
  const negSpans = neg.map(n => [layout[rowByByteStart.get(n.start)].startChar, layout[rowByByteStart.get(n.start)].endChar]);
  st.forEach((s, i) => {
    const bytes = s.end - s.start; total += bytes;
    const inNeg = negSpans.some(([x, y]) => s.start >= x && s.start < y);
    if (inNeg) negBytes += bytes;
    if (!lab[i].pkg) return;
    claimed += bytes; per[lab[i].pkg] = (per[lab[i].pkg] || 0) + bytes;
    if (s.row >= 0 && layout[s.row].kind === 'lazy-esm') lazyClaimed += bytes;
    if (inNeg) negClaimed += bytes;
  });
  out.push(`\n## streaming (statement level, gap ${gap}): ${(claimed / 1e6).toFixed(2)} of ${(total / 1e6).toFixed(2)} MB of module-level statements labelled; inside lazy-ESM groups ${(lazyClaimed / 1e6).toFixed(2)} MB; inside the 100 app-code negative groups ${negClaimed} of ${negBytes} bytes`);
  out.push('  ' + Object.entries(per).sort((x, y) => y[1] - x[1]).slice(0, 30).map(([p, x]) => `${p} ${(x / 1024).toFixed(0)}KB`).join('; '));
}
writeFileSync(`${ROOT}results/cc-scan-${V}-min${MIN}.txt`, out.join('\n') + '\n');
console.log(out.join('\n'));
