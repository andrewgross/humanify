// node scripts/build.mjs : entries/req.cjs per installed package version (only specs that resolve), then
// Bun 1.4.0 --minify --target node --format cjs --sourcemap=external  -> builds/bun-1.4.0/min/<app>/req/out.js(.map)
import fs from 'node:fs'; import path from 'node:path'; import { createRequire } from 'node:module'; import { execFile } from 'node:child_process';
const RW = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const BUN = '/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/.bin/bun';
const cfg = JSON.parse(fs.readFileSync(RW + '/versions.json', 'utf8'));
const jobs = [];
for (const p of cfg) for (const v of p.versions) {
  const app = `${p.name.replace('/', '__')}@${v}`, dir = `${RW}/pkgs/${app}`;
  if (!fs.existsSync(dir + '/.installed')) continue;
  const req = createRequire(dir + '/package.json');
  const specs = p.specs.filter(s => { try { req.resolve(s); return true; } catch { return false; } });
  if (!specs.length) continue;
  fs.mkdirSync(dir + '/entries', { recursive: true });
  fs.writeFileSync(dir + '/entries/req.cjs', `function rwAppMain(x){ if (!x) throw new Error("rw-app: no input given"); return Object.keys(x).length; }\n` + specs.map((s, i) => `const m${i} = require(${JSON.stringify(s)});`).join('\n') + `\nmodule.exports = { run: rwAppMain, ${specs.map((_, i) => 'm' + i).join(', ')} };\n`);
  fs.writeFileSync(dir + '/entries/dyn.cjs', `function rwAppMain(x){ if (!x) throw new Error("rw-app: no input given"); return Object.keys(x).length; }\n` + specs.map((s, i) => `const l${i} = () => import(${JSON.stringify(s)});`).join('\n') + `\nmodule.exports = { run: rwAppMain, ${specs.map((_, i) => 'l' + i).join(', ')} };\n`);
  for (const mode of ['req', 'dyn']) jobs.push({ app, mode, entry: `${dir}/entries/${mode}.cjs`, out: `${RW}/builds/bun-1.4.0/min/${app}/${mode}` });
}
let i = 0, ok = 0, fail = 0;
const run = j => new Promise(res => {
  if (fs.existsSync(j.out + '/out.js.map')) return res();
  fs.mkdirSync(j.out, { recursive: true });
  execFile(BUN, ['build', j.entry, '--target', 'node', '--format', 'cjs', '--minify', '--sourcemap=external', '--outdir', j.out + '/.tmp'], { cwd: path.dirname(j.entry), timeout: 300000, maxBuffer: 64 << 20 }, (e, so, se) => {
    if (!e) try { fs.renameSync(`${j.out}/.tmp/${j.mode}.js`, j.out + '/out.js'); fs.renameSync(`${j.out}/.tmp/${j.mode}.js.map`, j.out + '/out.js.map'); fs.rmSync(j.out + '/.tmp', { recursive: true }); } catch (x) { e = x; }
    if (e) { fail++; fs.appendFileSync(RW + '/logs/build-fail.txt', `${j.app} ${String(se || e.message).slice(0, 300)}\n`); fs.rmSync(j.out, { recursive: true, force: true }); } else ok++;
    res();
  });
});
await Promise.all(Array.from({ length: 48 }, async () => { while (i < jobs.length) await run(jobs[i++]); }));
console.log(`builds: ${jobs.length} ok=${ok} fail=${fail}`);
