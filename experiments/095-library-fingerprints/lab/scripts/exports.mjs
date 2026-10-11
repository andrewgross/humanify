// usage: node scripts/exports.mjs  -> pkgs/<d>/exports.json {spec: [names]}  (import() each spec in a child node)
import fs from "node:fs"; import path from "node:path"; import { execFile } from "node:child_process";
const lab = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const cat = Object.fromEntries(JSON.parse(fs.readFileSync(lab + "/scripts/catalog.json")).packages.map((p) => [p.name, p]));
const jobs = [];
for (const p of JSON.parse(fs.readFileSync(lab + "/versions.json"))) for (const v of Object.values(p.versions)) {
  const d = `${p.name.replace("/", "__")}@${v}`; const specs = cat[p.name].specs || [p.name];
  jobs.push({ d, specs });
}
const run = (dir, spec) => new Promise((res) => {
  const f = path.join(dir, "_probe.mjs");
  fs.writeFileSync(f, `const m = await import(${JSON.stringify(spec)}); process.stdout.write(JSON.stringify(Object.keys(m))); process.exit(0);`);
  execFile("node", [f], { cwd: dir, timeout: 30000 }, (e, out) => { res(e ? null : JSON.parse(out || "null")); });
});
let i = 0;
async function worker() { while (i < jobs.length) { const j = jobs[i++]; const dir = path.join(lab, "pkgs", j.d); const r = {};
  for (const s of j.specs) r[s] = await run(dir, s);
  fs.writeFileSync(path.join(dir, "exports.json"), JSON.stringify(r)); fs.rmSync(path.join(dir, "_probe.mjs"), { force: true });
  console.log(j.d, Object.values(r).map((x) => (x ? x.length : "ERR")).join(",")); } }
await Promise.all(Array.from({ length: 24 }, worker));
