// Precision / recall of "k matching unit hashes => library X" under several index/query scenarios.
// node scripts/identify.mjs [scenario...]  -> results/identify-<scenario>.txt
import { readFileSync, writeFileSync } from 'node:fs';
import { labels, units, realworld, isLib, LAB, pct } from '../lib/load.mjs';
import { buildIndex, classify } from '../lib/index.mjs';
const L = labels();
const versions = JSON.parse(readFileSync(LAB + 'versions.json', 'utf8'));
const heldOut = new Set(versions.map(p => `${p.name}@${p.versions.patch || p.versions.minor}`));
const isHeld = r => heldOut.has(`${r.pkg}@${r.pkgVersion}`);
const catalog = new Map(versions.map(p => [p.name, p.versions]));
const notLatest = r => catalog.has(r.pkg) && catalog.get(r.pkg).latest !== r.pkgVersion;
const isKind = kind => r => catalog.has(r.pkg) && catalog.get(r.pkg)[kind] === r.pkgVersion;

const SCEN = {
  'same-versions': { idx: { configs: ['bun-1.4.0/min'] }, q: { configs: ['bun-1.4.3/min'] }, desc: 'index Bun 1.4.0 (all versions) -> query Bun 1.4.3 builds' },
  'unseen-version': { idx: { configs: ['bun-1.4.0/min'], exclude: isHeld }, q: { configs: ['bun-1.4.3/min'], only: isHeld }, desc: 'index Bun 1.4.0 WITHOUT one version per package (patch, else minor) -> query Bun 1.4.3 spans of exactly those held-out versions' },
  'latest-only-to-minor': { idx: { configs: ['bun-1.4.0/min'], exclude: notLatest }, q: { configs: ['bun-1.4.3/min'], only: isKind('minor') }, desc: 'index Bun 1.4.0 with ONLY the latest version of each catalog package -> query Bun 1.4.3 spans of the previous-minor version' },
  'latest-only-to-major': { idx: { configs: ['bun-1.4.0/min'], exclude: notLatest }, q: { configs: ['bun-1.4.3/min'], only: isKind('major') }, desc: 'index Bun 1.4.0 with ONLY the latest version of each catalog package -> query Bun 1.4.3 spans of the previous-major version' },
  'cross-bundler': { idx: { configs: ['bun-1.4.0/min'] }, q: { configs: ['esbuild-0.28.2/min'] }, desc: 'index Bun 1.4.0 -> query esbuild builds' },
  'multi-bundler-unseen': { idx: { configs: ['bun-1.4.0/min', 'esbuild-0.28.2/min'], exclude: isHeld }, q: { configs: ['bun-1.4.3/min'], only: isHeld }, desc: 'index Bun 1.4.0 + esbuild WITHOUT held-out versions -> query Bun 1.4.3 held-out versions' },
  'rollup-terser': { idx: { configs: ['bun-1.4.0/min', 'esbuild-0.28.2/min'] }, q: { configs: ['rollup-4.64.5/min'] }, desc: 'index Bun 1.4.0 + esbuild -> query rollup+terser builds (purity>=0.9)' },
  'tree-shaken': { idx: { configs: ['bun-1.4.0/min'], modes: ['req', 'imp', 'dyn'] }, q: { configs: ['bun-1.4.3/min'], modes: ['some'] }, desc: 'index Bun 1.4.0 whole-package builds (req/imp/dyn) -> query Bun 1.4.3 tree-shaken `some` builds' },
};

function querySpans({ configs, modes = null, only = null }) {
  const out = [];
  for (const [b, rows] of L) {
    const parts = b.split('/'); const cfg = parts.slice(0, 2).join('/');
    if (!configs.includes(cfg) || (modes && !modes.includes(parts[3]))) continue;
    const us = units('loose', b); if (!us) continue;
    const by = new Map();
    for (const u of us) { if (u.row < 0) continue; let a = by.get(u.row); if (!a) by.set(u.row, a = []); a.push(u); }
    for (const [row, list] of by) {
      const r = rows[row]; if (!r) continue;
      if (r.purity !== null && r.purity < 0.9) continue;
      if (isLib(r) && only && !only(r)) continue;
      if (!isLib(r) && r.pkg !== '<app>') continue;
      out.push({ r, units: list, build: b });
    }
  }
  return out;
}

const KS = [1, 2, 3, 5, 8];
const MINS = [10, 20, 30, 50];
const which = process.argv.slice(2).length ? process.argv.slice(2) : Object.keys(SCEN);
// negatives that do not depend on the scenario: Claude Code app code
const ccNeg = [];
for (const v of ['2.1.198', '2.1.216']) for (const set of ['negatives', 'negative-functions']) for (const f of realworld('loose', v, set)) ccNeg.push({ set: `cc-${v}/${set}`, ...f });
const ccPos = [];
for (const v of ['2.1.198', '2.1.216']) for (const f of realworld('loose', v, 'positives')) ccPos.push({ v, ...f });

for (const name of which) {
  const S = SCEN[name];
  const qs = querySpans(S.q);
  const lib = qs.filter(q => isLib(q.r)), app = qs.filter(q => q.r.pkg === '<app>');
  const out = [`# scenario ${name}: ${S.desc}`, `query: ${lib.length} library module spans (${(lib.reduce((a, q) => a + q.r.bytes, 0) / 1e6).toFixed(1)} MB), ${app.length} lab app spans, ${ccNeg.length} Claude Code app-code negatives, ${ccPos.length} Claude Code positives`];
  for (const minNodes of MINS) {
    const idx = buildIndex({ ...S.idx, minNodes });
    out.push(`\n## unit size >= ${minNodes} nodes: index ${idx.stats.hashes} unit hashes (${idx.stats.ambiguous} in >=2 packages, ignored), ${idx.stats.roots} whole-module hashes`);
    // whole-module exact baseline
    let rootQ = 0, rootHit = 0, rootRight = 0;
    const res = lib.map(q => { const c = classify(idx, q.units, { k: 1, margin: 2 }); return { q, c }; });
    for (const { q, c } of res) { if (q.units.some(u => u.type === 'root')) { rootQ++; if (c.rootHit) { rootHit++; if ([...c.rootHit].some(f => f === `${q.r.pkg}/${q.r.file}`)) rootRight++; } } }
    out.push(`whole-module exact hash: ${rootHit}/${rootQ} wrapped modules hit (${pct(rootHit, rootQ)}), of which right file ${pct(rootRight, rootHit)}`);
    out.push(`k  | lib spans named (recall) | bytes named | precision pkg | precision file | wrong examples | lab-app FP | CC-app FP | CC positives named (right)`);
    for (const k of KS) {
      let named = 0, right = 0, rightFile = 0, bytes = 0, totalBytes = 0, wrong = [];
      const perKind = {};
      for (const q of lib) {
        const c = classify(idx, q.units, { k, margin: 2 });
        totalBytes += q.r.bytes;
        const pk = (perKind[q.r.kind] ??= [0, 0, 0]); pk[0]++;
        if (!c.pkg) continue;
        named++; bytes += q.r.bytes; pk[1]++;
        if (c.pkg === q.r.pkg) { right++; pk[2]++; if (c.file === `${q.r.pkg}/${q.r.file}`) rightFile++; }
        else if (wrong.length < 400) wrong.push(`${q.r.pkg}@${q.r.pkgVersion}/${q.r.file} -> ${c.pkg} (${c.votes.toFixed(1)} vs ${c.second.toFixed(1)}) [${q.build}]`);
      }
      let labFP = 0; for (const q of app) if (classify(idx, q.units, { k, margin: 2 }).pkg) labFP++;
      let ccFP = 0; const ccFPex = [];
      for (const q of ccNeg) { const c = classify(idx, q.units, { k, margin: 2 }); if (c.pkg) { ccFP++; ccFPex.push(`${q.set}/${q.id} -> ${c.pkg} (${c.votes})`); } }
      let posNamed = 0, posRight = 0;
      for (const q of ccPos) { const c = classify(idx, q.units, { k, margin: 2 }); if (c.pkg) { posNamed++; if (c.pkg === q.meta.pkg) posRight++; } }
      out.push(`${String(k).padEnd(2)} | ${named}/${lib.length} = ${pct(named, lib.length)} | ${pct(bytes, totalBytes)} | ${pct(right, named)} (${named - right} wrong) | ${pct(rightFile, named)} | ${wrong.slice(0, 2).join(' ; ')} | ${labFP}/${app.length} | ${ccFP}/${ccNeg.length} | ${posNamed}/${ccPos.length} (${posRight})`);
      out.push(`     by kind: ` + Object.entries(perKind).map(([kd, [n, nm, rt]]) => `${kd} named ${pct(nm, n)} prec ${pct(rt, nm)}`).join('; '));
      if (ccFPex.length) out.push(`     CC app FPs: ${ccFPex.slice(0, 5).join(' ; ')}`);
      if (k === 2 && minNodes === 20) writeFileSync(`results/identify-${name}-wrong.txt`, wrong.join('\n') + '\n');
    }
  }
  writeFileSync(`results/identify-${name}.txt`, out.join('\n') + '\n');
  console.log(out.join('\n'));
}
