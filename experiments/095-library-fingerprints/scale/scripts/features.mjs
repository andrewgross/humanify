// usage: node scripts/features.mjs <tasks.jsonl> <out.jsonl> [--jobs 24]
// One task per DISTINCT span text: {k, path, slice?:[startChar,endChar]}.
// Output per task: {k, bytes, u:[[type, nodes, nTok, content, h_mask], ...], l:[literals], pe?:parse error}
//   units: the subtree-hashes canonical form (subtree-hashes/scripts/canon.mjs, loose mode, identifiers masked),
//          every 'root' unit plus every stmt/fn/class unit of >= 20 nodes. content = distinct strings, property
//          names, globals and regexes inside the unit (the "content tokens" of SYNTHESIS §4).
//   literals: immutable-literals/scripts/lits.mjs extract(text, "min") (the minified-side literal set).
import fs from "node:fs";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
const CANON = "/work/exp095-library-fingerprints/subtree-hashes/scripts/canon.mjs";
const LITS = "/work/exp095-library-fingerprints/immutable-literals/scripts/lits.mjs";
const TY = { root: "r", stmt: "s", fn: "f", class: "c" };
if (!isMainThread) {
  const { parse, serialise, h64 } = await import(CANON);
  const { extract } = await import(LITS);
  const { tasks, out } = workerData;
  const fd = fs.openSync(out, "w"); let buf = [];
  let cachePath = null, cacheText = null;
  for (const t of tasks) {
    let text;
    try {
      if (t.slice) { if (cachePath !== t.path) { cacheText = fs.readFileSync(t.path, "utf8"); cachePath = t.path; } text = cacheText.slice(t.slice[0], t.slice[1]); }
      else text = fs.readFileSync(t.path, "utf8");
    } catch (e) { buf.push(JSON.stringify({ k: t.k, err: "read" })); continue; }
    const r = { k: t.k, bytes: Buffer.byteLength(text), u: [], l: [] };
    try {
      const ast = parse(text);
      const { tok, units } = serialise(ast, { loose: true, code: text });
      for (const u of units) {
        if (u.type !== "root" && u.nodes < 20) continue;
        const sl = tok.slice(u.a, u.b); const mask = new Array(sl.length); const content = new Set();
        for (let i = 0; i < sl.length; i++) {
          const x = sl[i]; const c = x.charCodeAt(0);
          if (c === 36) mask[i] = "I"; else { mask[i] = x; if (c === 83 || c === 46 || x.startsWith("G:") || x.startsWith("R/")) content.add(x); }
        }
        r.u.push([TY[u.type], u.nodes, sl.length, content.size, h64(mask.join(" "))]);
      }
    } catch (e) { r.pe = String(e.message).slice(0, 80); }
    try { r.l = [...extract(text, "min").lits]; } catch (e) { r.le = 1; }
    buf.push(JSON.stringify(r));
    if (buf.length >= 200) { fs.writeSync(fd, buf.join("\n") + "\n"); buf = []; }
  }
  if (buf.length) fs.writeSync(fd, buf.join("\n") + "\n");
  fs.closeSync(fd); parentPort.postMessage("done");
} else {
  const [inp, outp] = process.argv.slice(2);
  const jobs = Number(process.argv[process.argv.indexOf("--jobs") + 1]) || 24;
  const tasks = fs.readFileSync(inp, "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l));
  // sort by path so slice tasks share a cached file; deal in contiguous blocks
  tasks.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  const n = Math.min(jobs, Math.max(1, tasks.length));
  const parts = Array.from({ length: n }, () => []);
  if (tasks.some((t) => t.slice)) tasks.forEach((t, i) => parts[Math.floor((i * n) / tasks.length)].push(t));
  else tasks.forEach((t, i) => parts[i % n].push(t));
  await Promise.all(parts.map((p, i) => new Promise((res, rej) => {
    const w = new Worker(new URL(import.meta.url), { workerData: { tasks: p, out: `${outp}.part${i}` }, resourceLimits: { maxOldGenerationSizeMb: 6000 } });
    w.on("message", res); w.on("error", (e) => { console.error("worker", i, e); res(); });
  })));
  const fd = fs.openSync(outp, "w");
  for (let i = 0; i < n; i++) { if (fs.existsSync(`${outp}.part${i}`)) { fs.writeSync(fd, fs.readFileSync(`${outp}.part${i}`)); fs.unlinkSync(`${outp}.part${i}`); } }
  fs.closeSync(fd);
  console.log("features", tasks.length);
}
