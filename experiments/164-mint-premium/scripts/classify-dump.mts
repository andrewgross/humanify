/**
 * exp164 census, final: join the REAL names-mode hop's junk bindings
 * (shipped-located rows: name + shipped statement ordinal) against the
 * dump run's eraRestoration block — per-statement site rows, the freeze's
 * bindings, the converted claimed regions (with raw spans) — and the real
 * diag's claims (rules), attributing every junk binding to one hypothesis.
 *
 * Buckets (a binding's first match wins, most specific first):
 *   anomaly     answered AND applied yet still junk (should not exist)
 *   H1-frozen   answer covered the slot, skipped "renamed", binding frozen
 *   H1-transfer answer covered the slot, skipped "renamed", not frozen
 *   H1-conflict two prints named it differently
 *   H1-failed   the validated path refused the rename
 *   H1-data     the print is captured but its answer does not name the slot
 *   ambiguous   the print's captured answer is ambiguous (policy abstain)
 *   H2-join     the statement sits in a diag placement group the unpack
 *               could not convert (the group was dropped whole)
 *   H2-capture  the statement is claimed (seed+index) but no print hash
 *               of it is in the captured answers
 *   H3          the statement is claimed only by propagated rules
 *   H4          the statement is claimed by nothing
 * argv: <diagPath> (the dump run's diagnostics)
 */
import * as fs from "node:fs";

const LABEL = "exp161-names-hop";
const dumpDiag = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const realDiag = JSON.parse(fs.readFileSync(`/tmp/eval-work/${LABEL}/2.1.216.diag.json`, "utf8"));
const junk: { name: string; start: number; end: number; stmt: number; file: string }[] = JSON.parse(
  fs.readFileSync(`/work/exp164-mint-census/shipped-located-${LABEL}.json`, "utf8"),
);

const er = dumpDiag.eraRestoration as {
  bodyLen: number;
  bodySpans: [number, number][];
  frozen: [number, number, string][];
  regions: { statements: number[]; rawSpans: [number, number][]; claims: (string | null)[] }[];
  sites: {
    statement: number;
    prints: {
      hash: string;
      slots: [string, number | null, string][];
      captured: boolean;
      ambiguous: boolean;
      answered: [number, string][];
    }[];
    applied: [number, string][];
    skipped: [number, string, string][];
  }[];
  permuteMoves: number;
};
console.log(`bodyLen ${er.bodyLen} (expected 35903), permuteMoves ${er.permuteMoves}`);
console.log(`frozen bindings ${er.frozen.length}, regions ${er.regions.length}, sites ${er.sites.length}`);

const frozenByStmt = new Map<number, Set<string>>();
for (const [stmt, , name] of er.frozen) {
  const set = frozenByStmt.get(stmt) ?? new Set();
  set.add(name);
  frozenByStmt.set(stmt, set);
}
const sites = new Map(er.sites.map((s) => [s.statement, s]));
const regionByStmt = new Map<number, (typeof er.regions)[number]>();
for (const r of er.regions) for (const s of r.statements) regionByStmt.set(s, r);

// A diag placement group the unpack dropped whole (its spans did not join
// the container): every group NOT represented by a converted region.
const groups = realDiag.libraryDiscovery.placementGroups as {
  statements: { start: number; end: number }[];
}[];
// the NEW dump serializes camelCase (rawSpans); the old census dump
// (the exp161-era binary) was snake_case — read both.
const raw = (r: { raw_spans?: [number, number][]; rawSpans?: [number, number][] }) => r.raw_spans ?? r.rawSpans!;
const regionRaw = new Set(er.regions.flatMap((r) => raw(r).map(([s, e]) => `${s}:${e}`)));
const dropped = groups.filter(
  (g) => !g.statements.some((st) => regionRaw.has(`${st.start}:${st.end}`)),
);
console.log(`diag groups ${groups.length}, dropped by the join ${dropped.length}`);

// The real diag's claims: raw spans -> the rules that claimed them.
const claims = realDiag.libraryDiscovery.claims as {
  start: number;
  end: number;
  package: string;
  rule: string;
}[];
const claimRules = new Map<string, Set<string>>();
for (const c of claims) {
  const key = `${c.start}:${c.end}`;
  const set = claimRules.get(key) ?? new Set();
  set.add(c.rule);
  claimRules.set(key, set);
}
function rulesOf(region: (typeof er.regions)[number] | undefined): string[] {
  if (!region) return [];
  const out = new Set<string>();
  for (const [s, e] of raw(region)) for (const r of claimRules.get(`${s}:${e}`) ?? []) out.add(r);
  return [...out];
}

const multiName = { rows: 0 };
function slotOf(site: (typeof er.sites)[number], name: string): number[] {
  const out: number[] = [];
  for (const p of site.prints) {
    p.slots.forEach(([_, __, nm], i) => {
      if (nm === name) out.push(i);
    });
  }
  return out;
}

const buckets = new Map<string, { junk: number; stmts: number; byPkg: Map<string, number> }>();
const examples = new Map<string, string[]>();
const bump = (cls: string, junk: number, pkg: string, example?: string) => {
  const e = buckets.get(cls) ?? { junk: 0, stmts: 0, byPkg: new Map() };
  e.junk += junk;
  e.stmts += 1;
  e.byPkg.set(pkg, (e.byPkg.get(pkg) ?? 0) + junk);
  buckets.set(cls, e);
  if (example) {
    const list = examples.get(cls) ?? [];
    if (list.length < 6) list.push(example);
    examples.set(cls, list);
  }
};

const rows = junk.map((r) => {
  const pkg = r.file.split("/").slice(1, 2).join("/") || r.file;
  const site = sites.get(r.stmt);
  const region = regionByStmt.get(r.stmt);
  const isFrozen = (frozenByStmt.get(r.stmt) ?? new Set()).has(r.name);
  let cls: string;
  let detail = "";
  if (!site) {
    // The statement was never visited by Site A: claimed only outside the
    // converted regions (or not claimed at all — rulesOf reads the raw
    // spans, which the dump has only for converted regions).
    if (region) cls = "anomaly-region-no-site";
    else cls = rulesOf(region).length === 0 ? "H4-no-claim" : "anomaly-claim-no-site";
  } else {
    // Per print, the binding's slot: the mapping spells every slot's
    // name; the junk binding keeps its era-time name.
    type Verdict = { cls: string; want: boolean; cause?: string };
    let verdict: Verdict | undefined;
    let capturedPrints = 0;
    let nameMatches = 0;
    for (const p of site.prints) {
      const idx = p.slots.findIndex(([, , nm]) => nm === r.name);
      if (idx < 0) continue;
      nameMatches++;
      if (!p.captured) continue;
      capturedPrints++;
      const symbol = p.slots[idx][1];
      if (p.ambiguous) {
        verdict ??= { cls: "ambiguous", want: false };
        continue;
      }
      if (p.answered.some(([s]) => s === idx)) {
        const appliedSym = site.applied.find(([sym]) => sym === symbol);
        const skipped = site.skipped.find(([sym]) => sym === symbol);
        if (appliedSym) verdict = { cls: "anomaly-applied", want: true };
        else if (skipped)
          verdict = {
            cls:
              skipped[2] === "renamed"
                ? isFrozen
                  ? "H1-frozen"
                  : "H1-transfer"
                : skipped[2] === "conflict"
                  ? "H1-conflict"
                  : "H1-failed",
            want: true,
            cause: skipped[2],
          };
        else verdict = { cls: "anomaly-answered-unapplied", want: true };
        break; // the most specific reading wins
      }
      verdict ??= { cls: "H1-data", want: false };
    }
    cls = verdict?.cls ?? (capturedPrints > 0 ? "anomaly-captured-no-name" : "H2-capture");
    if (nameMatches > 1) multiName.rows++;
    if (cls === "H2-capture") {
      // Claimed (a converted region) but no print of the statement is in
      // the captured answers: propagated-only claims are H3 (the capture
      // queries Seed+Index rows only).
      const rules = rulesOf(region);
      cls = rules.length === 0 ? "H4-no-claim" : rules.every((x) => x !== "seed") ? "H3-propagated" : "H2-capture";
    }
    detail = site.prints
      .filter((p) => p.captured)
      .slice(0, 2)
      .map((p) => `${p.hash.slice(0, 8)}${p.ambiguous ? " amb" : ""}`)
      .join(",");
  }
  bump(cls, 1, pkg, `${r.name} @ ${r.file} stmt ${r.stmt} [${detail}]`);
  return { ...r, cls };
});

fs.writeFileSync(
  `/work/exp164-mint-census/dump-classified-${LABEL}.json`,
  JSON.stringify(rows, null, 1),
);
console.log("\n--- junk bindings by hypothesis ---");
for (const [c, v] of [...buckets.entries()].sort((a, b) => b[1].junk - a[1].junk)) {
  const top = [...v.byPkg.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5)
    .map(([p, n]) => `${p}:${n}`).join(" ");
  console.log(`${String(v.junk).padStart(4)}  ${c}  (${v.stmts} stmts)  ${top}`);
}
console.log("multi-slot-name rows:", multiName.rows);
for (const [c, list] of examples) console.log(`\n${c}:\n  ${list.join("\n  ")}`);
