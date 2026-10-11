// usage: node scripts/extract-stmts.mjs [--jobs 60]
// Every top-level statement of every MINIFIED lab build, labelled by the span that contains it, with its literal set.
// Writes data/stmts.jsonl (one row per statement) and data/stmt-lits.jsonl (one row per distinct statement text).
import fs from "node:fs"; import path from "node:path"; import crypto from "node:crypto";
import { Worker, isMainThread, parentPort, workerData } from "node:worker_threads";
import { extract, parse } from "./lits.mjs";
const LAB = "/work/exp095-library-fingerprints/lab/";
const DATA = "/work/exp095-library-fingerprints/immutable-literals/data/";

if (!isMainThread) {
  const { builds, out, k } = workerData;
  const rows = fs.openSync(`${out}.rows.part${k}`, "w"), lits = fs.openSync(`${out}.lits.part${k}`, "w");
  const seen = new Set();
  for (const b of builds) {
    const code = fs.readFileSync(LAB + "builds/" + b + "/out.js", "utf8");
    const labels = fs.readFileSync(LAB + "builds/" + b + "/labels.jsonl", "utf8").split("\n").filter(Boolean).map(JSON.parse);
    const { ast } = parse(code);
    if (!ast) continue;
    let body = ast.body;
    for (const st of ast.body) { let e = st.expression; if (st.type === "ExpressionStatement" && e) { if (e.type === "CallExpression") e = e.callee; if (/Function/.test(e.type) && e.body.body) { body = e.body.body; break; } } }
    let li = 0; const buf = [], lbuf = [];
    body.forEach((st, i) => {
      while (li < labels.length - 1 && labels[li].endChar <= st.start) li++;
      const L = labels[li] && labels[li].startChar <= st.start && st.start < labels[li].endChar ? labels[li] : null;
      const text = code.slice(st.start, st.end);
      const sha = crypto.createHash("sha1").update(text).digest("hex").slice(0, 16);
      buf.push(JSON.stringify({ build: b, i, n: body.length, startChar: st.start, endChar: st.end, bytes: text.length, sha, type: st.type,
        pkg: L ? L.pkg : "<none>", ver: L ? L.pkgVersion : null, file: L ? L.file : null, kind: L ? L.kind : null, purity: L ? L.purity : null, span: L ? L.idx : null }));
      if (!seen.has(sha)) { seen.add(sha); lbuf.push(JSON.stringify({ sha, lits: [...extract(text, "min").lits] })); }
    });
    fs.writeSync(rows, buf.join("\n") + "\n"); if (lbuf.length) fs.writeSync(lits, lbuf.join("\n") + "\n");
  }
  fs.closeSync(rows); fs.closeSync(lits);
  parentPort.postMessage("done");
} else {
  const jobs = Number(process.argv[process.argv.indexOf("--jobs") + 1]) || 60;
  const builds = fs.readFileSync(LAB + "labels/builds.jsonl", "utf8").split("\n").filter(Boolean).map(JSON.parse).filter((b) => b.minified).map((b) => b.dir.replace(/^builds\//, ""));
  console.log("builds", builds.length);
  const parts = Array.from({ length: jobs }, () => []);
  builds.forEach((b, i) => parts[i % jobs].push(b));
  const out = DATA + "stmt";
  await Promise.all(parts.map((p, k) => new Promise((res, rej) => { const w = new Worker(new URL(import.meta.url), { workerData: { builds: p, out, k }, resourceLimits: { maxOldGenerationSizeMb: 6000 } }); w.on("message", res); w.on("error", rej); })));
  const R = fs.openSync(DATA + "stmts.jsonl", "w"), Lf = fs.openSync(DATA + "stmt-lits.jsonl", "w");
  const seen = new Set();
  for (let k = 0; k < jobs; k++) {
    fs.writeSync(R, fs.readFileSync(`${out}.rows.part${k}`)); fs.unlinkSync(`${out}.rows.part${k}`);
    for (const l of fs.readFileSync(`${out}.lits.part${k}`, "utf8").split("\n")) { if (!l) continue; const sha = l.slice(8, 24); if (seen.has(sha)) continue; seen.add(sha); fs.writeSync(Lf, l + "\n"); }
    fs.unlinkSync(`${out}.lits.part${k}`);
  }
  console.log("distinct statements", seen.size);
}
