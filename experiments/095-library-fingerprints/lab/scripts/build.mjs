// usage: node scripts/build.mjs [--bundlers a,b] [--jobs N] [--apps glob-substring]
// Builds every pkgs/<app>/entries/<mode>.* with every bundler, minified and not, with an external sourcemap.
// Output: builds/<bundlerId>/<min|unmin>/<app>/<mode>/out.js(.map) ; failures -> logs/build-fail.jsonl. Re-runnable (skips done).
import fs from "node:fs"; import path from "node:path"; import { execFile } from "node:child_process";
const lab = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, arr) => (x.startsWith("--") ? [...a, [x.slice(2), arr[i + 1]]] : a), []));
const T = lab + "/tools";
export const BUNDLERS = {
  "bun-1.3.14": { kind: "bun", bin: process.env.HOME + "/.bun/bin/bun" },
  "bun-1.4.0": { kind: "bun", bin: T + "/bun-1.4.0/node_modules/.bin/bun" },
  "bun-1.4.3": { kind: "bun", bin: T + "/bun-1.4.3/node_modules/.bin/bun" },
  "esbuild-0.28.2": { kind: "esbuild", bin: T + "/node_modules/esbuild/bin/esbuild" },
  "rollup-4.64.5": { kind: "rollup" },
};
// per-package output format (default cjs, like Claude Code). ink uses top-level await, which CJS output cannot express.
const FORMAT = Object.fromEntries(JSON.parse(fs.readFileSync(lab + "/scripts/catalog.json")).packages.filter((p) => p.format).map((p) => [p.name.replace("/", "__"), p.format]));
const fmtOf = (app) => FORMAT[app.slice(0, app.lastIndexOf("@"))] || "cjs";
const EXTERNAL = Object.fromEntries(JSON.parse(fs.readFileSync(lab + "/scripts/catalog.json")).packages.filter((p) => p.external).map((p) => [p.name.replace("/", "__"), p.external]));
const extOf = (app) => EXTERNAL[app.slice(0, app.lastIndexOf("@"))] || [];
const want = args.bundlers ? args.bundlers.split(",") : Object.keys(BUNDLERS);
const jobs = [];
for (const appDir of fs.readdirSync(lab + "/pkgs").sort()) {
  if (args.apps && !appDir.includes(args.apps)) continue;
  const ed = path.join(lab, "pkgs", appDir, "entries"); if (!fs.existsSync(ed)) continue;
  for (const f of fs.readdirSync(ed).sort()) for (const b of want) for (const min of [1, 0])
    jobs.push({ app: appDir, mode: f.split(".")[0], entry: path.join(ed, f), b, min, out: path.join(lab, "builds", b, min ? "min" : "unmin", appDir, f.split(".")[0]) });
}
function cmd(j) {
  const B = BUNDLERS[j.b];
  if (B.kind === "bun") return [B.bin, ["build", j.entry, "--target", "node", "--format", fmtOf(j.app), ...(j.min ? ["--minify"] : []), "--sourcemap=external", ...extOf(j.app).flatMap((x) => ["--external", x]), "--outdir", j.out + "/.tmp"]];
  if (B.kind === "esbuild") return [B.bin, [j.entry, "--bundle", "--platform=node", "--format=" + fmtOf(j.app), "--log-level=error", ...(j.min ? ["--minify"] : []), "--sourcemap=external", ...extOf(j.app).map((x) => "--external:" + x), "--outfile=" + j.out + "/out.js"]];
  return ["node", [lab + "/scripts/rollup-build.mjs", j.entry, j.out, String(j.min), fmtOf(j.app)]];
}
const failLog = fs.createWriteStream(lab + "/logs/build-fail.jsonl", { flags: "a" });
let done = 0, ok = 0, skip = 0, fail = 0;
const run = (j) => new Promise((res) => {
  if (fs.existsSync(j.out + "/out.js.map")) { skip++; return res(); }
  fs.mkdirSync(j.out, { recursive: true });
  const [bin, a] = cmd(j);
  execFile(bin, a, { cwd: path.dirname(j.entry), timeout: 300000, maxBuffer: 64 << 20 }, (e, so, se) => {
    if (!e && BUNDLERS[j.b].kind === "bun") {
      const t = j.out + "/.tmp", base = path.basename(j.entry).replace(/\.[cm]?js$/, "");
      try { fs.renameSync(`${t}/${base}.js`, j.out + "/out.js"); fs.renameSync(`${t}/${base}.js.map`, j.out + "/out.js.map"); fs.rmSync(t, { recursive: true }); } catch (x) { e = x; }
    }
    if (e) { fail++; failLog.write(JSON.stringify({ app: j.app, mode: j.mode, bundler: j.b, min: j.min, err: String(se || e.message).slice(0, 600) }) + "\n"); fs.rmSync(j.out, { recursive: true, force: true }); }
    else ok++;
    if (++done % 200 === 0) console.log(`${done}/${jobs.length} ok=${ok} fail=${fail} skip=${skip}`);
    res();
  });
});
let i = 0; const N = Number(args.jobs || 48);
await Promise.all(Array.from({ length: N }, async () => { while (i < jobs.length) await run(jobs[i++]); }));
console.log(`total ${jobs.length} ok=${ok} fail=${fail} skip=${skip}`);
