// exp095 discovery: the lab's unit model (bundler-variation-and-boundaries/scripts/cc_scan.mjs) over a Claude Code
// release WITHOUT its realworld labels (none exist for 2.1.86): raw.jsonl + an empty raw.labels.json.
// usage: node scan_units.mjs <bundle.js> <outdir>
import fs from 'node:fs';
import { parseAny, unitsOf } from '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts/scan.mjs';
import { canonUnit, longStrings, hashStr } from '/work/exp095-library-fingerprints/bundler-variation-and-boundaries/scripts/canon.mjs';

const [bundle, outdir] = process.argv.slice(2);
const src = fs.readFileSync(bundle, 'utf8');
const ast = parseAny(src);
const rows = [];
unitsOf(ast, src, (node, depth, parent, i, inner) => {
  const c = canonUnit(node, src);
  let fns = c.fns;
  if (inner && inner.length) {
    const a = inner[0].start;
    const b = inner[inner.length - 1].end;
    fns = fns.filter((f) => !(f.s >= a && f.e <= b));
  }
  let wk = null;
  if (inner) {
    const call = node.type === 'VariableDeclaration' ? node.declarations[0].init : node.type === 'VariableDeclarator' ? node.init : node.expression;
    const fn = call.arguments[0];
    wk = fn.type === 'FunctionExpression' || fn.params.length ? 'cjs' : 'esm';
  }
  rows.push({
    wk, i, d: depth, p: parent, w: inner ? 1 : 0, s: node.start, e: node.end, lab: -1,
    h0: c.h0, h1: c.h1, h2: c.h2, h3: c.h3, h4: c.h4, sk: c.sk, bag: c.bag, n: c.n, skl: c.skLen, skd: c.skd,
    fns: fns.map((f) => [f.s, f.e, f.n, f.h1, f.h2, f.h3, f.h4, f.skd]),
    ls: inner ? [] : longStrings(node).map((s) => hashStr(s).slice(0, 12)),
  });
});
fs.mkdirSync(outdir, { recursive: true });
fs.writeFileSync(`${outdir}/raw.jsonl`, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
fs.writeFileSync(`${outdir}/raw.labels.json`, '{}');
console.error('units', rows.length);
