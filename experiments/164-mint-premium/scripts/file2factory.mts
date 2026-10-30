/** exp164: match each junk-holding vendor file to its raw factory
 * declarator via rare-literal inverted index + multiset containment. */
import * as fs from "node:fs";
import * as parser from "@babel/parser";
import _traverse from "@babel/traverse";
const traverseMod: any = _traverse;
const RAW = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const ROOT = "/tmp/eval-work/exp161-names-hop/2.1.216";
const raw = fs.readFileSync(RAW, "utf8");
const rAst = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rBody: any[] = (() => {
  const p0: any = rAst.program.body[0];
  if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") return p0.expression.body.body;
  if (p0.type === "FunctionDeclaration") return p0.body.body;
  return rAst.program.body;
})();
type Fac = { i: number; start: number; end: number; bodyStart: number; bodyEnd: number };
const factories: Fac[] = [];
for (const st of rBody) {
  if (st.type !== "VariableDeclaration") continue;
  for (const d of st.declarations) {
    const init: any = d.init;
    if (!(init && (init.type === "CallExpression" || init.type === "NewExpression"))) continue;
    const fn = init.arguments?.find((a: any) => a.type === "ArrowFunctionExpression" || a.type === "FunctionExpression");
    if (!fn) continue;
    factories.push({ i: factories.length, start: d.start, end: d.end, bodyStart: fn.body.start, bodyEnd: fn.body.end });
  }
}
console.log("factories:", factories.length);
const litPos: { pos: number; v: string }[] = [];
traverseMod.default(rAst, {
  StringLiteral: (p: { node: { start: number; value: string } }) => { if (p.node.start != null) litPos.push({ pos: p.node.start, v: p.node.value }); },
  TemplateElement: (p: { node: { start: number; value: { cooked: string | null } } }) => { if (p.node.start != null) litPos.push({ pos: p.node.start, v: p.node.value.cooked ?? "" }); },
});
litPos.sort((a, b) => a.pos - b.pos);
const pos = litPos.map((x) => x.pos);
function litsIn(a: number, b: number): Map<string, number> {
  let lo = 0, hi = litPos.length;
  while (lo < hi) { const mid = (lo + hi) >> 1; if (pos[mid] < a) lo = mid + 1; else hi = mid; }
  const out = new Map<string, number>();
  for (let i = lo; i < litPos.length && pos[i] < b; i++) out.set(litPos[i].v, (out.get(litPos[i].v) ?? 0) + 1);
  return out;
}
// factory literal multisets (body span) + rare-literal inverted index
const facLits: Map<string, number>[] = [];
const litFac = new Map<string, number[]>();
for (const f of factories) {
  const m = litsIn(f.bodyStart, f.bodyEnd);
  facLits.push(m);
  for (const v of m.keys()) {
    if (v.length < 4) continue;
    const l = litFac.get(v) ?? [];
    if (l.length <= 2) l.push(f.i);
    litFac.set(v, l);
  }
}
const rare = [...litFac.entries()].filter(([, l]) => l.length <= 2);
console.log("distinct literals (len>3):", litFac.size, "rare (<=2 factories):", rare.length);

const junk = JSON.parse(fs.readFileSync("/work/exp164-mint-census/shipped-located-exp161-names-hop.json", "utf8")) as { stmt: number; file: string }[];
const files = [...new Set(junk.filter((r) => r.file.startsWith("vendor")).map((r) => r.file))];
let matched = 0, ambiguous = 0, none = 0;
const results: Record<string, { fac: number; cands: number; how: string } | string> = {};
for (const file of files) {
  const text = fs.readFileSync(`${ROOT}/${file}`, "utf8");
  const a = parser.parse(text, { sourceType: "unambiguous", errorRecovery: true });
  const fl = new Map<string, number>();
  traverseMod.default(a, {
    StringLiteral: (p: { node: { value: string } }) => { fl.set(p.node.value, (fl.get(p.node.value) ?? 0) + 1); },
    TemplateElement: (p: { node: { value: { cooked: string | null } } }) => { const v = p.node.value.cooked ?? ""; fl.set(v, (fl.get(v) ?? 0) + 1); },
  });
  // candidate factories: contain ALL rare file literals
  const cand = new Map<number, number>();
  for (const [v] of fl) {
    if (v.length < 4) continue;
    for (const fi of litFac.get(v) ?? []) cand.set(fi, (cand.get(fi) ?? 0) + 1);
  }
  const scored = [...cand.entries()].map(([fi, hits]) => {
    // containment: file lits ⊆ factory lits (count slack)
    const fm = facLits[fi];
    let missing = 0;
    for (const [v, c] of fl) missing += Math.max(0, c - (fm.get(v) ?? 0));
    return { fi, hits, missing };
  }).filter((s) => s.hits >= 2).sort((a, b) => a.missing - b.missing || b.hits - a.hits);
  if (scored.length === 0) { none++; results[file] = "none"; continue; }
  const best = scored[0];
  const ties = scored.filter((s) => s.missing === best.missing).length;
  if (ties > 1) { ambiguous++; results[file] = `ambiguous(${ties}) best fac ${best.fi} missing ${best.missing}`; continue; }
  matched++;
  results[file] = { fac: best.fi, cands: scored.length, how: `missing=${best.missing}` };
}
console.log(`matched ${matched}, ambiguous ${ambiguous}, none ${none} of ${files.length}`);
for (const [f, r] of Object.entries(results)) console.log(`${(f.split("/").slice(1, 2).join("/")).padEnd(28)} ${JSON.stringify(r)}`);
fs.writeFileSync("/work/exp164-mint-census/file2factory.json", JSON.stringify(results, null, 1));
