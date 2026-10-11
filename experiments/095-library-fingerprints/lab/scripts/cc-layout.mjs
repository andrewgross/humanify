// usage: node scripts/cc-layout.mjs <raw-bundle.js> <out.json>
// Top-level layout of a Bun CJS bundle (Claude Code): every CJS factory (exact span) and every lazy-ESM init
// marker (exact END; start = right after the previous wrapper statement, so it may also hold unmarked modules).
import fs from "node:fs"; import { createRequire } from "node:module"; import path from "node:path";
const lab = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const acorn = createRequire(lab + "/tools/package.json")("acorn");
const [inp, outp] = process.argv.slice(2);
const code = fs.readFileSync(inp, "utf8");
const ast = acorn.parse(code, { ecmaVersion: "latest", sourceType: "script", allowReturnOutsideFunction: true, allowHashBang: true });
let body = ast.body;
for (const st of ast.body) { let e = st.expression; if (st.type === "ExpressionStatement" && e) { if (e.type === "CallExpression") e = e.callee; if (/Function/.test(e.type)) { body = e.body.body; break; } } }
const helper = new Map();
for (const st of body.slice(0, 80)) if (st.type === "VariableDeclaration") for (const d of st.declarations) if (d.init && d.id.type === "Identifier" && d.init.end - d.init.start < 400) {
  const t = code.slice(d.init.start, d.init.end);
  if (/exports:\{\}/.test(t)) helper.set(d.id.name, "cjs"); else if (/\([\w$]+=0\)/.test(t)) helper.set(d.id.name, "lazy-esm");
}
const kindOf = (st) => { if (st.type !== "VariableDeclaration") return null; for (const d of st.declarations) { const i = d.init; if (i && i.type === "CallExpression" && i.callee.type === "Identifier" && helper.has(i.callee.name) && i.arguments.length === 1) return { kind: helper.get(i.callee.name), id: d.id.name }; } return null; };
const B = (x) => Buffer.byteLength(code.slice(0, x)); // only used sparsely below
const mods = []; let gapStart = null; let prevEnd = body[0]?.start ?? 0;
for (const st of body) {
  const k = kindOf(st);
  if (!k) { if (gapStart === null) gapStart = st.start; continue; }
  if (k.kind === "cjs") { mods.push({ kind: "cjs", id: k.id, startChar: st.start, endChar: st.end, gapBefore: gapStart !== null }); }
  else mods.push({ kind: "lazy-esm", id: k.id, startChar: gapStart ?? st.start, endChar: st.end, markerStartChar: st.start });
  gapStart = null;
}
// byte offsets (one pass)
const marks = [...new Set(mods.flatMap((m) => [m.startChar, m.endChar, m.markerStartChar].filter((x) => x != null)))].sort((a, b) => a - b);
const bmap = new Map(); let bi = 0, ci = 0;
for (const m of marks) { bi += Buffer.byteLength(code.slice(ci, m)); ci = m; bmap.set(m, bi); }
for (const m of mods) { m.start = bmap.get(m.startChar); m.end = bmap.get(m.endChar); }
fs.writeFileSync(outp, JSON.stringify({ input: inp, helpers: Object.fromEntries(helper), modules: mods }));
console.log(inp, "cjs", mods.filter((m) => m.kind === "cjs").length, "lazy-esm", mods.filter((m) => m.kind === "lazy-esm").length);
