// node scripts/sample-diffs.mjs <mode> <cfgA> <cfgB> <n> [minNodes] [maxNodes] : show where unmatched units diverge
import { labels, units, isLib } from '../lib/load.mjs';
import { readFileSync } from 'node:fs';
import { parse, serialise } from './canon.mjs';
const [mode, A, B, N, mn = 30, mx = 150] = process.argv.slice(2);
const L = labels();
const builds = [...L.keys()].filter(b => b.startsWith(A + '/') && !b.includes('combo'));
let seed = 7; const rnd = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
const cache = new Map();
function toks(build, start) {
  let c = cache.get(build);
  if (!c) { const code = readFileSync('/work/exp095-library-fingerprints/lab/builds/' + build + '/out.js', 'utf8'); c = { code, ...serialise(parse(code), { loose: mode === 'loose', code }) }; cache.clear(); cache.set(build, c); }
  const u = c.units.find(u => u.start === start);
  return { t: c.tok.slice(u.a, u.b).map(t => t[0] === '$' ? 'I' : t), text: c.code.slice(u.start, u.end) };
}
let shown = 0, tries = 0;
while (shown < +N && tries++ < 5000) {
  const b = builds[Math.floor(rnd() * builds.length)];
  const bb = b.replace(A, B);
  const ua = units(mode, b), ub = units(mode, bb); if (!ua || !ub) continue;
  const ra = L.get(b), rb = L.get(bb);
  const hb = new Set(ub.map(u => u.mask));
  const cands = ua.filter(u => u.row >= 0 && isLib(ra[u.row]) && u.type === 'fn' && u.nodes >= mn && u.nodes <= mx && !hb.has(u.mask));
  if (!cands.length) continue;
  const u = cands[Math.floor(rnd() * cands.length)];
  const r = ra[u.row];
  const peers = ub.filter(v => v.row >= 0 && rb[v.row] && rb[v.row].file === r.file && rb[v.row].pkg === r.pkg && v.type === 'fn' && v.strChars === u.strChars && Math.abs(v.nodes - u.nodes) <= u.nodes * 0.25);
  if (!peers.length) continue;
  peers.sort((x, y) => Math.abs(x.nodes - u.nodes) - Math.abs(y.nodes - u.nodes));
  const X = toks(b, u.start), Y = toks(bb, peers[0].start);
  let i = 0; while (i < X.t.length && X.t[i] === Y.t[i]) i++;
  shown++;
  console.log(`\n#${shown} ${r.pkg}@${r.pkgVersion} ${r.file} (${u.nodes} nodes)`);
  console.log('A:', X.text.slice(0, 400)); console.log('B:', Y.text.slice(0, 400));
  console.log(' A@:', X.t.slice(Math.max(0, i - 12), i + 10).join(' '));
  console.log(' B@:', Y.t.slice(Math.max(0, i - 12), i + 10).join(' '));
}
