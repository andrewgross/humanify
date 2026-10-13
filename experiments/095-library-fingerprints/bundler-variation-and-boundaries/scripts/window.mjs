// Sliding-window fingerprints (the boundary-free baseline): lexical tokens with identifiers masked (property names
// after `.` kept, literals by value), k-gram rolling hashes, winnowing (keep the minimum hash of every w consecutive
// k-grams). Output per build: data/windows/<build>.tsv  with  startChar \t endChar \t hash
//
// usage: node scripts/window.mjs [--jobs 56] [--filter s] [--k 20] [--w 8]
import { Worker, isMainThread, parentPort, workerData } from 'node:worker_threads';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
const require = createRequire(import.meta.url);
const acorn = require('/work/exp095-library-fingerprints/lab/tools/node_modules/acorn');
const LAB = '/work/exp095-library-fingerprints/lab';
const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../data/windows');

function fnv(s) {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h;
}

export function windows(src, k, w) {
  const toks = [];
  let prevDot = false;
  for (const t of acorn.tokenizer(src, { ecmaVersion: 'latest', allowHashBang: true, allowReturnOutsideFunction: true, allowAwaitOutsideFunction: true })) {
    const ty = t.type.label;
    let s;
    if (ty === 'name') s = prevDot ? 'P' + t.value : '$';
    else if (ty === 'string') s = 'S' + t.value;
    else if (ty === 'num') s = 'N' + t.value;
    else if (ty === 'template') s = 'T' + t.value;
    else if (ty === 'regexp') s = 'R' + t.value.pattern;
    else if (ty === 'privateId') s = '#';
    else s = t.type.keyword ? 'K' + t.type.keyword : ty;
    prevDot = ty === '.' || ty === '?.';
    toks.push([fnv(s), t.start, t.end]);
  }
  const n = toks.length - k + 1;
  if (n <= 0) return [];
  // two independent 32-bit polynomial hashes per k-gram (=> 64 bits)
  const B1 = 1000003,
    B2 = 911382323;
  let p1 = 1,
    p2 = 1;
  for (let i = 0; i < k - 1; i++) {
    p1 = Math.imul(p1, B1) >>> 0;
    p2 = Math.imul(p2, B2) >>> 0;
  }
  const H1 = new Uint32Array(n),
    H2 = new Uint32Array(n);
  let a = 0,
    b = 0;
  for (let i = 0; i < toks.length; i++) {
    const x = toks[i][0];
    if (i >= k) {
      const y = toks[i - k][0];
      a = (a - Math.imul(y, p1)) >>> 0;
      b = (b - Math.imul(y ^ 0x5bd1e995, p2)) >>> 0;
    }
    a = (Math.imul(a, B1) + x) >>> 0;
    b = (Math.imul(b, B2) + (x ^ 0x5bd1e995)) >>> 0;
    if (i >= k - 1) {
      H1[i - k + 1] = a;
      H2[i - k + 1] = b;
    }
  }
  // winnowing
  const out = [];
  let last = -1;
  for (let i = 0; i + w <= n; i++) {
    let m = i;
    for (let j = i + 1; j < i + w; j++) if (H1[j] <= H1[m]) m = j;
    if (m !== last) {
      out.push([toks[m][1], toks[m + k - 1][2], H1[m].toString(36) + '.' + H2[m].toString(36)]);
      last = m;
    }
  }
  return out;
}

function processBuild(dir, k, w) {
  const src = fs.readFileSync(path.join(LAB, dir, 'out.js'), 'utf8');
  const rows = windows(src, k, w);
  const outFile = path.join(OUT, dir.replace(/^builds\//, '') + '.tsv');
  fs.mkdirSync(path.dirname(outFile), { recursive: true });
  fs.writeFileSync(outFile, rows.map((r) => r.join('\t')).join('\n') + '\n');
}

const isEntry = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isMainThread && isEntry) {
  const args = process.argv.slice(2);
  const opt = (x, d) => (args.indexOf(x) >= 0 ? args[args.indexOf(x) + 1] : d);
  const jobs = +opt('--jobs', 56),
    filter = opt('--filter', ''),
    k = +opt('--k', 20),
    w = +opt('--w', 8);
  const builds = fs
    .readFileSync(path.join(LAB, 'labels/builds.jsonl'), 'utf8')
    .split('\n')
    .filter(Boolean)
    .map((l) => JSON.parse(l))
    .filter((b) => b.minified && b.dir.includes(filter))
    .map((b) => b.dir);
  let next = 0,
    done = 0;
  await new Promise((resolve) => {
    let live = 0;
    for (let i = 0; i < Math.min(jobs, builds.length); i++) {
      live++;
      const wk = new Worker(fileURLToPath(import.meta.url), { workerData: { k, w } });
      const feed = () => wk.postMessage(next < builds.length ? builds[next++] : null);
      wk.on('message', (m) => {
        if (m.err) console.error('FAIL', m.dir, m.err);
        done++;
        feed();
      });
      wk.on('exit', () => --live === 0 && resolve());
      feed();
    }
  });
  console.error('windows done', done);
} else if (!isMainThread) {
  parentPort.on('message', (dir) => {
    if (dir === null) process.exit(0);
    try {
      processBuild(dir, workerData.k, workerData.w);
      parentPort.postMessage({ dir });
    } catch (e) {
      parentPort.postMessage({ dir, err: String(e).slice(0, 300) });
    }
  });
}
