// node extract.mjs <mode:loose|strict> <outRoot> <buildDir>...   (buildDir relative to lab/)
// Writes <outRoot>/<buildDir>.tsv : one line per unit
//   row  type  startChar  endChar  nodes  nTok  strChars  h_mask  h_alpha  h_nolit  crosses
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { dirname } from 'node:path';
import { parse, serialise, unitHashes } from './canon.mjs';
const LAB = process.env.LABROOT || '/work/exp095-library-fingerprints/lab/';
const [mode, outRoot, ...dirs] = process.argv.slice(2);
for (const d of dirs) {
  const out = `${outRoot}/${d}.tsv`;
  if (existsSync(out)) continue;
  try {
    const code = readFileSync(LAB + d + '/out.js', 'utf8');
    const labels = readFileSync(LAB + d + '/labels.jsonl', 'utf8').trim().split('\n').map(l => JSON.parse(l));
    const starts = labels.map(r => r.startChar), ends = labels.map(r => r.endChar);
    const rowOf = pos => { // binary search: last label with start <= pos
      let lo = 0, hi = starts.length - 1, ans = -1;
      while (lo <= hi) { const m = (lo + hi) >> 1; if (starts[m] <= pos) { ans = m; lo = m + 1; } else hi = m - 1; }
      return ans >= 0 && pos < ends[ans] ? ans : -1;
    };
    const ast = parse(code);
    const { tok, units } = serialise(ast, { loose: mode === 'loose', code });
    const lines = [];
    for (const u of units) {
      const h = unitHashes(tok, u.a, u.b);
      const r = rowOf(u.start);
      const crosses = r >= 0 && u.end > ends[r] ? 1 : 0;
      lines.push([r >= 0 ? labels[r].idx : -1, u.type, u.start, u.end, u.nodes, h.nTok, h.strChars, h.mask, h.alpha, h.nolit, crosses].join('\t'));
    }
    mkdirSync(dirname(out), { recursive: true });
    writeFileSync(out, lines.join('\n') + '\n');
  } catch (e) { console.error('FAIL', d, e.message); }
}
