// Tokenise CC 2.1.216 raw bundle once. Output data/bundletok.json: per token (s:/n: as in srcindex) -> [count in src/ part, count in CJS-factory part]
// and data/units-tok.jsonl: per population unit: tokens list (distinct, s:/n:)  -- only for units in data/sample.jsonl + their groups (all population units, actually)
import fs from 'node:fs';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const acorn = require('/work/exp095-library-fingerprints/lab/tools/node_modules/acorn');
const B = '/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js';
const src = fs.readFileSync(B, 'utf8');
const layout = JSON.parse(fs.readFileSync('/work/exp095-library-fingerprints/lab/realworld/layout-2.1.216.json', 'utf8'));
const cjs = layout.modules.filter((m) => m.kind === 'cjs').map((m) => [m.start, m.end]).sort((a, b) => a[0] - b[0]);
let ci = 0;
const pop = fs.readFileSync('data/population.jsonl', 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
pop.sort((a, b) => a.s - b.s);
const counts = new Map();
const unitToks = pop.map(() => new Set());
let ui = 0;
for (const t of acorn.tokenizer(src, { ecmaVersion: 'latest', allowHashBang: true, sourceType: 'script', allowReturnOutsideFunction: true, allowAwaitOutsideFunction: true })) {
  let key = null;
  if ((t.type.label === 'string' || t.type.label === 'template') && typeof t.value === 'string' && t.value.length >= 4 && t.value.length < 400) key = 's:' + t.value;
  else if ((t.type.label === 'name' || t.type.keyword) && typeof t.value === 'string' && t.value.length >= 6) key = 'n:' + t.value;
  if (!key) continue;
  while (ci < cjs.length && cjs[ci][1] <= t.start) ci++;
  const inCjs = ci < cjs.length && cjs[ci][0] <= t.start;
  let c = counts.get(key); if (!c) counts.set(key, (c = [0, 0]));
  c[inCjs ? 1 : 0]++;
  while (ui < pop.length && pop[ui].e <= t.start) ui++;
  if (!inCjs && ui < pop.length && pop[ui].s <= t.start) unitToks[ui].add(key);
}
fs.writeFileSync('data/bundletok.json', JSON.stringify(Object.fromEntries(counts)));
const w = fs.createWriteStream('data/units-tok.jsonl');
pop.forEach((u, i) => w.write(JSON.stringify({ i: u.i, s: u.s, t: [...unitToks[i]] }) + '\n'));
w.end();
console.error('tokens', counts.size);
