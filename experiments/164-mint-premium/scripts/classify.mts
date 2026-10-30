/**
 * exp164: classify each vendor-file junk binding of the names-mode hop by
 * hypothesis. Bridge: the junk bindings live in shipped statements; the
 * era's restructure turned each raw factory declarator (`var Xq=b(()=>{…})`,
 * often several joined in one raw `var`) into its own shipped statement, so
 * each junk statement is matched back to its RAW DECLARATOR (fallback: the
 * raw container statement) by literal-multiset + junk-name-subset signature
 * — renames never touch either. Claims and restoration applications are in
 * raw bundle coordinates, so the raw declarator span classifies the
 * statement. argv: <rawBundlePath> [label]
 */
import * as fs from "node:fs";
import * as parser from "@babel/parser";

const LABEL = process.argv[3] ?? "exp161-names-hop";
const RAW = process.argv[2];
const ROOT = `/tmp/eval-work/${LABEL}/2.1.216`;
const diag = JSON.parse(fs.readFileSync(`/tmp/eval-work/${LABEL}/2.1.216.diag.json`, "utf8"));
const ld = diag.libraryDiscovery;
const junk: { name: string; start: number; end: number; stmt: number; file: string }[] = JSON.parse(
  fs.readFileSync(`/work/exp164-mint-census/shipped-located-${LABEL}.json`, "utf8"),
);

const shipped = fs.readFileSync(`${ROOT}/.humanify/humanified.js`, "utf8");
const raw = fs.readFileSync(RAW, "utf8");
console.log(`shipped ${shipped.length} B, raw ${raw.length} B`);

const sAst = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const rAst = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });

function bodyStatements(ast: parser.ParseResult<import("@babel/types").File>): any[] {
  const p0: any = ast.program.body[0];
  if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") {
    return p0.expression.body.body;
  }
  if (p0.type === "FunctionDeclaration") return p0.body.body;
  return ast.program.body;
}
const sBody = bodyStatements(sAst);
const rBody = bodyStatements(rAst);
console.log("shipped body statements:", sBody.length, "raw container statements:", rBody.length);

const junkByStmt = new Map<number, { name: string; file: string }[]>();
for (const r of junk) {
  const list = junkByStmt.get(r.stmt) ?? [];
  list.push({ name: r.name, file: r.file });
  junkByStmt.set(r.stmt, list);
}
const junkStmts = [...junkByStmt.keys()].sort((a, b) => a - b);
console.log("junk statements:", junkStmts.length, "(vendor:", junkStmts.filter((k) => junkByStmt.get(k)![0].file.startsWith("vendor")).length, ")");

function literals(node: any): string[] {
  const out: string[] = [];
  const visit = (n: any) => {
    if (!n || typeof n !== "object") return;
    if (Array.isArray(n)) {
      n.forEach(visit);
      return;
    }
    if (n.type === "StringLiteral") out.push(n.value);
    else if (n.type === "TemplateElement") out.push(n.value.cooked ?? "");
    for (const k of Object.keys(n)) {
      if (k === "leadingComments" || k === "trailingComments" || k === "loc" || k === "regex") continue;
      const v = (n as Record<string, unknown>)[k];
      if (v && typeof v === "object") visit(v);
    }
  };
  visit(node);
  return out.sort();
}
const junkNames = new Set(junk.map((r) => r.name));
function junkNamesOf(node: any): string[] {
  const out: string[] = [];
  const visit = (n: any) => {
    if (!n || typeof n !== "object") return;
    if (Array.isArray(n)) {
      n.forEach(visit);
      return;
    }
    if (n.type === "Identifier" && junkNames.has(n.name)) out.push(n.name);
    for (const k of Object.keys(n)) {
      if (k === "leadingComments" || k === "trailingComments" || k === "loc") continue;
      const v = (n as Record<string, unknown>)[k];
      if (v && typeof v === "object") visit(v);
    }
  };
  visit(node);
  return out;
}
function isSubset(small: string[], big: string[]): boolean {
  // multiset subset
  const bag = new Map<string, number>();
  for (const x of big) bag.set(x, (bag.get(x) ?? 0) + 1);
  for (const x of small) {
    const c = bag.get(x) ?? 0;
    if (c === 0) return false;
    bag.set(x, c - 1);
  }
  return true;
}

// raw candidates: every container statement AND every factory declarator
// (init is a call/new whose first arg is a function), in bundle order.
type Cand = { start: number; end: number; kind: "stmt" | "decl"; lits: string[]; names: string[] };
const cands: Cand[] = [];
for (const st of rBody) {
  if (st.type === "VariableDeclaration") {
    for (const d of st.declarations) {
      const init: any = d.init;
      const isFactory =
        init &&
        (init.type === "CallExpression" || init.type === "NewExpression") &&
        init.arguments?.some((a: any) => a.type === "ArrowFunctionExpression" || a.type === "FunctionExpression");
      if (isFactory) {
        cands.push({ start: d.start, end: d.end, kind: "decl", lits: literals(d), names: junkNamesOf(d) });
      }
    }
  }
  cands.push({ start: st.start, end: st.end, kind: "stmt", lits: literals(st), names: junkNamesOf(st) });
}
cands.sort((a, b) => a.start - b.start);
console.log("raw candidates (statements + factory declarators):", cands.length);

// index candidates by literal multiset
const byLits = new Map<string, number[]>();
for (let i = 0; i < cands.length; i++) {
  const key = cands[i].lits.join("\u0000");
  const list = byLits.get(key) ?? [];
  list.push(i);
  byLits.set(key, list);
}

// assignment: junk statements in order; each matched to the candidate with
// an equal literal multiset (junk names a subset), order-constrained to the
// raw span between the previous and next matched statements; fallback:
// literal-multiset CONTAINMENT under the same constraint.
const matches = new Map<number, { cand: number; how: string }>();
const junkIdx = new Map<number, number>(); // k -> index in junkStmts
junkStmts.forEach((k, i) => junkIdx.set(k, i));
let ambiguous = 0;
let unmatched: number[] = [];
for (const k of junkStmts) {
  const node = sBody[k];
  const lits = literals(node).sort();
  const names = [...new Set(junkNamesOf(node))].sort();
  const key = lits.join("\u0000");
  // order constraint from the matched neighbours
  const at = junkIdx.get(k)!;
  let lo = -1;
  let hi = Number.MAX_SAFE_INTEGER;
  for (let j = at - 1; j >= 0; j--) {
    const m = matches.get(junkStmts[j]);
    if (m) {
      lo = cands[m.cand].start;
      break;
    }
  }
  for (let j = at + 1; j < junkStmts.length; j++) {
    const m = matches.get(junkStmts[j]);
    if (m) {
      hi = cands[m.cand].start;
      break;
    }
  }
  const inWindow = (i: number) => cands[i].start > lo && cands[i].start < hi;
  const pick = (hits: number[], how: string) => {
    const ok = hits.filter((i) => inWindow(i) && isSubset(names, cands[i].names));
    if (ok.length === 0) return false;
    const expected = lo < 0 || hi === Number.MAX_SAFE_INTEGER ? (lo + hi) / 2 : (lo + hi) / 2;
    const chosen =
      ok.length === 1
        ? ok[0]
        : ok.reduce((best, i) =>
            Math.abs(cands[i].start - expected) < Math.abs(cands[best].start - expected) ? i : best,
          );
    if (ok.length > 1) ambiguous++;
    matches.set(k, { cand: chosen, how: `${how}${ok.length > 1 ? `-amb${ok.length}` : ""}` });
    return true;
  };
  if (pick(byLits.get(key) ?? [], "exact")) continue;
  // containment fallback: the candidate holds every shipped literal
  const cont: number[] = [];
  for (let i = 0; i < cands.length; i++) {
    if (inWindow(i) && isSubset(lits, cands[i].lits) && isSubset(names, cands[i].names)) cont.push(i);
  }
  if (pick(cont, "contains")) continue;
  unmatched.push(k);
}
console.log(
  `bridge: ${matches.size} matched (${ambiguous} ambiguous, first taken), ${unmatched.length} unmatched`,
  unmatched.slice(0, 12),
);

const claims = ld.claims as { start: number; end: number; package: string; rule: string }[];
const apps = ld.restorationApplications as {
  hash: string;
  package: string;
  start: number;
  end: number;
  slots: number[];
  answer: { single?: { slot: number; name: string }[] };
}[];
const restKinds = new Map<string, string>();
for (const r of ld.restorations as { hash: string; answer: Record<string, unknown> }[]) {
  restKinds.set(r.hash, Object.keys(r.answer).join("+"));
}

const out: any[] = [];
for (const k of junkStmts) {
  const rows = junkByStmt.get(k)!;
  const rec: any = {
    stmt: k,
    junk: rows.length,
    file: rows[0].file,
    pkg: rows[0].file.split("/").slice(1, 2).join("/"),
    junkNames: [...new Set(rows.map((r) => r.name))].slice(0, 10),
  };
  const m = matches.get(k);
  if (!m) {
    rec.unmatched = true;
    out.push(rec);
    continue;
  }
  const c = cands[m.cand];
  rec.raw = m.cand;
  rec.how = m.how;
  rec.kind = c.kind;
  rec.rawSpan = [c.start, c.end];
  rec.rawSpanBytes = c.end - c.start;
  const inside = claims.filter((cl) => cl.start >= c.start && cl.end <= c.end);
  const appsIn = apps.filter((a) => a.start >= c.start && a.end <= c.end);
  rec.claims = inside.length;
  rec.rules = [...new Set(inside.map((cl) => cl.rule))].sort();
  rec.packages = [...new Set(inside.map((cl) => cl.package))].sort();
  rec.apps = appsIn.length;
  let slotsTotal = 0;
  let slotsAnswered = 0;
  let ambiguousApps = 0;
  for (const a of appsIn) {
    const answered = new Set((a.answer.single ?? []).map((e) => e.slot));
    slotsTotal += (a.slots ?? []).length;
    slotsAnswered += answered.size;
    if ((restKinds.get(a.hash) ?? "").startsWith("ambiguous")) ambiguousApps++;
  }
  rec.ambiguousApps = ambiguousApps;
  rec.slotsTotal = slotsTotal;
  rec.slotsAnswered = slotsAnswered;
  rec.slotsUnanswered = slotsTotal - slotsAnswered;
  out.push(rec);
}
fs.writeFileSync(`/work/exp164-mint-census/classified-${LABEL}.json`, JSON.stringify(out, null, 1));

let totJunk = 0;
const byClass = new Map<string, { junk: number; stmts: number }>();
const bump = (c: string, junk: number) => {
  const e = byClass.get(c) ?? { junk: 0, stmts: 0 };
  e.junk += junk;
  e.stmts += 1;
  byClass.set(c, e);
};
for (const r of out) {
  totJunk += r.junk;
  if (r.unmatched) {
    bump("UNMATCHED", r.junk);
    continue;
  }
  let cls: string;
  if (r.apps > 0) {
    cls =
      r.junk <= r.slotsUnanswered
        ? "H1 answered-print (junk <= unanswered slots)"
        : "H1+ answered-print (junk > unanswered slots)";
  } else if (r.claims > 0 && r.rules.includes("seed")) {
    cls = r.rules.length === 1 ? "H2 seeded-no-answer" : "H2+ seeded-with-propagated";
  } else if (r.claims > 0) {
    cls = "H3 propagated-only";
  } else {
    cls = "H4 no-claim";
  }
  bump(cls, r.junk);
}
console.log("\n--- ALL junk bindings by hypothesis ---");
for (const [c, v] of [...byClass.entries()].sort((a, b) => b[1].junk - a[1].junk)) {
  console.log(`${v.junk}\t${c}\t(${v.stmts} statements)`);
}
console.log("total junk:", totJunk);
