/**
 * exp164 census v3: match each junk-holding VENDOR FILE (split-emitted
 * module-<hash8>.js, one module's body) to its placement group by literal
 * containment against the group's raw span, then classify every junk
 * statement by the group's claims and restoration applications.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import * as parser from "@babel/parser";
import _traverse from "@babel/traverse";
const traverseMod: any = _traverse;

const LABEL = "exp161-names-hop";
const RAW = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const ROOT = `/tmp/eval-work/${LABEL}/2.1.216`;
const diag = JSON.parse(fs.readFileSync(`/tmp/eval-work/${LABEL}/2.1.216.diag.json`, "utf8"));
const ld = diag.libraryDiscovery;
const junk: { name: string; start: number; end: number; stmt: number; file: string }[] = JSON.parse(
  fs.readFileSync(`/work/exp164-mint-census/shipped-located-${LABEL}.json`, "utf8"),
);
const raw = fs.readFileSync(RAW, "utf8");

// literals with positions in the raw AST
const rAst = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rawLits: { pos: number; v: string }[] = [];
traverseMod.default(rAst, {
  StringLiteral: (p: { node: { start: number; value: string } }) => {
    if (p.node.start != null) rawLits.push({ pos: p.node.start, v: p.node.value });
  },
  TemplateElement: (p: { node: { start: number; value: { cooked: string | null } } }) => {
    if (p.node.start != null) rawLits.push({ pos: p.node.start, v: p.node.value.cooked ?? "" });
  },
});
rawLits.sort((a, b) => a.pos - b.pos);
const rawLitPos = rawLits.map((x) => x.pos);
function litsIn(a: number, b: number): string[] {
  let lo = 0;
  let hi = rawLits.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (rawLitPos[mid] < a) lo = mid + 1;
    else hi = mid;
  }
  const out: string[] = [];
  for (let i = lo; i < rawLits.length && rawLitPos[i] < b; i++) out.push(rawLits[i].v);
  return out;
}

type Group = { i: number; start: number; end: number; lits: string[]; freq: Map<string, number> };
const groups: Group[] = (ld.placementGroups as any[]).map((g, i) => {
  const start = g.statements[0].start;
  const end = g.statements[g.statements.length - 1].end;
  const lits = litsIn(start, end);
  const freq = new Map<string, number>();
  for (const v of lits) freq.set(v, (freq.get(v) ?? 0) + 1);
  return { i, start, end, lits, freq };
});
groups.sort((a, b) => a.start - b.start);
console.log("groups:", groups.length, "raw literal positions:", rawLits.length);

// inverted index: literal -> groups containing it (rare literals only)
const litGroups = new Map<string, number[]>();
for (const g of groups) {
  for (const [v, c] of g.freq) {
    if (c > 3) continue; // skip common literals
    const l = litGroups.get(v) ?? [];
    l.push(g.i);
    litGroups.set(v, l);
  }
}

// junk-holding vendor files
const junkByFile = new Map<string, Map<number, { name: string; file: string }[]>>();
for (const r of junk) {
  if (!r.file.startsWith("vendor")) continue;
  let m = junkByFile.get(r.file);
  if (!m) {
    m = new Map();
    junkByFile.set(r.file, m);
  }
  const list = m.get(r.stmt) ?? [];
  list.push({ name: r.name, file: r.file });
  m.set(r.stmt, list);
}

function fileLits(text: string): Map<string, number> {
  const ast = parser.parse(text, { sourceType: "unambiguous", errorRecovery: true });
  const out = new Map<string, number>();
  traverseMod.default(ast, {
    StringLiteral: (p: { node: { value: string } }) => {
      out.set(p.node.value, (out.get(p.node.value) ?? 0) + 1);
    },
    TemplateElement: (p: { node: { value: { cooked: string | null } } }) => {
      const v = p.node.value.cooked ?? "";
      out.set(v, (out.get(v) ?? 0) + 1);
    },
  });
  return out;
}

function missingCount(fileFreq: Map<string, number>, g: Group): number {
  let missing = 0;
  let total = 0;
  for (const [v, c] of fileFreq) {
    total += c;
    missing += Math.max(0, c - (g.freq.get(v) ?? 0));
  }
  return { missing, total }.missing;
}

const fileGroup = new Map<string, { group?: Group; cands: number; missing: number; how: string }>();
for (const [file] of junkByFile) {
  const text = fs.readFileSync(path.join(ROOT, file), "utf8");
  const ff = fileLits(text);
  // rarest literals of the file
  const entries = [...ff.entries()].sort((a, b) => a[1] - b[1]);
  const rare = entries.slice(0, 6).map(([v]) => v).filter((v) => v.length > 3);
  const candSet = new Set<number>();
  if (rare.length > 0) {
    for (const v of rare) for (const gi of litGroups.get(v) ?? []) candSet.add(gi);
  } else {
    for (let i = 0; i < groups.length; i++) candSet.add(i);
  }
  let best: Group | undefined;
  let bestMissing = Number.MAX_SAFE_INTEGER;
  let nBest = 0;
  for (const gi of candSet) {
    const g = groups[gi];
    const m = missingCount(ff, g);
    if (m < bestMissing) {
      bestMissing = m;
      best = g;
      nBest = 1;
    } else if (m === bestMissing) nBest++;
  }
  const total = [...ff.values()].reduce((a, b) => a + b, 0);
  fileGroup.set(file, {
    group: bestMissing <= Math.max(5, total * 0.05) ? best : undefined,
    cands: candSet.size,
    missing: bestMissing,
    how: nBest > 1 ? `tie${nBest}` : "unique",
  });
  console.log(`${file.split("/").slice(1, 2).join("/").padEnd(30)} lits=${total} missing=${bestMissing} cands=${candSet.size} group=${best?.i} ${fileGroup.get(file)!.how}`);
}
fs.writeFileSync(
  `/work/exp164-mint-census/file-groups-${LABEL}.json`,
  JSON.stringify([...fileGroup.entries()].map(([f, v]) => ({ file: f, group: v.group?.i, missing: v.missing, how: v.how })), null, 1),
);
