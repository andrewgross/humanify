/**
 * exp164: the decisive apportionment by CONTENT, span-based.
 * For each junk binding's era statement, find its TRUE raw (bundle)
 * statement — by TS statementHash equality, simulating the beautifier's
 * declarator split for fragments — then ask:
 *   self-captured   the raw statement's span is a restorationApplication's span
 *   inner-captured  the raw statement sits inside one
 *   neither         no captured answer anywhere above it
 * plus whether Site A actually visited the era ordinal.
 */
import * as fs from "node:fs";
import * as parser from "@babel/parser";
import { statementHash } from "../../../experiments/lib/js/statement-hash.js";

const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const realDiag = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216.diag.json", "utf8"));
const junk: { name: string; start: number; end: number; stmt: number; file: string }[] = JSON.parse(
  fs.readFileSync("/work/exp164-mint-census/shipped-located-exp161-names-hop.json", "utf8"),
);
const apps = realDiag.libraryDiscovery.restorationApplications as { start: number; end: number; hash: string }[];
const appStarts = new Map<number, number>(); // start -> end
for (const a of apps) appStarts.set(a.start, a.end);
const visited = new Set((dumpDiag.eraRestoration.sites as any[]).map((s) => s.statement));
const regions = dumpDiag.eraRestoration.regions as { statements: number[]; raw_spans: [number, number][] }[];
const regionByStmt = new Map<number, { statements: number[]; raw_spans: [number, number][] }>();
for (const r of regions) for (const s of r.statements) regionByStmt.set(s, r);

// the bundle
const bundlePath = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const raw = fs.readFileSync(bundlePath, "utf8");
const rast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rbody: any[] = (rast.program.body[0] as any).expression.body.body;
console.log("bundle container statements", rbody.length);

// era side
const shipped = fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body: any[] = p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression" ? p0.expression.body.body : ast.program.body;

// bundle hash maps: whole-statement hash -> [starts]; fragment hash -> [starts]
const wholeByHash = new Map<string, number[]>();
const fragByHash = new Map<string, number[]>();
const startOf = new Map<number, number>(); // index -> start
rbody.forEach((s: any, i: number) => {
  startOf.set(i, s.start);
  const h = statementHash(s);
  const a = wholeByHash.get(h) ?? [];
  a.push(s.start);
  wholeByHash.set(h, a);
  if (s.type === "VariableDeclaration" && s.declarations.length > 1) {
    const initLess = s.declarations.filter((d: any) => !d.init);
    const withInit = s.declarations.filter((d: any) => d.init);
    const mk = (decls: any[], kind: string) => {
      const frag = { type: "VariableDeclaration", kind: s.kind, declarations: decls };
      const fh = statementHash(frag as any);
      const fa = fragByHash.get(fh) ?? [];
      fa.push(s.start);
      fragByHash.set(fh, fa);
    };
    if (initLess.length) mk(initLess, "initless");
    for (const d of withInit) mk([d], "init");
  }
});
console.log("distinct bundle whole hashes", wholeByHash.size, "fragment hashes", fragByHash.size);

// era hashes
const eraHash = new Map<number, string>();
body.forEach((s: any, i: number) => eraHash.set(i, statementHash(s)));

function trueRaw(ord: number): { start: number; how: string } | null {
  const h = eraHash.get(ord);
  if (!h) return null;
  const w = wholeByHash.get(h);
  if (w && w.length) return { start: w[0], how: "whole" };
  const f = fragByHash.get(h);
  if (f && f.length) return { start: f[0], how: "fragment" };
  return null;
}
// innermost capturing ancestor of a raw span
function innermostApp(start: number, end: number): { start: number; end: number } | null {
  let best: { start: number; end: number } | null = null;
  for (const [s, e] of appStarts) {
    if (s <= start && end <= e) {
      if (!best || (e - s) < (best.end - best.start)) best = { start: s, end: e };
    }
  }
  return best;
}

const rows = junk.map((r) => {
  const ord = r.stmt;
  const tr = trueRaw(ord);
  const region = regionByStmt.get(ord);
  const inRegion = !!region;
  let cls: string;
  if (tr) {
    // the raw statement's end: approximate with the app's end or the next statement's start
    let end = -1;
    {
      const idx = [...startOf.entries()].find(([, s]) => s === tr.start);
      if (idx) {
        const i = idx[0];
        end = i + 1 < rbody.length ? rbody[i + 1].start : raw.length;
        const s: any = rbody[i];
        end = Math.max(end, s.end);
      }
    }
    const self = appStarts.has(tr.start);
    const anc = self ? { start: tr.start, end: appStarts.get(tr.start)! } : innermostApp(tr.start, end);
    if (self) cls = "raw-self-captured";
    else if (anc) cls = "raw-inner-captured";
    else cls = "raw-nothing-captured";
    if (!visited.has(ord)) cls += "+not-visited";
  } else {
    cls = inRegion ? "no-true-raw(region)" : "no-true-raw";
  }
  return { ...r, cls };
});
const buckets = new Map<string, Map<string, number>>();
for (const r of rows) {
  const m = buckets.get(r.cls) ?? new Map();
  const pkg = r.file.split("/").slice(1, 2).join("/") || r.file;
  m.set(pkg, (m.get(pkg) ?? 0) + 1);
  buckets.set(r.cls, m);
}
console.log("\n--- the 684 by true-raw capture coverage ---");
const totals = new Map<string, number>();
for (const [c, m] of buckets) totals.set(c, [...m.values()].reduce((a, b) => a + b, 0));
for (const [c, n] of [...totals.entries()].sort((a, b) => b[1] - a[1])) {
  const top = [...(buckets.get(c) ?? new Map()).entries()].sort((a, b) => b[1] - a[1]).slice(0, 5).map(([p, x]) => `${p}:${x}`).join(" ");
  console.log(`${String(n).padStart(4)}  ${c}  ${top}`);
}
fs.writeFileSync("/tmp/exp164-coverage2-rows.json", JSON.stringify(rows));
