// usage: node scripts/srclits.mjs [--jobs 24]
// The literal index's input, built from npm SOURCE (no bundler): every .js/.mjs/.cjs/.json file (<= 4 MB, not .d.ts)
// of every package@version installed under ix/pkgs/*/node_modules (nested node_modules included, each file owned by
// its nearest package.json with name+version), deduplicated by (pkg, version, file).
// Literal extraction: immutable-literals/scripts/lits.mjs, mode "source" (JSON: extractJson).
// -> data/srclits.jsonl   {pkg, ver, file, csha, lits}   one row per distinct file CONTENT per (pkg,ver,file)
// -> data/srcpos.json     {"pkg@ver": min root position that installs it}   (index scale membership)
import fs from "node:fs"; import path from "node:path"; import crypto from "node:crypto";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
const S = "/work/exp095-library-fingerprints/scale";
const LITS = "/work/exp095-library-fingerprints/immutable-literals/scripts/lits.mjs";
if (!isMainThread) {
  const { extract, extractJson } = await import(LITS);
  const { tasks, out } = workerData; const fd = fs.openSync(out, "w"); let buf = [];
  for (const t of tasks) {
    let r; try { const s = fs.readFileSync(t.path, "utf8"); r = t.json ? extractJson(s) : extract(s, "source"); } catch { r = { lits: new Set(), how: "err" }; }
    buf.push(JSON.stringify({ csha: t.csha, how: r.how, lits: [...r.lits] }));
    if (buf.length >= 500) { fs.writeSync(fd, buf.join("\n") + "\n"); buf = []; }
  }
  if (buf.length) fs.writeSync(fd, buf.join("\n") + "\n"); fs.closeSync(fd); parentPort.postMessage(1);
} else {
  const jobs = Number(process.argv[process.argv.indexOf("--jobs") + 1]) || 24;
  const pos = {}; for (const l of fs.readFileSync(S + "/data/versions.jsonl", "utf8").split("\n").filter(Boolean)) { const r = JSON.parse(l); const d = r.name.replace("/", "__") + "@" + r.version; pos[d] = Math.min(pos[d] ?? 1e9, r.pos); }
  const files = new Map(); // "pkg@ver/file" -> {pkg,ver,file,path}
  const pkgPos = {};
  const walk = (dir, owner, rootPos) => {
    let ents; try { ents = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
    let own = owner;
    if (ents.some((e) => e.name === "package.json" && e.isFile())) {
      try { const pj = JSON.parse(fs.readFileSync(dir + "/package.json", "utf8")); if (pj.name && pj.version) { own = { pkg: pj.name, ver: pj.version, root: dir }; const k = pj.name + "@" + pj.version; pkgPos[k] = Math.min(pkgPos[k] ?? 1e9, rootPos); } } catch {}
    }
    for (const e of ents) {
      const p = dir + "/" + e.name;
      if (e.isDirectory()) { if (e.name === ".bin" || e.name === ".cache") continue; walk(p, own, rootPos); }
      else if (e.isFile() && own && /\.(m?js|cjs|json)$/.test(e.name) && !/\.d\.[mc]?ts$/.test(e.name)) {
        const rel = path.relative(own.root, p); if (rel.startsWith("node_modules/")) continue;
        const key = `${own.pkg}@${own.ver}/${rel}`; if (files.has(key)) continue;
        let size; try { size = fs.statSync(p).size; } catch { continue; } if (size > 4e6) continue;
        files.set(key, { pkg: own.pkg, ver: own.ver, file: rel, path: p });
      }
    }
  };
  const apps = fs.readdirSync(S + "/ix/pkgs").sort((a, b) => (pos[a] ?? 1e9) - (pos[b] ?? 1e9));
  for (const d of apps) walk(`${S}/ix/pkgs/${d}/node_modules`, null, pos[d] ?? 1e9);
  console.log("files", files.size, "pkg@ver", Object.keys(pkgPos).length);
  // content hash -> extract once
  const bySha = new Map();
  for (const f of files.values()) { let b; try { b = fs.readFileSync(f.path); } catch { continue; } f.csha = crypto.createHash("sha1").update(b).digest("hex").slice(0, 16); if (!bySha.has(f.csha)) bySha.set(f.csha, { csha: f.csha, path: f.path, json: f.path.endsWith(".json") }); }
  const tasks = [...bySha.values()]; console.log("distinct contents", tasks.length);
  const n = jobs; const parts = Array.from({ length: n }, () => []); tasks.forEach((t, i) => parts[i % n].push(t));
  await Promise.all(parts.map((p, i) => new Promise((res) => { const w = new Worker(new URL(import.meta.url), { workerData: { tasks: p, out: `${S}/data/srclits.c${i}` }, resourceLimits: { maxOldGenerationSizeMb: 6000 } }); w.on("message", res); w.on("error", (e) => { console.error(e); res(); }); })));
  const fd = fs.openSync(S + "/data/srclits-content.jsonl", "w");
  for (let i = 0; i < n; i++) { fs.writeSync(fd, fs.readFileSync(`${S}/data/srclits.c${i}`)); fs.unlinkSync(`${S}/data/srclits.c${i}`); }
  fs.closeSync(fd);
  const fm = fs.openSync(S + "/data/srcfiles.jsonl", "w"); let buf = [];
  for (const f of files.values()) { if (!f.csha) continue; buf.push(JSON.stringify({ pkg: f.pkg, ver: f.ver, file: f.file, csha: f.csha })); if (buf.length > 5000) { fs.writeSync(fm, buf.join("\n") + "\n"); buf = []; } }
  if (buf.length) fs.writeSync(fm, buf.join("\n") + "\n"); fs.closeSync(fm);
  fs.writeFileSync(S + "/data/srcpos.json", JSON.stringify(pkgPos));
  console.log("done");
}
