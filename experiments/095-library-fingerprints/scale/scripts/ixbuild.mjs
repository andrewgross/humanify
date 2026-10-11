// usage: node scripts/ixbuild.mjs [--maxpos 1000] [--minpos 1] [--jobs 20]
// The OFFLINE INDEX BUILDER, stage 1 (install + build). For every row of data/versions.jsonl with pos in range:
//   1. ix/pkgs/<slug>@<ver>/: npm install <name>@<ver> --ignore-scripts (retry with --legacy-peer-deps)
//   2. strip `//# sourceMappingURL` comment lines under node_modules (esbuild chains input maps otherwise)
//   3. entries/req.cjs (require) and entries/dyn.mjs (import()), each with a tiny app function (lab/scripts/entries.mjs)
//   4. four minified builds: bun-1.4.0 x {req,dyn}, esbuild-0.28.2 x {req,dyn}, CJS output, node target, sourcemap.
//      A build that fails on an unresolvable specifier is retried with it marked --external (up to 4 rounds);
//      top-level await -> retried with ESM output.
// Status per version -> ix/logs/status.jsonl. Re-runnable: skips versions with ix/pkgs/<d>/.done
// No package code is ever executed (install scripts off; builds only parse/print).
import fs from "node:fs"; import path from "node:path"; import { execFile } from "node:child_process";
const S = "/work/exp095-library-fingerprints/scale", IX = S + "/ix", T = "/work/exp095-library-fingerprints/lab/tools";
const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, arr) => (x.startsWith("--") ? [...a, [x.slice(2), arr[i + 1]]] : a), []));
const MIN = Number(args.minpos || 1), MAX = Number(args.maxpos || 1000), JOBS = Number(args.jobs || 20);
const BUN = T + "/bun-1.4.0/node_modules/.bin/bun", ESB = T + "/node_modules/esbuild/bin/esbuild";
const rows = fs.readFileSync(S + "/data/versions.jsonl", "utf8").trim().split("\n").map((l) => JSON.parse(l)).filter((r) => r.pos >= MIN && r.pos <= MAX);
// dedupe (name, version): two roles can name one version
const seen = new Set(); const jobs = rows.filter((r) => { const k = r.name + "@" + r.version; if (seen.has(k)) return false; seen.add(k); return true; });
const status = fs.createWriteStream(IX + "/logs/status.jsonl", { flags: "a" });
const sh = (bin, a, opts = {}) => new Promise((res) => execFile(bin, a, { timeout: 300000, maxBuffer: 64 << 20, ...opts }, (e, so, se) => res({ ok: !e, out: String(so || ""), err: String(se || "") + (e && !se ? String(e.message) : "") })));
const app = (tag) => `function labAppMain_${tag}(x){ if (!x) throw new Error("lab-app ${tag}: no input given"); return Object.keys(x).length + ":${tag}"; }\n`;
function unresolved(err) {
  const s = new Set();
  for (const m of err.matchAll(/Could not resolve:? "([^"]+)"/g)) s.add(m[1]);
  for (const m of err.matchAll(/Could not resolve: '([^']+)'/g)) s.add(m[1]);
  return [...s];
}
async function build(dir, d, bundler, mode) {
  const entry = `${dir}/entries/${mode === "req" ? "req.cjs" : "dyn.mjs"}`;
  const out = `${IX}/builds/${bundler}/min/${d}/${mode}`;
  if (fs.existsSync(out + "/out.js.map")) return { ok: true, skipped: true };
  let ext = [], fmt = "cjs", last = "";
  for (let round = 0; round < 6; round++) {
    fs.rmSync(out, { recursive: true, force: true }); fs.mkdirSync(out, { recursive: true });
    let r;
    if (bundler.startsWith("bun")) {
      r = await sh(BUN, ["build", entry, "--target", "node", "--format", fmt, "--minify", "--sourcemap=external", ...ext.flatMap((x) => ["--external", x]), "--outdir", out + "/.tmp"], { cwd: dir });
      if (r.ok) {
        const base = path.basename(entry).replace(/\.[cm]?js$/, "");
        try { fs.renameSync(`${out}/.tmp/${base}.js`, out + "/out.js"); fs.renameSync(`${out}/.tmp/${base}.js.map`, out + "/out.js.map"); fs.rmSync(out + "/.tmp", { recursive: true }); }
        catch (e) { r = { ok: false, err: "rename: " + e.message }; }
      }
    } else {
      r = await sh(ESB, [entry, "--bundle", "--platform=node", "--format=" + fmt, "--log-level=error", "--minify", "--sourcemap=external", ...ext.map((x) => "--external:" + x), "--outfile=" + out + "/out.js"], { cwd: dir });
    }
    if (r.ok) return { ok: true, ext, fmt, rounds: round };
    last = r.err;
    const u = unresolved(r.err).filter((x) => !ext.includes(x));
    if (u.length) { ext = [...ext, ...u]; continue; }
    if (/[Tt]op-level await/.test(r.err) && fmt === "cjs") { fmt = "esm"; continue; }
    break;
  }
  fs.rmSync(out, { recursive: true, force: true });
  return { ok: false, ext, err: last.slice(0, 500) };
}
async function one(j) {
  const d = `${j.name.replace("/", "__")}@${j.version}`, dir = `${IX}/pkgs/${d}`;
  if (fs.existsSync(dir + "/.done")) return;
  const t0 = Date.now(); const st = { d, name: j.name, version: j.version, pos: j.pos, role: j.role };
  if (!fs.existsSync(dir + "/.installed")) {
    fs.rmSync(dir, { recursive: true, force: true }); fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(dir + "/package.json", JSON.stringify({ private: true, name: "ix-app", dependencies: { [j.name]: j.version } }) + "\n");
    let r = await sh("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", "--loglevel=error"], { cwd: dir, timeout: 600000 });
    if (!r.ok) { st.legacyPeer = true; r = await sh("npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", "--loglevel=error", "--legacy-peer-deps"], { cwd: dir, timeout: 600000 }); }
    if (!r.ok) { st.install = "FAIL"; st.err = r.err.slice(0, 400); status.write(JSON.stringify(st) + "\n"); fs.rmSync(dir + "/node_modules", { recursive: true, force: true }); fs.writeFileSync(dir + "/.done", "install-fail"); return; }
    await sh("bash", ["-c", `grep -rlZ --include='*.js' --include='*.mjs' --include='*.cjs' -E '^//[#@] sourceMappingURL=' node_modules 2>/dev/null | xargs -0 -r sed -i -E '/^\\/\\/[#@] sourceMappingURL=/d'`], { cwd: dir });
    fs.mkdirSync(dir + "/entries", { recursive: true });
    fs.writeFileSync(dir + "/entries/req.cjs", app("req") + `const m0 = require(${JSON.stringify(j.name)});\nmodule.exports = { run: labAppMain_req, m0 };\n`);
    fs.writeFileSync(dir + "/entries/dyn.mjs", app("dyn") + `export const loaders = [() => import(${JSON.stringify(j.name)})];\nexport { labAppMain_dyn };\n`);
    fs.writeFileSync(dir + "/.installed", "");
  }
  st.install = "ok"; st.builds = {};
  for (const b of ["bun-1.4.0", "esbuild-0.28.2"]) for (const m of ["req", "dyn"]) st.builds[`${b}/${m}`] = await build(dir, d, b, m);
  st.secs = Math.round((Date.now() - t0) / 1000);
  status.write(JSON.stringify(st) + "\n");
  fs.writeFileSync(dir + "/.done", "ok");
}
let i = 0, n = 0;
await Promise.all(Array.from({ length: JOBS }, async () => { while (i < jobs.length) { const j = jobs[i++]; try { await one(j); } catch (e) { status.write(JSON.stringify({ d: j.name + "@" + j.version, crash: String(e).slice(0, 300) }) + "\n"); } if (++n % 50 === 0) console.log(new Date().toISOString(), n, "/", jobs.length); } }));
console.log("done", jobs.length);
