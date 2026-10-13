// Token index over installed npm SOURCE (pkgs/*/node_modules/**.{js,mjs,cjs}): string literal values (len>=4, also
// template chunks) and identifier/property names (len>=6) -> set of "pkg@ver" owning the file.
// usage: node scripts/srcindex.mjs <pkgsDir> <out.tsv>    (out: token \t pkg@ver|pkg@ver...)
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const acorn = require('/work/exp095-library-fingerprints/lab/tools/node_modules/acorn');
const [pkgsDir, outPath] = process.argv.slice(2);
const owner = new Map(); // file -> pkg@ver
const tok = new Map(); // token -> Set
const seenPkgVer = new Set();
function* walk(d) {
  let ents;
  try { ents = fs.readdirSync(d, { withFileTypes: true }); } catch { return; }
  for (const e of ents) {
    const p = path.join(d, e.name);
    if (e.isDirectory()) yield* walk(p);
    else if (/\.(c|m)?js$/.test(e.name) && !e.name.endsWith('.d.ts')) yield p;
  }
}
// enumerate package roots: node_modules/<name> or node_modules/@s/<name>, nested allowed
function pkgRoots(nm, acc) {
  let ents; try { ents = fs.readdirSync(nm, { withFileTypes: true }); } catch { return; }
  for (const e of ents) {
    if (!e.isDirectory() || e.name.startsWith('.')) continue;
    if (e.name.startsWith('@')) { for (const f of fs.readdirSync(path.join(nm, e.name), { withFileTypes: true })) if (f.isDirectory()) addRoot(path.join(nm, e.name, f.name), acc); }
    else addRoot(path.join(nm, e.name), acc);
  }
}
function addRoot(dir, acc) {
  let pj; try { pj = JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8')); } catch { return; }
  acc.push({ dir, id: `${pj.name}@${pj.version}` });
  pkgRoots(path.join(dir, 'node_modules'), acc);
}
function add(t, id) { let s = tok.get(t); if (!s) tok.set(t, (s = new Set())); s.add(id); }
let files = 0, bad = 0;
for (const inst of fs.readdirSync(pkgsDir)) {
  const roots = []; pkgRoots(path.join(pkgsDir, inst, 'node_modules'), roots);
  for (const { dir, id } of roots) {
    if (seenPkgVer.has(id)) continue; seenPkgVer.add(id);
    for (const f of walk(dir)) {
      if (f.slice(dir.length).includes('/node_modules/')) continue;
      let src; try { src = fs.readFileSync(f, 'utf8'); } catch { continue; }
      if (src.length > 8e6) continue;
      files++;
      try {
        for (const t of acorn.tokenizer(src, { ecmaVersion: 'latest', allowHashBang: true, sourceType: 'module', allowReturnOutsideFunction: true, allowAwaitOutsideFunction: true })) {
          if (t.type.label === 'string' || t.type.label === 'template') { if (typeof t.value === 'string' && t.value.length >= 4 && t.value.length < 400) add('s:' + t.value, id); }
          else if (t.type.label === 'name' || t.type.keyword) { if (typeof t.value === 'string' && t.value.length >= 6) add('n:' + t.value, id); }
        }
      } catch { bad++;
        for (const m of src.matchAll(/"((?:[^"\\\n]|\\.){4,200})"|'((?:[^'\\\n]|\\.){4,200})'/g)) add('s:' + (m[1] ?? m[2]), id);
      }
    }
  }
}
const w = fs.createWriteStream(outPath);
for (const [t, s] of tok) { if (t.includes('\t') || t.includes('\n')) continue; w.write(t + '\t' + [...s].join('|') + '\n'); }
w.end();
console.error('pkgVersions', seenPkgVer.size, 'files', files, 'tokenizeFail', bad, 'tokens', tok.size);
fs.writeFileSync(outPath + '.pkgs.json', JSON.stringify([...seenPkgVer].sort()));
