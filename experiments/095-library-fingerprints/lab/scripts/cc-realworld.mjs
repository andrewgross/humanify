// usage: node scripts/cc-realworld.mjs <ccVersion> <treeDir> <rawBundle>
//   e.g. node scripts/cc-realworld.mjs 2.1.216 /tmp/eval-work/candidate-e214db7a-scratch/2.1.216 \
//          /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js
// Needs realworld/layout-<v>.json (scripts/cc-layout.mjs). Writes realworld/cc-<v>/:
//   positives.jsonl + positives/<n>.min.js (raw minified factory) + positives/<n>.tree.js (the humanified vendor file)
//   negatives.jsonl + negatives/<n>.min.js (raw lazy-ESM group) + negatives/<n>.tree.js (src/ file it maps to)
//   negative-functions.jsonl + negative-functions/<n>.min.js (single raw top-level functions, the strictest app set)
import fs from "node:fs"; import path from "node:path"; import crypto from "node:crypto"; import { createRequire } from "node:module";
const lab = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const acorn = createRequire(lab + "/tools/package.json")("acorn");
const [ver, tree, rawPath] = process.argv.slice(2);
const out = path.join(lab, "realworld", `cc-${ver}`); fs.rmSync(out, { recursive: true, force: true });
for (const d of ["positives", "negatives", "negative-functions"]) fs.mkdirSync(path.join(out, d), { recursive: true });
const rawBuf = fs.readFileSync(rawPath); const layout = JSON.parse(fs.readFileSync(path.join(lab, "realworld", `layout-${ver}.json`)));
const rawText = (m) => rawBuf.subarray(m.start, m.end).toString("utf8");
const sha = (t) => crypto.createHash("sha256").update(t).digest("hex").slice(0, 16);
const strings = (code) => { const s = new Set(); try { for (const t of acorn.tokenizer(code, { ecmaVersion: "latest", allowReturnOutsideFunction: true, allowHashBang: true, sourceType: "script" })) {
  if ((t.type.label === "string" || t.type.label === "template") && typeof t.value === "string" && t.value.length >= 3 && !/^\.{1,2}\//.test(t.value) && !/\.humanify\//.test(t.value)) s.add(t.value); } } catch { /* partial set is fine */ } return s; };
const cjs = layout.modules.filter((m) => m.kind === "cjs");
cjs.forEach((m, i) => { m.i = i; m.strs = strings(rawText(m)); });
const df = new Map(); for (const m of cjs) for (const s of m.strs) df.set(s, (df.get(s) || 0) + 1);
const inv = new Map(); for (const m of cjs) for (const s of m.strs) if (df.get(s) <= 20) { if (!inv.has(s)) inv.set(s, []); inv.get(s).push(m); }
const jacc = (a, b) => { let n = 0; for (const x of a) if (b.has(x)) n++; return n / (a.size + b.size - n || 1); };
// ---- positives
const man = JSON.parse(fs.readFileSync(path.join(tree, "vendor/_bun-modules.json"))).factories;
const pos = []; let n = 0; const used = new Set();
const sameHash = new Map(); for (const e of man) if (e.identity) sameHash.set(e.structuralHash, (sameHash.get(e.structuralHash) || 0) + 1);
man.forEach((e, mi) => {
  if (!e.identity) return;
  const treeFile = path.join(tree, e.fileName); const tcode = fs.readFileSync(treeFile, "utf8"); const ts = strings(tcode);
  const cand = new Set(); for (const s of ts) for (const m of inv.get(s) || []) cand.add(m);
  const scored = [...cand].map((m) => [jacc(ts, m.strs), m]).sort((a, b) => b[0] - a[0]);
  let [best, second] = scored; let how = "literal-jaccard";
  let ok = best && best[0] >= 0.5 && (!second || best[0] - second[0] >= 0.1);
  if (best && best[0] >= 0.5 && !ok) {
    const close = scored.filter((x) => best[0] - x[0] < 0.1 && !used.has(x[1].i));
    // tie-break 1: the declared member/evidence literal is in exactly one close candidate
    const key = e.identity.member || e.identity.evidence;
    const withKey = close.filter((x) => x[1].strs.has(key));
    if (withKey.length === 1) { best = withKey[0]; second = close.find((x) => x !== best) || null; ok = true; how = "jaccard+declared-literal"; }
    // tie-break 2: several byte-for-byte copies of one module (same structuralHash in the manifest) -> take them in raw order
    else if (close.every((x) => x[0] === best[0]) && sameHash.get(e.structuralHash) > 1) { best = close.sort((a, b) => a[1].i - b[1].i)[0]; ok = true; how = "identical-copy-in-order"; }
  }
  const m = ok ? best[1] : null; if (m) used.add(m.i); const id = String(n++).padStart(4, "0");
  const row = { set: "cc-positive", ccVersion: ver, bundler: "bun", bundlerVersion: "1.4.0-or-earlier", minified: true, kind: "cjs",
    pkg: e.identity.package, pkgVersion: e.identity.version ?? null, member: e.identity.member ?? null, aliases: e.identity.aliases ?? null,
    identitySource: e.identity.source, evidence: e.identity.evidence, vendorFile: e.fileName, structuralHash: e.structuralHash, manifestIndex: mi,
    matched: !!m, matchedBy: m ? how : null, matchScore: best ? +best[0].toFixed(3) : null, matchMargin: best ? +(best[0] - (second ? second[0] : 0)).toFixed(3) : null,
    rawFactoryIndex: m ? m.i : null, indexAgrees: m ? m.i === mi : null, start: m ? m.start : null, end: m ? m.end : null,
    evidenceInRaw: m ? rawText(m).includes(e.identity.evidence.slice(0, 30)) : null,
    textPath: m ? `realworld/cc-${ver}/positives/${id}.min.js` : null, treeTextPath: `realworld/cc-${ver}/positives/${id}.tree.js` };
  if (m) fs.writeFileSync(path.join(lab, row.textPath), rawText(m)); fs.writeFileSync(path.join(lab, row.treeTextPath), tcode);
  if (m) row.sha256 = sha(rawText(m));
  pos.push(row);
});
fs.writeFileSync(path.join(out, "positives.jsonl"), pos.map((r) => JSON.stringify(r)).join("\n") + "\n");
console.log(ver, "positives", pos.length, "matched", pos.filter((r) => r.matched).length, "indexAgrees", pos.filter((r) => r.indexAgrees).length, "evidenceInRaw", pos.filter((r) => r.evidenceInRaw).length);
// ---- negatives: lazy-ESM groups that are clearly Claude Code's own code
const PRODUCT = [/tengu_[a-z0-9_]+/, /Claude Code/, /claude\.ai/, /CLAUDE\.md/, /ANTHROPIC_[A-Z_]+/, /\.claude\//, /claude-code/, /Anthropic/];
const LIBSIG = [/\/\*!/, /@license/, /node_modules\//, /\bname:"[^"]+",version:"\d+\.\d+\.\d+/, /Error\("@[a-z0-9-]+\/[a-z0-9-]+/, /https?:\/\/(?:www\.)?github\.com\/(?!anthropics)/];
// app-ness cross-check: the group's rarest literals must be found in a humanified src/ file and in no vendor/ file
const listJs = (d) => fs.readdirSync(d, { recursive: true }).filter((f) => /\.(m?js|cjs)$/.test(f)).map((f) => path.join(d, f));
const srcFiles = listJs(path.join(tree, "src")); const vendorFiles = listJs(path.join(tree, "vendor"));
const srcStr = new Map(); for (const f of srcFiles) for (const s of strings(fs.readFileSync(f, "utf8"))) if (s.length >= 12) { if (!srcStr.has(s)) srcStr.set(s, []); srcStr.get(s).push(f); }
const vendorAll = new Set(); for (const f of vendorFiles) for (const s of strings(fs.readFileSync(f, "utf8"))) if (s.length >= 12) vendorAll.add(s);
const groups = layout.modules.filter((m) => m.kind === "lazy-esm" && m.end - m.start >= 400 && m.end - m.start <= 60000);
const cands = [];
for (const g of groups) {
  const t = rawText(g);
  const hits = PRODUCT.filter((r) => r.test(t)).length; if (hits < 2) continue;
  if (LIBSIG.some((r) => r.test(t))) continue;
  const ss = [...strings(t)].filter((s) => s.length >= 12);
  if (ss.length < 3) continue;
  if (ss.some((s) => vendorAll.has(s))) continue; // shares a long literal with library code -> skip
  const files = new Map(); for (const s of ss) for (const f of srcStr.get(s) || []) files.set(f, (files.get(f) || 0) + 1);
  const best = [...files].sort((a, b) => b[1] - a[1])[0];
  if (!best || best[1] / ss.length < 0.6) continue;
  cands.push({ g, t, hits, tree: best[0], treeShare: best[1] / ss.length, nLong: ss.length });
}
// deterministic spread over sizes: sort by size, take every k-th, up to 100
cands.sort((a, b) => a.g.end - a.g.start - (b.g.end - b.g.start));
const seen = new Set(); const uniq = cands.filter((c) => { const h = sha(c.t); if (seen.has(h)) return false; seen.add(h); return true; });
const step = Math.max(1, uniq.length / 100); const pickN = []; for (let k = 0; k < uniq.length && pickN.length < 100; k += step) pickN.push(uniq[Math.floor(k)]);
const neg = pickN.map((c, k) => { const id = String(k).padStart(4, "0");
  fs.writeFileSync(path.join(out, "negatives", id + ".min.js"), c.t); fs.copyFileSync(c.tree, path.join(out, "negatives", id + ".tree.js"));
  return { set: "cc-negative", ccVersion: ver, bundler: "bun", minified: true, kind: "lazy-esm", marker: true, startExact: false, pkg: "<app>",
    start: c.g.start, end: c.g.end, bytes: c.g.end - c.g.start, productStringKinds: c.hits, longLiterals: c.nLong,
    treeFile: path.relative(tree, c.tree), treeShare: +c.treeShare.toFixed(3), sha256: sha(c.t),
    textPath: `realworld/cc-${ver}/negatives/${id}.min.js`, treeTextPath: `realworld/cc-${ver}/negatives/${id}.tree.js` }; });
fs.writeFileSync(path.join(out, "negatives.jsonl"), neg.map((r) => JSON.stringify(r)).join("\n") + "\n");
console.log(ver, "negative candidates", cands.length, "unique", uniq.length, "picked", neg.length);
// ---- negative functions: top-level function declarations inside the picked groups that themselves hold a product string
const nf = []; for (const r of neg) { const t = fs.readFileSync(path.join(lab, r.textPath), "utf8"); let ast; try { ast = acorn.parse(t, { ecmaVersion: "latest", sourceType: "script", allowReturnOutsideFunction: true }); } catch { continue; }
  for (const st of ast.body) if (st.type === "FunctionDeclaration") { const ft = t.slice(st.start, st.end); if (ft.length >= 120 && PRODUCT.some((p) => p.test(ft))) {
    const id = String(nf.length).padStart(5, "0"); fs.writeFileSync(path.join(out, "negative-functions", id + ".min.js"), ft);
    nf.push({ set: "cc-negative-function", ccVersion: ver, fromNegative: r.textPath, name: st.id.name, bytes: Buffer.byteLength(ft), sha256: sha(ft), textPath: `realworld/cc-${ver}/negative-functions/${id}.min.js` }); } } }
fs.writeFileSync(path.join(out, "negative-functions.jsonl"), nf.map((r) => JSON.stringify(r)).join("\n") + "\n");
console.log(ver, "negative functions", nf.length);
