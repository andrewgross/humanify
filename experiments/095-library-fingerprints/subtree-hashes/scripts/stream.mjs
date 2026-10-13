// Can unit hits locate library code in an UNBOUNDED stream of statements (eager ESM, lazy ESM with hoisted
// functions, app code interleaved)? Statement-level labelling + smoothing, scored in bytes against the lab labels.
// node scripts/stream.mjs > results/stream.txt
import { labels, units, isLib, pct } from '../lib/load.mjs';
import { buildIndex } from '../lib/index.mjs';
import { labelStream, statementsOf } from '../lib/stream.mjs';
const L = labels();
const idx = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: 20 });
const idx10 = buildIndex({ configs: ['bun-1.4.0/min'], minNodes: 10 });
const out = [];
for (const [name, filter] of [['combo apps (8-9 libraries + app files, all modes mixed)', b => b.includes('/combo-')], ['single-package `imp` builds (eager, scope-hoisted ESM)', b => b.endsWith('/imp') && !b.includes('combo')], ['single-package `some` builds (tree-shaken eager)', b => b.endsWith('/some')]]) {
  for (const [iname, I] of [['>=20 nodes', idx], ['>=10 nodes', idx10]]) for (const gap of [0, 3, 10]) {
    const T = { libBytes: 0, appBytes: 0, libHitRight: 0, libHitWrong: 0, libFillRight: 0, libFillWrong: 0, appClaimed: 0, otherBytes: 0, otherClaimed: 0, byKind: {} };
    const wrongEx = [], appEx = [];
    for (const [b, rows] of L) {
      if (!b.startsWith('bun-1.4.3/min/') || !filter(b)) continue;
      const us = units('loose', b); if (!us) continue;
      const st = statementsOf(us);
      const lab = labelStream(I, st, { gap });
      st.forEach((s, i) => {
        const r = rows[s.row]; if (!r) return;
        const bytes = s.end - s.start, l = lab[i];
        if (isLib(r)) {
          T.libBytes += bytes;
          const k = (T.byKind[r.kind] ??= [0, 0]); k[0] += bytes;
          if (l.pkg === r.pkg) { if (l.hit) T.libHitRight += bytes; else T.libFillRight += bytes; k[1] += bytes; }
          else if (l.pkg) { if (l.hit) T.libHitWrong += bytes; else T.libFillWrong += bytes; if (wrongEx.length < 5) wrongEx.push(`${r.pkg}/${r.file} -> ${l.pkg}${l.filled ? ' (filled)' : ''} [${b}@${s.start}]`); }
        } else if (r.pkg === '<app>') { T.appBytes += bytes; if (l.pkg) { T.appClaimed += bytes; if (appEx.length < 5) appEx.push(`${b}@${s.start} -> ${l.pkg}${l.filled ? ' (filled)' : ''}`); } }
        else { T.otherBytes += bytes; if (l.pkg) T.otherClaimed += bytes; }
      });
    }
    const named = T.libHitRight + T.libHitWrong + T.libFillRight + T.libFillWrong;
    out.push(`${name} | units ${iname} | gap ${gap}: library bytes ${T.libBytes}, labelled right ${pct(T.libHitRight + T.libFillRight, T.libBytes)} (by hit ${pct(T.libHitRight, T.libBytes)}, by fill ${pct(T.libFillRight, T.libBytes)}), labelled WRONG pkg ${pct(T.libHitWrong + T.libFillWrong, named)} of labelled; app bytes ${T.appBytes}, claimed ${T.appClaimed}; glue/runtime claimed ${pct(T.otherClaimed, T.otherBytes)}` +
      `\n    by kind: ` + Object.entries(T.byKind).map(([k, [a, c]]) => `${k} ${pct(c, a)} of ${(a / 1e6).toFixed(1)}MB`).join('; ') +
      (wrongEx.length ? `\n    wrong: ${wrongEx.join(' ; ')}` : '') + (appEx.length ? `\n    app claimed: ${appEx.join(' ; ')}` : ''));
  }
}
console.log(out.join('\n'));
