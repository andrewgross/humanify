/**
 * exp164 census, anchor-based. Restoration applications are anchored to ERA
 * statement indices via their restored names (a name declared in exactly one
 * era statement anchors its application there; applications carry several
 * names, the majority vote wins). Anchors are dense (4,718 apps) and
 * monotone, so a junk statement's placement group = the group(s) between its
 * bounding anchors' raw spans, disambiguated by linear era interpolation.
 * Classification per junk statement (group = raw span):
 *   H1  answered print, junk <= unanswered slots   (plumbing/join gap)
 *   H1+ answered print, junk > unanswered slots    (residual)
 *   H2  seed claim, no application                  (queried, no answer)
 *   H3  propagated claim only, no application       (never captured)
 *   H4  no claims at all in the group               (moved content-only)
 */
import * as fs from "node:fs";
import * as parser from "@babel/parser";
import traverseMod from "@babel/traverse";

const LABEL = "exp161-names-hop";
const ROOT = `/tmp/eval-work/${LABEL}/2.1.216`;
const diag = JSON.parse(fs.readFileSync(`/tmp/eval-work/${LABEL}/2.1.216.diag.json`, "utf8"));
const ld = diag.libraryDiscovery;
const junk: { name: string; start: number; end: number; stmt: number; file: string }[] = JSON.parse(
  fs.readFileSync(`/work/exp164-mint-census/shipped-located-${LABEL}.json`, "utf8"),
);
const shipped = fs.readFileSync(`${ROOT}/.humanify/humanified.js`, "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });

// era statements
const sBody: any[] = (() => {
  const p0: any = ast.program.body[0];
  if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") return p0.expression.body.body;
  if (p0.type === "FunctionDeclaration") return p0.body.body;
  return ast.program.body;
})();
const stmtStarts = sBody.map((s) => s.start);
function stmtOf(pos: number): number {
  // last statement whose start <= pos (top-level statements are sequential)
  let lo = 0;
  let hi = stmtStarts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (stmtStarts[mid] <= pos) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

// declared names per era statement (binding declaration ids only)
const declaredAt = new Map<number, Set<string>>();
traverseMod.default(ast, {
  Scopable: {
    enter: (p: any) => {
      for (const [name, binding] of Object.entries(p.scope.bindings as Record<string, any>)) {
        const st = stmtOf(binding.identifier.start);
        const set = declaredAt.get(st) ?? new Set<string>();
        set.add(name);
        declaredAt.set(st, set);
      }
    },
  },
});
console.log("era statements with declared names:", declaredAt.size);

// anchor applications to era statements
type App = { hash: string; package: string; start: number; end: number; slots: number[]; answer: { single?: { slot: number; name: string }[] } };
const apps = ld.restorationApplications as App[];
// name -> statements declaring it
const declStmts = new Map<string, number[]>();
for (const [st, names] of declaredAt) {
  for (const n of names) {
    const l = declStmts.get(n) ?? [];
    l.push(st);
    declStmts.set(n, l);
  }
}
type Anchor = { app: App; st: number; votes: number; namesTotal: number };
const anchors: Anchor[] = [];
let unanchored = 0;
for (const a of apps) {
  const names = (a.answer.single ?? []).map((e) => e.name);
  if (names.length === 0) {
    unanchored++;
    continue;
  }
  const votes = new Map<number, number>();
  let ok = true;
  for (const n of names) {
    const sts = declStmts.get(n) ?? [];
    if (sts.length === 0) continue; // name not declared anywhere (used only)
    if (sts.length > 1) {
      ok = false;
      break;
    }
    votes.set(sts[0], (votes.get(sts[0]) ?? 0) + 1);
  }
  if (!ok || votes.size === 0) {
    unanchored++;
    continue;
  }
  const [best] = [...votes.entries()].sort((x, y) => y[1] - x[1]);
  anchors.push({ app: a, st: best[0], votes: best[1], namesTotal: names.length });
}
console.log(`anchors: ${anchors.length} of ${apps.length} applications (${unanchored} unanchored)`);
// monotonicity: sort by era statement, check raw start ordering
anchors.sort((x, y) => x.st - y.st);
let viol = 0;
for (let i = 1; i < anchors.length; i++) if (anchors[i].app.start < anchors[i - 1].app.start) viol++;
console.log("raw-start monotonicity violations among era-sorted anchors:", viol);

// groups (raw spans)
type Group = { i: number; start: number; end: number; stmts: { start: number; end: number; package?: string }[] };
const groups: Group[] = (ld.placementGroups as any[]).map((g, i) => ({
  i,
  start: g.statements[0].start,
  end: g.statements[g.statements.length - 1].end,
  stmts: g.statements,
}));
groups.sort((a, b) => a.start - b.start);
const claims = ld.claims as { start: number; end: number; package: string; rule: string; round: number }[];
const restKinds = new Map<string, string>();
for (const r of ld.restorations as { hash: string; answer: Record<string, unknown> }[]) {
  restKinds.set(r.hash, Object.keys(r.answer).join("+"));
}

const junkByStmt = new Map<number, { name: string; file: string }[]>();
for (const r of junk) {
  const list = junkByStmt.get(r.stmt) ?? [];
  list.push({ name: r.name, file: r.file });
  junkByStmt.set(r.stmt, list);
}
const junkStmts = [...junkByStmt.keys()].sort((a, b) => a - b);

// region lookup for a junk statement between anchors
const anchorOf = new Map<number, Anchor>();
for (const a of anchors) if (!anchorOf.has(a.st)) anchorOf.set(a.st, a);

function regionFor(k: number): { group?: Group; how: string } {
  const direct = anchorOf.get(k);
  if (direct) {
    const g = groups.find((x) => x.start <= direct.app.start && direct.app.end <= x.end);
    if (g) return { group: g, how: "direct-anchor" };
  }
  let prev: Anchor | undefined;
  let next: Anchor | undefined;
  let lo = 0;
  let hi = anchors.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (anchors[mid].st < k) {
      prev = anchors[mid];
      lo = mid + 1;
    } else {
      next = anchors[mid];
      hi = mid - 1;
    }
  }
  const aEnd = prev ? prev.app.end : -1;
  const bStart = next ? next.app.start : Number.MAX_SAFE_INTEGER;
  const cands = groups.filter((g) => g.start < bStart && g.end > aEnd);
  if (cands.length === 0) return { how: "no-group-in-window" };
  if (cands.length === 1) return { group: cands[0], how: prev && next ? "between" : "one-sided" };
  if (!prev || !next) return { group: cands[0], how: "one-sided-ambiguous" };
  const ea = prev.st;
  const eb = next.st;
  const ra = prev.app.end;
  const rb = next.app.start;
  const est = ra + ((k - ea) / (eb - ea)) * (rb - ra);
  const group = cands.reduce((best, g) => {
    const c = (g.start + g.end) / 2;
    const cb = (best.start + best.end) / 2;
    return Math.abs(c - est) < Math.abs(cb - est) ? g : best;
  });
  return { group, how: "interpolated" };
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
  const { group, how } = regionFor(k);
  rec.regionHow = how;
  if (!group) {
    rec.noRegion = true;
    out.push(rec);
    continue;
  }
  rec.group = group.i;
  rec.groupSpan = [group.start, group.end];
  rec.groupStmts = group.stmts.length;
  const inside = claims.filter((cl) => cl.start >= group.start && cl.end <= group.end);
  const appsIn = apps.filter((a) => a.start >= group.start && a.end <= group.end);
  rec.claims = inside.length;
  rec.rules = [...new Set(inside.map((cl) => cl.rule))].sort();
  rec.packages = [...new Set(inside.map((cl) => cl.package))].sort();
  rec.apps = appsIn.length;
  let slotsTotal = 0;
  let slotsAnswered = 0;
  let ambiguousApps = 0;
  for (const a of appsIn) {
    slotsTotal += (a.slots ?? []).length;
    slotsAnswered += (a.answer.single ?? []).length;
    if ((restKinds.get(a.hash) ?? "").startsWith("ambiguous")) ambiguousApps++;
  }
  rec.ambiguousApps = ambiguousApps;
  rec.slotsTotal = slotsTotal;
  rec.slotsAnswered = slotsAnswered;
  rec.slotsUnanswered = slotsTotal - slotsAnswered;
  out.push(rec);
}
fs.writeFileSync(`/work/exp164-mint-census/classified2-${LABEL}.json`, JSON.stringify(out, null, 1));

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
  byHow.set(r.regionHow, (byHow.get(r.regionHow) ?? 0) + r.junk);
  if (r.noRegion) {
    bump("NO-REGION", r.junk);
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
console.log("\n--- region identification ---");
for (const [h, c] of [...byHow.entries()].sort((a, b) => b[1] - a[1])) console.log(`${c}\t${h}`);
console.log("\n--- ALL junk bindings by hypothesis ---");
for (const [c, v] of [...byClass.entries()].sort((a, b) => b[1].junk - a[1].junk)) {
  console.log(`${v.junk}\t${c}\t(${v.stmts} statements)`);
}
console.log("total junk:", totJunk);
