// Fingerprint every unit of every (minified) lab build, with its ground-truth label.
//
// usage: node scripts/scan.mjs [--jobs 56] [--filter <substring>] [--unmin]
// output: data/units/<build>.jsonl (one row per unit), data/units/<build>.labels.json (label idx -> pkg/file/kind)
//
// A "unit" is a top-level statement of out.js (a multi-declarator `var` is split into one unit per declarator),
// and, one level down, each statement inside a module wrapper body (`var X=H(function(e,t){...})`,
// `var I=b(()=>{...})`). Ground truth = the lab label span containing the unit's start.
import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { canonUnit, longStrings, hashStr } from './canon.mjs';

const require = createRequire(import.meta.url);
const acorn = require('/work/exp095-library-fingerprints/lab/tools/node_modules/acorn');
const LAB = process.env.LAB_ROOT || '/work/exp095-library-fingerprints/lab';
const OUT = process.env.UNITS_OUT || path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/units');

export function parseAny(src) {
  const opts = { ecmaVersion: 'latest', allowHashBang: true, allowReturnOutsideFunction: true, allowAwaitOutsideFunction: true };
  try {
    return acorn.parse(src, { ...opts, sourceType: 'script' });
  } catch {
    return acorn.parse(src, { ...opts, sourceType: 'module' });
  }
}

// body statements of a module wrapper, or null
export function wrapperBody(stmt) {
  let call = null;
  if (stmt.type === 'VariableDeclaration' && stmt.declarations.length === 1) call = stmt.declarations[0].init;
  else if (stmt.type === 'VariableDeclarator') call = stmt.init;
  else if (stmt.type === 'ExpressionStatement') call = stmt.expression;
  if (!call || call.type !== 'CallExpression' || call.arguments.length !== 1) return null;
  let fn = call.arguments[0];
  // esbuild unminified: __commonJS({"path"(exports,module){...}})
  if (fn.type === 'ObjectExpression' && fn.properties.length === 1 && fn.properties[0].value && fn.properties[0].value.type === 'FunctionExpression') fn = fn.properties[0].value;
  if ((fn.type === 'FunctionExpression' || fn.type === 'ArrowFunctionExpression') && fn.body.type === 'BlockStatement') return fn.body.body;
  return null;
}

// top-level units of a program (descends into a single @bun-cjs style outer wrapper)
export function topStatements(ast) {
  let body = ast.body;
  if (body.length <= 3) {
    const big = body.find((s) => s.end - s.start > (ast.end - ast.start) * 0.9);
    if (big && big.type === 'ExpressionStatement') {
      let e = big.expression;
      if (e.type === 'CallExpression') e = e.callee;
      if (e.type === 'FunctionExpression' && e.body.body.length > 10) body = e.body.body;
    }
  }
  return body;
}

export function unitsOf(ast, src, emit) {
  let idx = 0;
  for (const stmt of topStatements(ast)) {
    const parts = stmt.type === 'VariableDeclaration' && stmt.declarations.length > 1 ? stmt.declarations : [stmt];
    for (const u of parts) {
      const inner = wrapperBody(u);
      const me = idx++;
      emit(u, 0, -1, me, inner);
      if (inner) {
        for (const s of inner) emit(s, 1, me, idx++, null);
      }
    }
  }
}

function processBuild(dir) {
  const src = fs.readFileSync(path.join(LAB, dir, 'out.js'), 'utf8');
  const labels = fs
    .readFileSync(path.join(LAB, dir, 'labels.jsonl'), 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((l) => JSON.parse(l));
  labels.sort((a, b) => a.startChar - b.startChar);
  const findLabel = (c) => {
    let lo = 0,
      hi = labels.length - 1,
      ans = -1;
    while (lo <= hi) {
      const m = (lo + hi) >> 1;
      if (labels[m].startChar <= c) {
        ans = m;
        lo = m + 1;
      } else hi = m - 1;
    }
    if (ans >= 0 && c < labels[ans].endChar) return labels[ans].idx;
    return -1;
  };
  const ast = parseAny(src);
  const rows = [];
  unitsOf(ast, src, (node, depth, parent, i, inner) => {
    const c = canonUnit(node, src);
    let fns = c.fns;
    if (inner && inner.length) {
      const a = inner[0].start,
        b = inner[inner.length - 1].end;
      fns = fns.filter((f) => !(f.s >= a && f.e <= b));
    }
    const ls = longStrings(node).map((s) => hashStr(s).slice(0, 12));
    rows.push({
      i,
      d: depth,
      p: parent,
      w: inner ? 1 : 0,
      s: node.start,
      e: node.end,
      lab: findLabel(node.start),
      h0: c.h0,
      h1: c.h1,
      h2: c.h2,
      h3: c.h3,
      h4: c.h4,
      sk: c.sk,
      bag: c.bag,
      n: c.n,
      skl: c.skLen,
      skd: c.skd,
      fns: fns.map((f) => [f.s, f.e, f.n, f.h1, f.h2, f.h3, f.h4, f.skd]),
      ls: inner ? [] : ls,
    });
  });
  const outFile = path.join(OUT, dir.replace(/^builds\//, '') + '.jsonl');
  fs.mkdirSync(path.dirname(outFile), { recursive: true });
  fs.writeFileSync(outFile, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
  const lab = {};
  for (const l of labels) lab[l.idx] = [l.pkg, l.pkgVersion, l.file, l.kind, l.marker, l.purity, l.startChar, l.endChar];
  fs.writeFileSync(outFile.replace(/\.jsonl$/, '.labels.json'), JSON.stringify(lab));
  return rows.length;
}

const isEntry = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isMainThread && isEntry) {
  const args = process.argv.slice(2);
  const opt = (k, d) => {
    const i = args.indexOf(k);
    return i >= 0 ? args[i + 1] : d;
  };
  const jobs = +opt('--jobs', 56);
  const filter = opt('--filter', '');
  const unmin = args.includes('--unmin');
  const listed = fs.existsSync(path.join(LAB, 'labels/builds.jsonl'));
  const globbed = () => {
    const out = [];
    for (const bd of fs.readdirSync(path.join(LAB, 'builds')))
      for (const m of ['min']) {
        const p = path.join(LAB, 'builds', bd, m);
        if (!fs.existsSync(p)) continue;
        for (const app of fs.readdirSync(p)) for (const mode of fs.readdirSync(path.join(p, app))) out.push(JSON.stringify({ dir: `builds/${bd}/${m}/${app}/${mode}`, minified: true }));
      }
    return out.join('\n');
  };
  const builds = (listed ? fs.readFileSync(path.join(LAB, 'labels/builds.jsonl'), 'utf8') : globbed())
    .split('\n')
    .filter(Boolean)
    .map((l) => JSON.parse(l))
    .filter((b) => (unmin ? true : b.minified) && b.dir.includes(filter))
    .map((b) => b.dir);
  console.error(`scanning ${builds.length} builds with ${jobs} workers`);
  let next = 0,
    done = 0,
    failed = 0;
  const t0 = Date.now();
  await new Promise((resolve) => {
    let live = 0;
    for (let w = 0; w < Math.min(jobs, builds.length); w++) {
      live++;
      const worker = new Worker(fileURLToPath(import.meta.url), { workerData: {}, resourceLimits: { maxOldGenerationSizeMb: 6000 } });
      const feed = () => {
        if (next < builds.length) worker.postMessage(builds[next++]);
        else worker.postMessage(null);
      };
      worker.on('message', (m) => {
        if (m.err) {
          failed++;
          console.error('FAIL', m.dir, m.err);
        }
        done++;
        if (done % 200 === 0) console.error(`${done}/${builds.length} ${((Date.now() - t0) / 1000).toFixed(0)}s`);
        feed();
      });
      worker.on('exit', () => {
        if (--live === 0) resolve();
      });
      feed();
    }
  });
  console.error(`done ${done} failed ${failed} in ${((Date.now() - t0) / 1000).toFixed(0)}s`);
} else if (!isMainThread) {
  parentPort.on('message', (dir) => {
    if (dir === null) process.exit(0);
    try {
      processBuild(dir);
      parentPort.postMessage({ dir });
    } catch (e) {
      parentPort.postMessage({ dir, err: String(e.stack || e).slice(0, 500) });
    }
  });
}
