// VERIFY: independent query set. Bundles packages with Bun 1.4.0 (--minify, CJS out, node target, deps BUNDLED),
// labels every top-level statement by the majority sourcemap owner package, extracts literals with the claimant's
// lits.mjs (unchanged).
// usage: node verify/build-label.mjs <projectDir> <pkgList.txt> <out.jsonl> [modes=imp,req,dyn]
import fs from "node:fs"; import path from "node:path"; import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { extract, parse } from "../scripts/lits.mjs";
const req = createRequire("/work/exp095-library-fingerprints/lab/tools/package.json");
const { decode } = req("@jridgewell/sourcemap-codec");
const BUN = "/work/exp095-library-fingerprints/lab/tools/bun-1.4.0/node_modules/bun/bin/bun.exe";
const [proj0, listFile, outFile, modesArg] = process.argv.slice(2); const proj = path.resolve(proj0);
const modes = (modesArg || "imp,req,dyn").split(",");
const pkgs = fs.readFileSync(listFile, "utf8").split("\n").map((s) => s.trim()).filter(Boolean);
const out = fs.openSync(outFile, "w");
const owner = (abs) => {
  const i = abs.lastIndexOf("/node_modules/");
  if (i < 0) return { pkg: "<app>", ver: null };
  const rest = abs.slice(i + 14).split("/"); const n = rest[0].startsWith("@") ? 2 : 1;
  const name = rest.slice(0, n).join("/"); let ver = null;
  try { ver = JSON.parse(fs.readFileSync(abs.slice(0, i + 14) + name + "/package.json", "utf8")).version; } catch {}
  return { pkg: name, ver, file: rest.slice(n).join("/") };
};
for (const p of pkgs) {
  const safe = p.replace(/\//g, "__");
  for (const mode of modes) {
    const dir = `${proj}/builds/${safe}/${mode}`; fs.mkdirSync(dir, { recursive: true });
    const entry = `${proj}/entries/${safe}.${mode}.${mode === "req" ? "cjs" : "mjs"}`; fs.mkdirSync(path.dirname(entry), { recursive: true });
    const APP = `function appMarker(){return "verify-app-${safe}-${mode}"}`;
    fs.writeFileSync(entry, mode === "imp" ? `import * as m0 from ${JSON.stringify(p)};\n${APP}\nglobalThis.__m=[m0,appMarker];\n`
      : mode === "req" ? `const m0=require(${JSON.stringify(p)});\n${APP}\nmodule.exports=[m0,appMarker];\n`
      : `${APP}\nexport const load=()=>import(${JSON.stringify(p)});\nglobalThis.__m=appMarker;\n`);
    try {
      execFileSync(BUN, ["build", entry, "--target", "node", "--format", "cjs", "--minify", "--sourcemap=external", "--outdir", dir, ...(process.env.EXT||"").split(",").filter(Boolean).flatMap((x)=>["--external",x])], { cwd: proj, stdio: "pipe", timeout: 120000 });
    } catch (e) { console.log("FAIL", p, mode, String(e.stderr || e).slice(0, 200)); continue; }
    const outJs = fs.readdirSync(dir).find((f) => f.endsWith(".js"));
    const code = fs.readFileSync(`${dir}/${outJs}`, "utf8"); const map = JSON.parse(fs.readFileSync(`${dir}/${outJs}.map`, "utf8"));
    const src = map.sources.map((s) => owner(path.resolve(dir, s)));
    const lineStart = [0]; for (let k = 0; k < code.length; k++) if (code.charCodeAt(k) === 10) lineStart.push(k + 1);
    const segs = []; decode(map.mappings).forEach((l, li) => { for (const s of l) if (s.length >= 4) segs.push([lineStart[li] + s[0], s[1]]); });
    segs.sort((a, b) => a[0] - b[0]);
    const lb = (x) => { let lo = 0, hi = segs.length; while (lo < hi) { const m = (lo + hi) >> 1; if (segs[m][0] < x) lo = m + 1; else hi = m; } return lo; };
    const { ast } = parse(code); if (!ast) { console.log("NOPARSE", p, mode); continue; }
    let body = ast.body;
    for (const st of ast.body) { let e = st.expression; if (st.type === "ExpressionStatement" && e) { if (e.type === "CallExpression") e = e.callee; if (/Function/.test(e.type) && e.body.body) { body = e.body.body; break; } } }
    const buf = [];
    body.forEach((st, i) => {
      const c = new Map(); let tot = 0;
      for (let k = lb(st.start); k < segs.length && segs[k][0] < st.end; k++) { const o = src[segs[k][1]]; const key = o.pkg + "@" + o.ver; c.set(key, (c.get(key) || 0) + 1); tot++; }
      let best = null, bn = 0; for (const [k, n] of c) if (n > bn) { bn = n; best = k; }
      const at = best ? best.lastIndexOf("@") : -1;
      const text = code.slice(st.start, st.end);
      buf.push(JSON.stringify({ query: p, mode, i, pkg: best ? best.slice(0, at) : "<unmapped>", ver: best ? best.slice(at + 1) : null, purity: tot ? bn / tot : null,
        bytes: Buffer.byteLength(text), head: text.slice(0, 160), lits: [...extract(text, "min").lits] }));
    });
    fs.writeSync(out, buf.join("\n") + "\n");
    console.log("ok", p, mode, body.length);
  }
}
fs.closeSync(out);
