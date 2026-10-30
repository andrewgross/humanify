/**
 * exp164: the final apportionment by CONTENT. Parse the shipped era text,
 * hash every era statement (format-invariant), and join:
 *  - the real run's restorationApplications (bundle-statement hashes),
 *  - the dump's regions (which era ordinals Site A visited),
 *  - the shipped-located junk rows (era statement ordinals).
 * For each junk row: is its statement's hash captured? is an ANCESTOR
 * statement's hash captured (the inner-statement gap)? was the statement
 * visited by Site A?
 */
import * as fs from "node:fs";
import * as parser from "@babel/parser";
import { statementHash } from "../../../experiments/lib/js/statement-hash.js";

const dumpDiag = JSON.parse(fs.readFileSync("/work/exp164-mint-census/dump-run/exp164.diag.json", "utf8"));
const realDiag = JSON.parse(fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216.diag.json", "utf8"));
const junk: { name: string; start: number; end: number; stmt: number; file: string }[] = JSON.parse(
  fs.readFileSync("/work/exp164-mint-census/shipped-located-exp161-names-hop.json", "utf8"),
);

const shipped = fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body: any[] = p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression" ? p0.expression.body.body : ast.program.body;
console.log("era statements", body.length);
// hash -> ordinals; and a parent map (nested statement containment)
const hashOf = new Map<number, string>();
const byHash = new Map<string, number[]>();
const spans: { i: number; start: number; end: number }[] = [];
body.forEach((s: any, i: number) => {
  const h = statementHash(s);
  hashOf.set(i, h);
  const a = byHash.get(h) ?? [];
  a.push(i);
  byHash.set(h, a);
  spans.push({ i, start: s.start, end: s.end });
});
console.log("distinct era statement hashes", byHash.size);

// the applications' hashes (bundle statement prints) -> era ordinals
const apps = realDiag.libraryDiscovery.restorationApplications as { start: number; end: number; hash: string }[];
const appHashes = new Set(apps.map((a) => a.hash));
const appOrds = new Set<number>();
for (const h of appHashes) for (const o of byHash.get(h) ?? []) appOrds.add(o);
console.log(`applications ${apps.length}, distinct hashes ${appHashes.size}, era ordinals matching one: ${appOrds.size}`);

// Site A's visited ordinals
const visited = new Set((dumpDiag.eraRestoration.sites as any[]).map((s) => s.statement));
console.log("Site A visited ordinals", visited.size);

// ancestor: innermost enclosing era statement whose hash is in the apps
function ancestors(ord: number): number[] {
  const s = spans[ord];
  const out: number[] = [];
  for (const t of spans) {
    if (t.i === ord) continue;
    if (t.start < s.start && s.end < t.end) out.push(t.i);
  }
  return out.sort((a, b) => (spans[b].end - spans[b].start) - (spans[a].end - spans[a].start)); // innermost first
}

const rows = junk.map((r) => {
  const ord = r.stmt;
  const h = hashOf.get(ord);
  const selfCaptured = h != null && (byHash.get(h) ?? []).some((o) => appOrds.has(o)) || appHashes.has(h ?? "");
  const anc = ancestors(ord);
  const ancCaptured = anc.find((o) => appOrds.has(o));
  let cls: string;
  if (visited.has(ord)) cls = selfCaptured ? "visited+self-captured" : "visited+not-captured";
  else if (selfCaptured) cls = "NOT-visited+self-captured";
  else if (ancCaptured !== undefined) cls = `NOT-visited+ancestor-captured(depth ${anc.length - anc.indexOf(ancCaptured)})`;
  else cls = "NOT-visited+nothing-captured";
  return { ...r, cls, ancDepth: anc.length };
});
const buckets = new Map<string, number>();
const byPkg = new Map<string, Map<string, number>>();
for (const r of rows) {
  buckets.set(r.cls, (buckets.get(r.cls) ?? 0) + 1);
  const pkg = r.file.split("/").slice(1, 2).join("/") || r.file;
  const m = byPkg.get(r.cls) ?? new Map();
  m.set(pkg, (m.get(pkg) ?? 0) + 1);
  byPkg.set(r.cls, m);
}
console.log("\n--- the 684 by capture coverage ---");
for (const [c, n] of [...buckets.entries()].sort((a, b) => b[1] - a[1])) {
  const top = [...(byPkg.get(c) ?? new Map()).entries()].sort((a, b) => b[1] - a[1]).slice(0, 5).map(([p, x]) => `${p}:${x}`).join(" ");
  console.log(`${String(n).padStart(4)}  ${c}  ${top}`);
}
fs.writeFileSync("/tmp/exp164-coverage-rows.json", JSON.stringify(rows));
