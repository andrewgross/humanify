// Transform catalogue by ablation. For module pairs (same app/pkg/version/file) in two configs, count functions of A
// whose canonical hash is found in B (a) with every normalisation on, (b) with ONE transform switched off — the drop
// is how many matches that transform is necessary for. Also counts raw syntax features on each side.
//
// usage: node scripts/ablate.mjs <cfgA[:mode]> <cfgB[:mode]> <maxModules> <out.json>
import fs from 'node:fs';
import path from 'node:path';
import { parseAny } from './scan.mjs';
import { ablateUnit, TRANSFORMS } from './canon.mjs';

const LAB = '/work/exp095-library-fingerprints/lab';
const [specA, specB, maxMods, outFile] = process.argv.slice(2);
const [cfgA, modeA] = specA.split(':');
const [cfgB, modeB] = specB.split(':');
const byMode = !!modeA;

const rows = fs.readFileSync(LAB + '/labels/all.jsonl', 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
const keyOf = (r) => (byMode ? [r.app, r.pkg, r.pkgVersion, r.file] : [r.app, r.mode, r.pkg, r.pkgVersion, r.file]).join('|');
const A = new Map(),
  B = new Map();
for (const r of rows) {
  if (!r.minified || r.pkg.startsWith('<') || r.parts !== 1 || (r.purity ?? 1) < 0.9) continue;
  const cfg = r.build.split('/')[0];
  if (cfg === cfgA && (!modeA || r.mode === modeA)) A.set(keyOf(r), r);
  if (cfg === cfgB && (!modeB || r.mode === modeB)) B.set(keyOf(r), r);
}
const shared = [...A.keys()].filter((k) => B.has(k)).sort();
const step = Math.max(1, Math.floor(shared.length / +maxMods));
const sample = shared.filter((_, i) => i % step === 0).slice(0, +maxMods);

const CONFIGS = [['ALL-ON', []], ['NONE (L1-like)', TRANSFORMS], ...TRANSFORMS.map((t) => ['-' + t, [t]])];

// the function SET is fixed by the all-on pass (>= 12 tokens); ablated passes only change the hashes
const fnSets = new Map();
function hashesOf(r, off) {
  const src = fs.readFileSync(path.join(LAB, r.textPath), 'utf8');
  const ast = parseAny(src);
  const fns = [];
  const top = [];
  let set = fnSets.get(r.textPath);
  const first = !set;
  if (first) set = new Set();
  for (const st of ast.body) {
    const a = ablateUnit(st, src, off, first ? 12 : 1);
    top.push(a.h);
    for (const f of a.fns) {
      if (first) set.add(f[0]);
      if (set.has(f[0])) fns.push(f[3]);
    }
  }
  if (first) fnSets.set(r.textPath, set);
  return { fns, mod: top.join('|') };
}

// raw syntax feature counts (what each minifier actually writes)
const FEATURES = {
  'arrow fn': (n) => n.type === 'ArrowFunctionExpression',
  'function expr/decl': (n) => n.type === 'FunctionExpression' || n.type === 'FunctionDeclaration',
  '!0 / !1': (n) => n.type === 'UnaryExpression' && n.operator === '!' && n.argument.type === 'Literal' && typeof n.argument.value === 'number',
  'true/false literal': (n) => n.type === 'Literal' && typeof n.value === 'boolean',
  'void 0': (n) => n.type === 'UnaryExpression' && n.operator === 'void',
  'const decl': (n) => n.type === 'VariableDeclaration' && n.kind === 'const',
  'let decl': (n) => n.type === 'VariableDeclaration' && n.kind === 'let',
  'var decl': (n) => n.type === 'VariableDeclaration' && n.kind === 'var',
  '=== / !==': (n) => n.type === 'BinaryExpression' && (n.operator === '===' || n.operator === '!=='),
  '== / !=': (n) => n.type === 'BinaryExpression' && (n.operator === '==' || n.operator === '!='),
  'yoda (literal on left)': (n) => n.type === 'BinaryExpression' && ['==', '!=', '===', '!=='].includes(n.operator) && n.left.type === 'Literal' && n.right.type !== 'Literal',
  'new Error-like': (n) => n.type === 'NewExpression' && n.callee.type === 'Identifier' && /Error$/.test(n.callee.name),
  'Error-like call w/o new': (n) => n.type === 'CallExpression' && n.callee.type === 'Identifier' && /^[A-Z]\w*Error$/.test(n.callee.name),
  'comma sequence': (n) => n.type === 'SequenceExpression',
  'if statement': (n) => n.type === 'IfStatement',
  '&&/|| as statement': (n) => n.type === 'ExpressionStatement' && n.expression.type === 'LogicalExpression',
  'ternary': (n) => n.type === 'ConditionalExpression',
  'template literal': (n) => n.type === 'TemplateLiteral',
  'class decl': (n) => n.type === 'ClassDeclaration',
  'class expr': (n) => n.type === 'ClassExpression',
  'while': (n) => n.type === 'WhileStatement',
  'for(;;)': (n) => n.type === 'ForStatement',
  'optional chain ?.': (n) => n.type === 'ChainExpression',
  '??': (n) => n.type === 'LogicalExpression' && n.operator === '??',
  '"use strict"': (n) => n.type === 'ExpressionStatement' && n.directive === 'use strict',
  'catch without param': (n) => n.type === 'CatchClause' && !n.param,
  'destructuring pattern': (n) => n.type === 'ObjectPattern' || n.type === 'ArrayPattern',
};
function featureCounts(r, acc) {
  const src = fs.readFileSync(path.join(LAB, r.textPath), 'utf8');
  acc.bytes += Buffer.byteLength(src);
  (function walk(n) {
    if (!n || typeof n !== 'object') return;
    if (Array.isArray(n)) return n.forEach(walk);
    if (n.type) for (const [k, f] of Object.entries(FEATURES)) if (f(n)) acc[k] = (acc[k] || 0) + 1;
    for (const k in n) if (k !== 'start' && k !== 'end') walk(n[k]);
  })(parseAny(src));
}

const res = Object.fromEntries(CONFIGS.map(([name]) => [name, { fnHit: 0, modHit: 0 }]));
let fnTotal = 0,
  modTotal = 0;
const featA = { bytes: 0 },
  featB = { bytes: 0 };
for (const k of sample) {
  try {
    featureCounts(A.get(k), featA);
    featureCounts(B.get(k), featB);
    let counted = false;
    for (const [name, off] of CONFIGS) {
      const ha = hashesOf(A.get(k), off);
      const hb = hashesOf(B.get(k), off);
      if (!counted) {
        fnTotal += ha.fns.length;
        modTotal++;
        counted = true;
      }
      const c = new Map();
      for (const h of hb.fns) c.set(h, (c.get(h) || 0) + 1);
      for (const h of ha.fns)
        if (c.get(h)) {
          c.set(h, c.get(h) - 1);
          res[name].fnHit++;
        }
      if (ha.mod === hb.mod) res[name].modHit++;
    }
  } catch (e) {
    // unparsable span (rare) — skip
  }
}
const all = res['ALL-ON'];
const out = {
  pair: `${specA} vs ${specB}`,
  modules: modTotal,
  functions: fnTotal,
  allOn: { fnMatch: all.fnHit, modMatch: all.modHit },
  noneOn: res['NONE (L1-like)'],
  necessary: Object.fromEntries(TRANSFORMS.map((t) => [t, { fnLost: all.fnHit - res['-' + t].fnHit, modLost: all.modHit - res['-' + t].modHit }]).sort((a, b) => b[1].fnLost - a[1].fnLost)),
  featuresPerMB: { [specA]: perMB(featA), [specB]: perMB(featB) },
};
function perMB(f) {
  const o = {};
  for (const [k, v] of Object.entries(f)) if (k !== 'bytes') o[k] = +((v / f.bytes) * 1e6).toFixed(0);
  o.MB = +(f.bytes / 1e6).toFixed(2);
  return o;
}
fs.writeFileSync(outFile, JSON.stringify(out, null, 1));
console.log(out.pair, 'modules', modTotal, 'fns', fnTotal, 'allOn', JSON.stringify(out.allOn), 'none', JSON.stringify(out.noneOn));
