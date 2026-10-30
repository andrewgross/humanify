/**
 * exp164 census v4: era<->raw statement anchors from the restoration
 * applications themselves. An application's raw span identifies its raw
 * container statement exactly; its restored names (unique-declared majority
 * vote) identify its era statement. Anchors are dense, monotone (verified),
 * so a junk statement's raw statement = interpolation between its bounding
 * anchors. Classification per junk statement, from the raw statement's own
 * claims and applications:
 *   H1  statement's print answered, junk <= unanswered slots
 *   H1+ statement's print answered, junk > unanswered slots
 *   H2  seed claim, no application  (queried, index had no answer)
 *   H3  propagated claim only       (never captured by design)
 *   H4  no claim                    (moved on content evidence alone)
 */
import * as fs from "node:fs";
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
const shipped = fs.readFileSync(`${ROOT}/.humanify/humanified.js`, "utf8");
const raw = fs.readFileSync(RAW, "utf8");

const sAst = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const rAst = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
function bodyStatements(ast: any): any[] {
  const p0 = ast.program.body[0];
  if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") return p0.expression.body.body;
  if (p0.type === "FunctionDeclaration") return p0.body.body;
  return ast.program.body;
}
const sBody = bodyStatements(sAst);
const rBody = bodyStatements(rAst);
const stmtStarts = sBody.map((s) => s.start);
function stmtOf(pos: number): number {
  let lo = 0;
  let hi = stmtStarts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (stmtStarts[mid] <= pos) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

// era statements declaring each name
const declStmts = new Map<string, number[]>();
traverseMod.default(sAst, {
  Scopable: {
    enter: (p: { scope: { bindings: Record<string, { identifier: { start: number } }> } }) => {
      for (const [name, binding] of Object.entries(p.scope.bindings)) {
        const st = stmtOf(binding.identifier.start);
        const l = declStmts.get(name) ?? [];
        l.push(st);
        declStmts.set(name, l);
      }
    },
  },
});
// raw statements by span
const rawByStart = new Map<number, number>();
rBody.forEach((s, i) => rawByStart.set(s.start, i));
function rawStmtOf(a: number, b: number): number {
  const i = rawByStart.get(a);
  if (i !== undefined && rBody[i].end === b) return i;
  // containment fallback
  let best = -1;
  for (let j = 0; j < rBody.length; j++) {
    if (rBody[j].start <= a && rBody[j].end >= b) {
      if (best < 0 || rBody[j].end - rBody[j].start < rBody[best].end - rBody[best].start) best = j;
    }
  }
  return best;
}

type App = { hash: string; package: string; start: number; end: number; slots: number[]; answer: { single?: { slot: number; name: string }[] } };
const apps = ld.restorationApplications as App[];
type Anchor = { es: number; rs: number; app: App };
const anchors: Anchor[] = [];
let noNames = 0;
let noRaw = 0;
let ambiguousName = 0;
for (const a of apps) {
  const names = (a.answer.single ?? []).map((e) => e.name);
  if (names.length === 0) {
    noNames++;
    continue;
  }
  const votes = new Map<number, number>();
  for (const n of names) {
    const sts = declStmts.get(n) ?? [];
    if (sts.length === 0 || sts.length > 5) continue;
    for (const st of sts) votes.set(st, (votes.get(st) ?? 0) + 1);
  }
  if (votes.size === 0) {
    ambiguousName++;
    continue;
  }
  const [best] = [...votes.entries()].sort((x, y) => y[1] - x[1]);
  const rs = rawStmtOf(a.start, a.end);
  if (rs < 0) {
    noRaw++;
    continue;
  }
  anchors.push({ es: best[0], rs, app: a });
}
console.log(
  `anchors: ${anchors.length} / ${apps.length} apps (noNames ${noNames}, all-names-ambiguous ${ambiguousName}, noRawStmt ${noRaw})`,
);

// enforce monotonicity: sort by raw statement; keep the longest run with
// non-decreasing era statements (simple greedy over sorted-by-raw anchors)
anchors.sort((a, b) => a.rs - b.rs || a.es - b.es);
const kept: Anchor[] = [];
let lastEs = -1;
let lastRs = -1;
for (const a of anchors) {
  if (a.es >= lastEs && a.rs > lastRs) {
    kept.push(a);
    lastEs = a.es;
    lastRs = a.rs;
  }
}
console.log(`monotone anchors kept: ${kept.length} / ${anchors.length}`);
const byEs = new Map<number, Anchor[]>();
for (const a of kept) {
  const l = byEs.get(a.es) ?? [];
  l.push(a);
  byEs.set(a.es, l);
}

const claims = ld.claims as { start: number; end: number; package: string; rule: string; round: number }[];
const restKinds = new Map<string, string>();
for (const r of ld.restorations as { hash: string; answer: Record<string, unknown> }[]) {
  restKinds.set(r.hash, Object.keys(r.answer).join("+"));
}
const claimBySpan = new Map<string, typeof claims>();
for (const c of claims) {
  const key = `${c.start}:${c.end}`;
  const l = claimBySpan.get(key) ?? [];
  l.push(c);
  claimBySpan.set(key, l);
}
const appBySpan = new Map<string, App[]>();
for (const a of apps) {
  const key = `${a.start}:${a.end}`;
  const l = appBySpan.get(key) ?? [];
  l.push(a);
  appBySpan.set(key, l);
}

const junkByStmt = new Map<number, { name: string; file: string }[]>();
for (const r of junk) {
  const list = junkByStmt.get(r.stmt) ?? [];
  list.push({ name: r.name, file: r.file });
  junkByStmt.set(r.stmt, list);
}
const junkStmts = [...junkByStmt.keys()].sort((a, b) => a - b);

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
  const direct = byEs.get(k);
  let rs: number;
  let how: string;
  if (direct) {
    rs = direct[0].rs;
    how = "direct-anchor";
  } else {
    // bounding anchors by era statement
    let prev: Anchor | undefined;
    let next: Anchor | undefined;
    const ess = [...byEs.keys()].sort((a, b) => a - b);
    let lo = 0;
    let hi = ess.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (ess[mid] < k) {
        prev = byEs.get(ess[mid])![0];
        lo = mid + 1;
      } else {
        next = byEs.get(ess[mid])![0];
        hi = mid - 1;
      }
    }
    if (!prev || !next) {
      rec.unanchored = true;
      out.push(rec);
      continue;
    }
    const est = prev.rs + ((k - prev.es) / (next.es - prev.es)) * (next.rs - prev.rs);
    rs = Math.round(est);
    how = `interp(${prev.es}->${prev.rs}, ${next.es}->${next.rs})`;
  }
  const rstmt = rBody[rs];
  rec.rs = rs;
  rec.rawSpan = [rstmt.start, rstmt.end];
  rec.how = how;
  const cls = claimBySpan.get(`${rstmt.start}:${rstmt.end}`) ?? [];
  const apl = appBySpan.get(`${rstmt.start}:${rstmt.end}`) ?? [];
  rec.claims = cls.length;
  rec.rules = [...new Set(cls.map((c) => c.rule))].sort();
  rec.packages = [...new Set(cls.map((c) => c.package))].sort();
  rec.apps = apl.length;
  let slotsTotal = 0;
  let slotsAnswered = 0;
  for (const a of apl) {
    slotsTotal += (a.slots ?? []).length;
    slotsAnswered += (a.answer.single ?? []).length;
  }
  rec.slotsTotal = slotsTotal;
  rec.slotsAnswered = slotsAnswered;
  rec.slotsUnanswered = slotsTotal - slotsAnswered;
  out.push(rec);
}
fs.writeFileSync(`/work/exp164-mint-census/classified4-${LABEL}.json`, JSON.stringify(out, null, 1));

let totJunk = 0;
const byClass = new Map<string, { junk: number; stmts: number }>();
const bump = (c: string, junk: number) => {
  const e = byClass.get(c) ?? { junk: 0, stmts: 0 };
  e.junk += junk;
  e.stmts += 1;
  byClass.set(c, e);
};
const byHow = new Map<string, number>();
for (const r of out) {
  totJunk += r.junk;
  if (r.unanchored) {
    bump("UNANCHORED", r.junk);
    continue;
  }
  byHow.set(r.how.split("(")[0], (byHow.get(r.how.split("(")[0]) ?? 0) + r.junk);
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
console.log("\n--- identification method ---");
for (const [h, c] of [...byHow.entries()].sort((a, b) => b[1] - a[1])) console.log(`${c}\t${h}`);
console.log("\n--- ALL junk bindings by hypothesis ---");
for (const [c, v] of [...byClass.entries()].sort((a, b) => b[1].junk - a[1].junk)) {
  console.log(`${v.junk}\t${c}\t(${v.stmts} statements)`);
}
console.log("total junk:", totJunk);
