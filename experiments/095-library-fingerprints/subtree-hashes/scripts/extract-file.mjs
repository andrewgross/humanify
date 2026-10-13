// node extract-file.mjs <mode> <out.tsv> <file> [layout.json]
// Units of a standalone JS file. With a layout (lab/realworld/layout-<v>.json), row = index of the layout module
// whose [startChar,endChar) holds the unit start; else row = 0.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { parse, serialise, unitHashes } from './canon.mjs';
const [mode, out, file, layoutPath] = process.argv.slice(2);
const code = readFileSync(file, 'utf8');
let rowOf = () => 0, ends = null;
if (layoutPath) {
  const mods = JSON.parse(readFileSync(layoutPath, 'utf8')).modules;
  const starts = mods.map(m => m.startChar); ends = mods.map(m => m.endChar);
  rowOf = pos => { let lo = 0, hi = starts.length - 1, ans = -1; while (lo <= hi) { const m = (lo + hi) >> 1; if (starts[m] <= pos) { ans = m; lo = m + 1; } else hi = m - 1; } return ans >= 0 && pos < ends[ans] ? ans : -1; };
}
let ast;
try { ast = parse(code); } catch (e) { console.error('PARSEFAIL', file, e.message); process.exit(0); }
const { tok, units } = serialise(ast, { loose: mode === 'loose', code });
const lines = units.map(u => { const h = unitHashes(tok, u.a, u.b); const r = rowOf(u.start); return [r, u.type, u.start, u.end, u.nodes, h.nTok, h.strChars, h.mask, h.alpha, h.nolit, 0].join('\t'); });
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, lines.join('\n') + '\n');
