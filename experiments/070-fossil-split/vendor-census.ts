import * as fs from "node:fs";
import * as t from "@babel/types";
import { parseFileAst } from "./src/babel-utils.js";
import { extractFossilModules } from "./src/split/fossil-map.js";

const code = fs.readFileSync("/tmp/eval-work/exp070-r1/2.1.86/.humanify/humanified.js", "utf8");
const ast = parseFileAst(code)!;
let body: t.Statement[] = [];
(function walk(n: any) {
  if (!n || typeof n !== "object") return;
  if (Array.isArray(n)) { n.forEach(walk); return; }
  if (n.type === "BlockStatement" && Array.isArray(n.body) && n.body.length > body.length) body = n.body;
  for (const k of Object.keys(n)) { if (k !== "loc") walk(n[k]); }
})(ast.program);
const ex = extractFossilModules(body, body.map((_, i) => `h${i}`));
const n = ex.modules.length;
const APP = /claude|anthropic|tool_use|toolUse|\bmcp\b|sonnet|opus/i;
const VEND = /aws|azure|opentelemetry|smithy|protobuf|grpc|lodash|highlight\.js|prism|xmlbuilder|jsonwebtoken|google-auth|sharp|pngjs|qrcode|node_modules/i;
const text = (m: any) => m.statements.map((s: number) => {
  const st: any = body[s]; return st.start != null ? code.slice(st.start, Math.min(st.end, st.start + 4000)) : "";
}).join("\n");
const isApp = new Array<boolean>(n).fill(false), isVend = new Array<boolean>(n).fill(false);
ex.modules.forEach((m: any, i: number) => { const s = text(m); if (APP.test(s)) isApp[i] = true; else if (VEND.test(s)) isVend[i] = true; });
const seedApp = isApp.filter(Boolean).length, seedVend = isVend.filter(Boolean).length;
// upward: anything importing an app module is app
const importers = new Map<number, number[]>();
ex.modules.forEach((m: any, i: number) => { for (const imp of m.imports) { if (imp === i || imp >= n) continue; (importers.get(imp) ?? importers.set(imp, []).get(imp)!).push(i); } });
let q = ex.modules.map((_: any, i: number) => i).filter((i) => isApp[i]);
while (q.length) { const i = q.pop()!; for (const up of importers.get(i) ?? []) if (!isApp[up]) { isApp[up] = true; q.push(up); } }
// downward: everything a vendor module imports is vendor (unless app)
q = ex.modules.map((_: any, i: number) => i).filter((i) => isVend[i]);
while (q.length) { const i = q.pop()!; for (const dep of (ex.modules[i] as any).imports) { if (dep >= n || dep === i) continue; if (!isVend[dep] && !isApp[dep]) { isVend[dep] = true; q.push(dep); } } }
const app = isApp.filter(Boolean).length, vend = isVend.filter(Boolean).length;
console.log(`modules=${n} | seeds: app=${seedApp} vendor=${seedVend}`);
console.log(`after propagation: APP=${app} VENDOR=${vend} UNKNOWN=${n - app - vend}`);

// Rule 3: an unknown imported ONLY by vendor modules is vendor (fixpoint).
let changed = true;
while (changed) {
  changed = false;
  for (let i = 0; i < n; i++) {
    if (isApp[i] || isVend[i]) continue;
    const ups = importers.get(i) ?? [];
    if (ups.length > 0 && ups.every((u) => isVend[u])) { isVend[i] = true; changed = true; }
  }
}
// Rule 4: an unknown whose whole import closure is vendor (and non-trivial) is a package entry.
const closureAllVendor = (i: number) => {
  const seen = new Set<number>([i]); const st = [...(ex.modules[i] as any).imports];
  let count = 0;
  while (st.length) {
    const d = st.pop()!; if (d >= n || seen.has(d)) continue; seen.add(d); count++;
    if (isApp[d]) return { ok: false, count };
    if (!isVend[d]) { for (const x of (ex.modules[d] as any).imports) st.push(x); }
  }
  return { ok: true, count };
};
let entries = 0;
for (let i = 0; i < n; i++) {
  if (isApp[i] || isVend[i]) continue;
  const r = closureAllVendor(i);
  if (r.ok && r.count >= 3) { isVend[i] = true; entries++; }
}
const app2 = isApp.filter(Boolean).length, vend2 = isVend.filter(Boolean).length;
console.log(`after graph rules 3+4 (entries found: ${entries}): APP=${app2} VENDOR=${vend2} UNKNOWN=${n - app2 - vend2}`);
const leafUnknown = [...Array(n).keys()].filter((i) => !isApp[i] && !isVend[i] && (ex.modules[i] as any).imports.length === 0).length;
console.log(`  of the remaining unknowns, import-less leaves (truly ambiguous tiny utils): ${leafUnknown}`);

// Emit per-module classification joined to emitted file paths (ledger order == module order)
const ledger = JSON.parse(fs.readFileSync("/tmp/eval-work/exp070-r1/2.1.86/.humanify/split-ledger.json", "utf8"));
const out: Record<string, string> = {};
for (let i = 0; i < n; i++) {
  const f = ledger.fossilModules?.[i]?.file;
  if (f) out[f] = isApp[i] ? "app" : isVend[i] ? "vendor" : "unknown";
}
fs.writeFileSync("/tmp/classification.json", JSON.stringify(out));
console.log("classification written:", Object.keys(out).length);
