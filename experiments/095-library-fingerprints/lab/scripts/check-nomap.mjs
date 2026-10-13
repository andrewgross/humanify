// usage: node scripts/check-nomap.mjs [N=30]  -> logs/nomap-check.json
// Rebuilds N random min+unmin builds per bundler WITHOUT --sourcemap and checks the code is byte-identical to
// out.js once the trailing `//# debugId=` / `//# sourceMappingURL=` line is removed (i.e. labelling via a
// sourcemap did not change what was emitted).
import fs from "node:fs"; import path from "node:path"; import { execFileSync } from "node:child_process"; import os from "node:os";
const lab = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const N = Number(process.argv[2] || 30);
const T = lab + "/tools";
const BIN = { "bun-1.3.14": process.env.HOME + "/.bun/bin/bun", "bun-1.4.0": T + "/bun-1.4.0/node_modules/.bin/bun", "esbuild-0.28.2": T + "/node_modules/esbuild/bin/esbuild" };
const cat = JSON.parse(fs.readFileSync(lab + "/scripts/catalog.json")).packages;
const fmt = (app) => cat.find((p) => p.name.replace("/", "__") === app.slice(0, app.lastIndexOf("@")))?.format || "cjs";
const ext = (app) => cat.find((p) => p.name.replace("/", "__") === app.slice(0, app.lastIndexOf("@")))?.external || [];
const strip = (s) => s.replace(/\n*\/\/# (debugId|sourceMappingURL)=\S+\s*$/, "").replace(/\s+$/, "");
const res = {}; let seed = 7; const rnd = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
for (const b of Object.keys(BIN)) for (const m of ["min", "unmin"]) {
  const all = []; const base = path.join(lab, "builds", b, m);
  for (const app of fs.readdirSync(base)) for (const mode of fs.readdirSync(path.join(base, app))) all.push([app, mode]);
  const pick = Array.from({ length: N }, () => all[Math.floor(rnd() * all.length)]);
  let same = 0, diff = 0; const ex = [];
  for (const [app, mode] of pick) {
    const ed = path.join(lab, "pkgs", app, "entries"); const entry = fs.readdirSync(ed).find((f) => f.startsWith(mode + "."));
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "nomap-"));
    if (b.startsWith("bun")) execFileSync(BIN[b], ["build", path.join(ed, entry), "--target", "node", "--format", fmt(app), ...(m === "min" ? ["--minify"] : []), ...ext(app).flatMap((x) => ["--external", x]), "--outdir", tmp], { cwd: ed, stdio: "ignore" });
    else execFileSync(BIN[b], [path.join(ed, entry), "--bundle", "--platform=node", "--format=" + fmt(app), "--log-level=error", ...(m === "min" ? ["--minify"] : []), ...ext(app).map((x) => "--external:" + x), "--outfile=" + tmp + "/" + entry.replace(/\.[cm]?js$/, ".js")], { cwd: ed, stdio: "ignore" });
    const a = strip(fs.readFileSync(path.join(tmp, entry.replace(/\.[cm]?js$/, ".js")), "utf8"));
    const o = strip(fs.readFileSync(path.join(base, app, mode, "out.js"), "utf8"));
    if (a === o) same++; else { diff++; ex.push(`${app}/${mode}`); }
    fs.rmSync(tmp, { recursive: true });
  }
  res[`${b}/${m}`] = { same, diff, ex: ex.slice(0, 5) };
  console.log(b, m, same, diff, ex.slice(0, 3));
}
fs.writeFileSync(lab + "/logs/nomap-check.json", JSON.stringify(res, null, 1));
