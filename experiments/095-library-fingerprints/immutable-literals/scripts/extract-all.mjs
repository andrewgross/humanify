// usage: node scripts/extract-all.mjs [--jobs 60]
// Builds the literal sets for everything the analysis uses, into ../data/:
//   sources.jsonl  one row per (pkg, version, file) of every .js/.mjs/.cjs/.json in lab/pkgs/*/node_modules (deduped)
//   spans.jsonl    one row per distinct span text (label sha256) in lab/labels/all.jsonl
//   cc.jsonl       Claude Code: realworld positives / negatives / negative functions, every layout unit of the raw
//                  bundle (cjs factory, lazy-esm group), and every top-level statement inside lazy-esm groups
import fs from "node:fs"; import path from "node:path"; import os from "node:os";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { extract, extractJson, parse } from "./lits.mjs";

const LAB = "/work/exp095-library-fingerprints/lab/";
const HERE = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const DATA = HERE + "/data/";
const CCIN = (v) => `/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-${v}/binary-decompiled/src/entrypoints/index.js`;
const CCV = ["2.1.198", "2.1.216"];

if (!isMainThread) {
  const { tasks, out } = workerData;
  const fd = fs.openSync(out, "w");
  const cache = new Map();
  const text = (t) => {
    if (t.slice) { let c = cache.get(t.path); if (!c) { c = fs.readFileSync(t.path, "utf8"); cache.clear(); cache.set(t.path, c); } return c.slice(t.slice[0], t.slice[1]); }
    return fs.readFileSync(t.path, "utf8");
  };
  let buf = [];
  for (const t of tasks) {
    let r;
    try {
      const s = text(t);
      if (t.json) r = extractJson(s); else r = extract(s, t.mode);
      buf.push(JSON.stringify({ ...t.meta, bytes: Buffer.byteLength(s), how: r.how, lits: [...r.lits] }));
    } catch (e) { buf.push(JSON.stringify({ ...t.meta, error: String(e).slice(0, 200), lits: [] })); }
    if (buf.length > 200) { fs.writeSync(fd, buf.join("\n") + "\n"); buf = []; }
  }
  if (buf.length) fs.writeSync(fd, buf.join("\n") + "\n");
  fs.closeSync(fd);
  parentPort.postMessage("done");
} else {
  const jobs = Number(process.argv[process.argv.indexOf("--jobs") + 1]) || 60;
  const only = process.argv.includes("--only") ? process.argv[process.argv.indexOf("--only") + 1].split(",") : ["sources", "sourcesplus", "spans", "cc"];
  fs.mkdirSync(DATA, { recursive: true });

  const sets = {};
  for (const which of ["sources", "sourcesplus"]) if (only.includes(which)) {
    // every JS/JSON file, owner = nearest package.json with name+version
    const tasks = new Map();
    const walk = (dir, owner) => {
      let ents; try { ents = fs.readdirSync(dir, { withFileTypes: true }); } catch { return; }
      let own = owner;
      if (ents.some((e) => e.name === "package.json" && e.isFile())) {
        try { const pj = JSON.parse(fs.readFileSync(dir + "/package.json", "utf8")); if (pj.name && pj.version) own = { pkg: pj.name, ver: pj.version, root: dir }; } catch {}
      }
      for (const e of ents) {
        const p = dir + "/" + e.name;
        if (e.isDirectory()) { if (e.name === ".bin") continue; walk(p, own); }
        else if (e.isFile() && own && /\.(m?js|cjs|json)$/.test(e.name) && !/\.d\.[mc]?ts$/.test(e.name)) {
          const rel = path.relative(own.root, p);
          if (rel.startsWith("node_modules/")) continue;
          const key = `${own.pkg}@${own.ver}/${rel}`;
          if (tasks.has(key)) continue;
          const size = fs.statSync(p).size;
          if (size > 4e6) continue;
          tasks.set(key, { path: p, mode: "source", json: e.name.endsWith(".json"), meta: { key, pkg: own.pkg, ver: own.ver, file: rel } });
        }
      }
    };
    for (const d of fs.readdirSync(LAB + "pkgs")) walk(LAB + "pkgs/" + d + "/node_modules", null);
    // "sourcesplus" = the lab packages + the Claude Code-era versions installed in ../pkgs-cc (see README)
    if (which === "sourcesplus") walk(HERE + "/pkgs-cc/node_modules", null);
    sets[which] = [...tasks.values()];
  }
  if (only.includes("spans")) {
    const seen = new Map();
    for (const l of fs.readFileSync(LAB + "labels/all.jsonl", "utf8").split("\n")) {
      if (!l) continue; const r = JSON.parse(l);
      if (seen.has(r.sha256)) continue;
      seen.set(r.sha256, { path: LAB + r.textPath, mode: "min", meta: { sha: r.sha256 } });
    }
    sets.spans = [...seen.values()];
  }
  if (only.includes("cc")) {
    const t = [];
    for (const v of CCV) {
      for (const set of ["positives", "negatives", "negative-functions"]) {
        fs.readFileSync(`${LAB}realworld/cc-${v}/${set}.jsonl`, "utf8").split("\n").filter(Boolean).forEach((l, i) => {
          const r = JSON.parse(l);
          t.push({ path: LAB + r.textPath, mode: "min", meta: { cc: v, set, i, pkg: r.pkg ?? "<app>", bytes0: r.bytes } });
        });
      }
      // layout units (char offsets into the decompiled bundle) + top-level statements inside lazy groups
      const lay = JSON.parse(fs.readFileSync(`${LAB}realworld/layout-${v}.json`, "utf8"));
      const code = fs.readFileSync(CCIN(v), "utf8");
      const { ast } = parse(code);
      let body = ast.body;
      for (const st of ast.body) { let e = st.expression; if (st.type === "ExpressionStatement" && e) { if (e.type === "CallExpression") e = e.callee; if (/Function/.test(e.type)) { body = e.body.body; break; } } }
      lay.modules.forEach((m, i) => t.push({ path: CCIN(v), slice: [m.startChar, m.endChar], mode: "min", meta: { cc: v, set: "unit", i, kind: m.kind, startChar: m.startChar, endChar: m.endChar } }));
      // statements: every top-level statement of the bundle body (cjs factories are one statement each)
      body.forEach((st, i) => t.push({ path: CCIN(v), slice: [st.start, st.end], mode: "min", meta: { cc: v, set: "stmt", i, startChar: st.start, endChar: st.end } }));
    }
    sets.cc = t;
  }
  for (const [name, tasks] of Object.entries(sets)) {
    // big files spread: sort by path so a worker's slice cache hits, then deal round-robin in blocks
    const n = Math.min(jobs, tasks.length);
    const parts = Array.from({ length: n }, () => []);
    if (name === "cc") { tasks.forEach((t, i) => parts[Math.floor((i * n) / tasks.length)].push(t)); }
    else tasks.forEach((t, i) => parts[i % n].push(t));
    console.log(name, tasks.length, "tasks on", n, "workers");
    await Promise.all(parts.map((p, k) => new Promise((res, rej) => {
      const w = new Worker(new URL(import.meta.url), { workerData: { tasks: p, out: `${DATA}${name}.part${k}` }, resourceLimits: { maxOldGenerationSizeMb: 4000 } });
      w.on("message", res); w.on("error", rej);
    })));
    const out = fs.openSync(`${DATA}${name === "sourcesplus" ? "sources-plus" : name}.jsonl`, "w");
    for (let k = 0; k < n; k++) { fs.writeSync(out, fs.readFileSync(`${DATA}${name}.part${k}`)); fs.unlinkSync(`${DATA}${name}.part${k}`); }
    fs.closeSync(out);
    console.log(name, "written");
  }
}
