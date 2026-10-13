// How unique is a unit hash, by unit size?  For every distinct unit hash in the corpus: how many distinct
// PACKAGES contain it, and does it ALSO occur in app code (lab <app> spans, Claude Code negatives)?
// node scripts/uniqueness.mjs > results/uniqueness.txt
import { forEachLibUnit, labels, units, realworld, isLib, BUCKETS, bucketOf, bucketName, pct } from '../lib/load.mjs';
const MODE = 'loose';
const CONFIGS = ['bun-1.3.14/min', 'bun-1.4.0/min', 'bun-1.4.3/min', 'esbuild-0.28.2/min'];
for (const variant of ['mask', 'nolit']) {
  const H = new Map(); // hash -> {pkgs:Set, nodes, str, app:bool, ex}
  const add = (u, owner, isApp, ex) => {
    if (u.type === 'root') return;
    let e = H.get(u[variant]); if (!e) H.set(u[variant], e = { pkgs: new Set(), nodes: u.nodes, str: u.strChars, app: false, ex: null });
    if (isApp) e.app = true; else e.pkgs.add(owner);
    if (!e.ex) e.ex = ex;
  };
  forEachLibUnit(MODE, CONFIGS, (u, r, b) => { if (isLib(r)) add(u, r.pkg, false, `${b}@${u.start}`); });
  // app code: lab <app> spans (all configs) + CC negatives (both versions)
  let appUnits = 0;
  forEachLibUnit(MODE, CONFIGS, (u, r, b) => { if (r.pkg === '<app>') { add(u, null, true, `${b}@${u.start}`); appUnits++; } });
  for (const v of ['2.1.198', '2.1.216']) for (const set of ['negatives', 'negative-functions'])
    for (const f of realworld(MODE, v, set)) for (const u of f.units) { add(u, null, true, `cc-${v}/${set}/${f.id}@${u.start}`); appUnits++; }
  const byB = BUCKETS.map(() => ({ n: 0, multi: 0, multi3: 0, appAndLib: 0, appOnly: 0 }));
  const byStr = {};
  const examples = BUCKETS.map(() => []);
  for (const [h, e] of H) {
    const b = byB[bucketOf(e.nodes)];
    if (e.pkgs.size === 0) { b.appOnly++; continue; }
    b.n++;
    if (e.pkgs.size >= 2) { b.multi++; if (examples[bucketOf(e.nodes)].length < 6 && Math.random() < 0.05) examples[bucketOf(e.nodes)].push(`${[...e.pkgs].slice(0, 5).join(', ')}${e.pkgs.size > 5 ? ` (+${e.pkgs.size - 5})` : ''}  ex ${e.ex}`); }
    if (e.pkgs.size >= 3) b.multi3++;
    if (e.app) b.appAndLib++;
    const sb = e.str === 0 ? '0' : e.str < 10 ? '1-9' : e.str < 30 ? '10-29' : '30+';
    const k = bucketName(bucketOf(e.nodes)) + ' str ' + sb;
    const s = (byStr[k] ??= { n: 0, multi: 0, app: 0 }); s.n++; if (e.pkgs.size >= 2) s.multi++; if (e.app) s.app++;
  }
  console.log(`\n=== variant ${variant}: ${H.size} distinct unit hashes (app units scanned: ${appUnits})`);
  console.log('size(nodes)   lib-hashes   in>=2 pkgs   in>=3 pkgs   ALSO in app code   app-only hashes');
  byB.forEach((b, i) => console.log(`${bucketName(i).padEnd(12)} ${String(b.n).padStart(10)}   ${pct(b.multi, b.n).padStart(9)}   ${pct(b.multi3, b.n).padStart(9)}   ${(pct(b.appAndLib, b.n) + ' (' + b.appAndLib + ')').padStart(14)}   ${b.appOnly}`));
  console.log('\nby size x string-literal chars:  [hashes, in>=2 pkgs, also-in-app]');
  for (const k of Object.keys(byStr).sort()) { const s = byStr[k]; console.log(`  ${k.padEnd(22)} ${String(s.n).padStart(8)}  ${pct(s.multi, s.n).padStart(7)}  ${pct(s.app, s.n).padStart(7)} (${s.app})`); }
  if (variant === 'mask') { console.log('\nexamples of hashes shared by >=2 packages:'); examples.forEach((ex, i) => ex.forEach(x => console.log(`  [${bucketName(i)}] ${x}`))); }
}
