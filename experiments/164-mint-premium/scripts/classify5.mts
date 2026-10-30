/**
 * exp164 census v5 (final): junk-holding vendor file -> raw factory
 * (literal-occurrence dominance) -> factory body statements; the file's
 * statements align to the body statements by literal containment + app
 * anchors (an application's restored names are declared in its era
 * statement); the junk statement's raw body statement then classifies it:
 *   H1  its print answered, junk <= unanswered slots   (answer lacked slots)
 *   H1+ its print answered, junk >  unanswered slots   (residual)
 *   H2  seed claim, no application                      (queried, no answer)
 *   H3  propagated claim only                           (never captured)
 *   H4  no claim                                        (moved content-only)
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
const raw = fs.readFileSync(RAW, "utf8");
const rAst = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rBody: any[] = (() => {
  const p0: any = rAst.program.body[0];
  if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") return p0.expression.body.body;
  if (p0.type === "FunctionDeclaration") return p0.body.body;
  return rAst.program.body;
})();

// container statement -> factory declarators (span + body statement spans)
const containerStarts = rBody.map((s) => s.start);
function containerOf(pos: number): number {
  let lo = 0;
  let hi = containerStarts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (containerStarts[mid] <= pos) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}
type Factory = { declStart: number; declEnd: number; bodyStart: number; bodyEnd: number; body: { start: number; end: number }[] };
const factoriesOf = new Map<number, Factory[]>();
for (let i = 0; i < rBody.length; i++) {
  const st: any = rBody[i];
  if (st.type !== "VariableDeclaration") continue;
  const list: Factory[] = [];
  for (const d of st.declarations) {
    const init: any = d.init;
    const isFactory =
      init &&
      (init.type === "CallExpression" || init.type === "NewExpression") &&
      init.arguments?.some((a: any) => a.type === "ArrowFunctionExpression" || a.type === "FunctionExpression");
    if (!isFactory) continue;
    const fn = init.arguments.find((a: any) => a.type === "ArrowFunctionExpression" || a.type === "FunctionExpression");
    list.push({
      declStart: d.start,
      declEnd: d.end,
      bodyStart: fn.body.start,
      bodyEnd: fn.body.end,
      body: fn.body.body.map((b: any) => ({ start: b.start, end: b.end })),
    });
  }
  if (list.length > 0) factoriesOf.set(i, list);
}
console.log("containers with factories:", factoriesOf.size);

// raw literal positions -> for occurrence lookup
const rLitPos: { pos: number; v: string }[] = [];
traverseMod.default(rAst, {
  StringLiteral: (p: { node: { start: number; value: string } }) => { if (p.node.start != null) rLitPos.push({ pos: p.node.start, v: p.node.value }); },
  TemplateElement: (p: { node: { start: number; value: { cooked: string | null } } }) => { if (p.node.start != null) rLitPos.push({ pos: p.node.start, v: p.node.value.cooked ?? "" }); },
});
rLitPos.sort((a, b) => a.pos - b.pos);
const litPositions = rLitPos.map((x) => x.pos);
function litsIn(a: number, b: number): Map<string, number> {
  let lo = 0;
  let hi = rLitPos.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (litPositions[mid] < a) lo = mid + 1;
    else hi = mid;
  }
  const out = new Map<string, number>();
  for (let i = lo; i < rLitPos.length && litPositions[i] < b; i++) out.set(rLitPos[i].v, (out.get(rLitPos[i].v) ?? 0) + 1);
  return out;
}

const claims = ld.claims as { start: number; end: number; package: string; rule: string; round: number }[];
type App = { hash: string; package: string; start: number; end: number; slots: number[]; answer: { single?: { slot: number; name: string }[] } };
const apps = ld.restorationApplications as App[];

const junkByFile = new Map<string, Map<number, string[]>>();
for (const r of junk) {
  if (!r.file.startsWith("vendor")) continue;
  let m = junkByFile.get(r.file);
  if (!m) {
    m = new Map();
    junkByFile.set(r.file, m);
  }
  const l = m.get(r.stmt) ?? [];
  l.push(r.name);
  m.set(r.stmt, l);
}

const out: any[] = [];
const notes: string[] = [];
for (const [file, stmts] of junkByFile) {
  const text = fs.readFileSync(`${ROOT}/${file}`, "utf8");
  const fAst = parser.parse(text, { sourceType: "unambiguous", errorRecovery: true });
  const fBody: any[] = (() => {
    const p0: any = fAst.program.body[0];
    if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") return p0.expression.body.body;
    if (p0.type === "FunctionDeclaration") return p0.body.body;
    return fAst.program.body;
  })();
  // file statement -> declared names
  const fDecl: Set<string>[] = fBody.map(() => new Set<string>());
  const fStarts = fBody.map((s) => s.start);
  const fStmtOf = (pos: number): number => {
    let lo = 0;
    let hi = fStarts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (fStarts[mid] <= pos) lo = mid;
      else hi = mid - 1;
    }
    return lo;
  };
  traverseMod.default(fAst, {
    Scopable: {
      enter: (p: { scope: { bindings: Record<string, { identifier: { start: number } }> } }) => {
        for (const [name, binding] of Object.entries(p.scope.bindings)) fDecl[fStmtOf(binding.identifier.start)].add(name);
      },
    },
  });
  // file literals
  const fLits = new Map<string, number>();
  traverseMod.default(fAst, {
    StringLiteral: (p: { node: { value: string } }) => { fLits.set(p.node.value, (fLits.get(p.node.value) ?? 0) + 1); },
    TemplateElement: (p: { node: { value: { cooked: string | null } } }) => { const v = p.node.value.cooked ?? ""; fLits.set(v, (fLits.get(v) ?? 0) + 1); },
  });

  // file -> factory by literal occurrence dominance
  const tally = new Map<string, { container: number; factory: Factory; hits: number }>();
  let totalHits = 0;
  for (const [v] of fLits) {
    if (v.length < 4) continue;
    const needle = v.includes("\\") || v.includes('"') ? null : v;
    if (needle === null) continue;
    let at = raw.indexOf(needle);
    let n = 0;
    while (at >= 0 && n < 40) {
      const ci = containerOf(at);
      for (const f of factoriesOf.get(ci) ?? []) {
        if (at >= f.declStart && at < f.declEnd) {
          const key = `${ci}@${f.declStart}`;
          const e = tally.get(key) ?? { container: ci, factory: f, hits: 0 };
          e.hits++;
          tally.set(key, e);
          totalHits++;
          break;
        }
      }
      at = raw.indexOf(needle, at + 1);
      n++;
    }
  }
  const ranked = [...tally.values()].sort((a, b) => b.hits - a.hits);
  const best = ranked[0];
  if (!best || totalHits < 3 || best.hits < totalHits * 0.4) {
    notes.push(`${file}: no dominant factory (hits ${totalHits}, best ${best?.hits ?? 0})`);
    for (const [k, names] of stmts) out.push({ stmt: k, junk: names.length, file, pkg: file.split("/").slice(1, 2).join("/"), unmatched: true });
    continue;
  }
  const factory = best.factory;

  // claims/apps mapped to body statements
  const bodyClaims: Map<number, typeof claims> = new Map();
  for (const c of claims) {
    if (c.start < factory.bodyStart || c.end > factory.bodyEnd) continue;
    let bi = factory.body.findIndex((b) => b.start <= c.start && c.end <= b.end);
    if (bi < 0) continue;
    const l = bodyClaims.get(bi) ?? [];
    l.push(c);
    bodyClaims.set(bi, l);
  }
  const bodyApps: Map<number, App[]> = new Map();
  for (const a of apps) {
    if (a.start < factory.bodyStart || a.end > factory.bodyEnd) continue;
    let bi = factory.body.findIndex((b) => b.start <= a.start && a.end <= b.end);
    if (bi < 0) continue;
    const l = bodyApps.get(bi) ?? [];
    l.push(a);
    bodyApps.set(bi, l);
  }

  // alignment: file statement -> body statement index
  // anchors 1: literal containment; 2: app answer names declared in the statement
  const bodyLitCache = new Map<number, Map<string, number>>();
  const bodyLits = (bi: number) => {
    let m = bodyLitCache.get(bi);
    if (!m) {
      const b = factory.body[bi];
      m = litsIn(b.start, b.end);
      bodyLitCache.set(bi, m);
    }
    return m;
  };
  const anchorOf = new Map<number, number>(); // file stmt -> body stmt
  for (let fi = 0; fi < fBody.length; fi++) {
    const fl = litsIn(fBody[fi].start, fBody[fi].end);
    if (fl.size === 0) continue;
    const hits: number[] = [];
    for (let bi = 0; bi < factory.body.length; bi++) {
      const bl = bodyLits(bi);
      let ok = true;
      for (const [v, c] of fl) if ((bl.get(v) ?? 0) < c) {
        ok = false;
        break;
      }
      if (ok) hits.push(bi);
    }
    if (hits.length === 1) anchorOf.set(fi, hits[0]);
  }
  for (const [bi, al] of bodyApps) {
    const names = new Set<string>();
    for (const a of al) for (const e of a.answer.single ?? []) names.add(e.name);
    if (names.size === 0) continue;
    for (let fi = 0; fi < fBody.length; fi++) {
      let v = 0;
      for (const n of names) if (fDecl[fi].has(n)) v++;
      if (v >= Math.max(1, Math.ceil(names.size / 2))) {
        if (!anchorOf.has(fi)) anchorOf.set(fi, bi);
        break;
      }
    }
  }
  // monotone interpolation between anchors
  const fStmtToBody = (fi: number): { bi: number; how: string } => {
    const direct = anchorOf.get(fi);
    if (direct !== undefined) return { bi: direct, how: "anchor" };
    let prevF = -1;
    let prevB = -1;
    let nextF = -1;
    let nextB = -1;
    for (const [f, b] of [...anchorOf.entries()].sort((a, b) => a[0] - b[0])) {
      if (f < fi) {
        prevF = f;
        prevB = b;
      } else if (f > fi) {
        nextF = f;
        nextB = b;
        break;
      }
    }
    if (prevF < 0 && nextF < 0) return { bi: -1, how: "no-anchor" };
    if (prevF < 0) return { bi: nextB, how: "edge-next" };
    if (nextF < 0) return { bi: prevB, how: "edge-prev" };
    const bi = Math.round(prevB + ((fi - prevF) / (nextF - prevF)) * (nextB - prevB));
    return { bi: Math.min(nextB, Math.max(prevB, bi)), how: "interp" };
  };

  for (const [k, names] of stmts) {
    const rec: any = {
      stmt: k,
      junk: names.length,
      file,
      pkg: file.split("/").slice(1, 2).join("/"),
      junkNames: [...new Set(names)].slice(0, 10),
      factory: `${best.container}@${best.factory.declStart}`,
      hits: best.hits,
    };
    // the file statement holding era statement k: find by index — the junk
    // row's stmt is the ERA statement index; within the file, match by the
    // junk names declared
    let fi = fBody.findIndex((_, i) => {
      for (const n of names) if (fDecl[i].has(n)) return true;
      return false;
    });
    if (fi < 0) {
      rec.alignFailed = "no-file-stmt";
      out.push(rec);
      continue;
    }
    const { bi, how } = fStmtToBody(fi);
    rec.fileStmt = fi;
    rec.bodyStmt = bi;
    rec.align = how;
    if (bi < 0) {
      rec.unaligned = true;
      out.push(rec);
      continue;
    }
    const apl = bodyApps.get(bi) ?? [];
    const cls = bodyClaims.get(bi) ?? [];
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
}
fs.writeFileSync(`/work/exp164-mint-census/classified5-${LABEL}.json`, JSON.stringify(out, null, 1));
console.log(notes.join("\n"));

let totJunk = 0;
const byClass = new Map<string, { junk: number; stmts: number }>();
const bump = (c: string, junk: number) => {
  const e = byClass.get(c) ?? { junk: 0, stmts: 0 };
  e.junk += junk;
  e.stmts += 1;
  byClass.set(c, e);
};
const byAlign = new Map<string, number>();
for (const r of out) {
  totJunk += r.junk;
  if (r.unmatched || r.alignFailed || r.unaligned) {
    bump(r.unmatched ? "NO-FACTORY" : r.alignFailed ? "NO-FILE-STMT" : "UNALIGNED", r.junk);
    continue;
  }
  byAlign.set(r.align, (byAlign.get(r.align) ?? 0) + r.junk);
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
console.log("\n--- alignment method ---");
for (const [h, c] of [...byAlign.entries()].sort((a, b) => b[1] - a[1])) console.log(`${c}\t${h}`);
console.log("\n--- ALL vendor junk bindings by hypothesis ---");
for (const [c, v] of [...byClass.entries()].sort((a, b) => b[1].junk - a[1].junk)) {
  console.log(`${v.junk}\t${c}\t(${v.stmts} statements)`);
}
console.log("total vendor junk:", totJunk);
