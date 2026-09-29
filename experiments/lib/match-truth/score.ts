/**
 * The scorer (exp092): one `humanify match` dump in, one deterministic
 * scorecard out — precision-like and recall-like numbers for the
 * matching machinery against the harness's ground-truth rule.
 *
 * GROUND TRUTH. A function/statement whose canonical form
 * (see canonical.ts) appears EXACTLY ONCE on each side is a must-match
 * pair: the same code, provably, modulo wrapper spelling and identifier
 * renaming. Canonical forms that repeat on a side (duplicate helpers)
 * are excluded — the matcher may pair those in any order — and counted
 * separately (`duplicateClass`).
 *
 * WHAT EACH NUMBER IS.
 * - `recall` — of the must-match pairs, the fraction the matcher
 *   reported. THE primary metric: a miss here is a real matching gap,
 *   visible on a per-package breakdown.
 * - `reportedClasses` — every reported pair classified by what the two
 *   slices actually are: `identical` (canonical-equal), `near` (line
 *   distance within the near threshold — the changed-but-should-match
 *   tier), `far`. `far` is NOT proof of a wrong match (matching changed
 *   code is the matcher's job) — it is a LEAD list; exact precision is
 *   only knowable where ground truth exists by construction.
 * - `statements.recall` — the same rule over the statement twins
 *   (proposals, not bridging: `abstained:no-candidacy` is by design).
 */
import { canonical, canonicalLoose, lineDiffMass } from "./canonical.js";

export const SCORECARD_SCHEMA_VERSION = 1;

export interface InventoryRow {
  id: string;
  start: number;
  end: number;
  name: string;
  slice: string;
}

interface StatementRow {
  start: number;
  end: number;
  hash: string;
  slice: string;
}

interface PairRow {
  prior: number;
  fresh: number;
  priorId: string;
  freshId: string;
  tier: string;
}

interface GatesRow {
  tier: string;
  prior: { start: number; end: number };
  fresh: { start: number; end: number };
  outcome: string;
}

interface DumpFile {
  path: string;
  freshText: string;
  functions: {
    prior: InventoryRow[];
    fresh: InventoryRow[];
    pairs: PairRow[];
  };
  twins: {
    prior: StatementRow[];
    fresh: StatementRow[];
    gates: { rows: GatesRow[] };
  };
}

export interface Scorecard {
  schemaVersion: number;
  path: string;
  functions: {
    priorCount: number;
    freshCount: number;
    priorNames: string[];
    mustMatch: number;
    matchedOfMust: number;
    recall: number | null;
    duplicateClass: number;
    shouldMatch: number;
    matchedOfShould: number;
    shouldRecall: number | null;
    reported: number;
    reportedClasses: { identical: number; near: number; far: number };
    tierCounts: Record<string, number>;
    missedMust: Array<{
      priorName: string;
      freshName: string;
      priorSlice: string;
    }>;
    missedShould: Array<{
      priorName: string;
      freshName: string;
      priorSlice: string;
    }>;
    farPairs: Array<{
      priorName: string;
      freshName: string;
      tier: string;
      priorSlice: string;
      freshSlice: string;
    }>;
  };
  statements: {
    priorCount: number;
    freshCount: number;
    mustMatch: number;
    proposedOfMust: number;
    bridgedOfMust: number;
    recall: number | null;
    duplicateClass: number;
  };
}

const FAR_SLICE_CHARS = 400;
const NEAR_ABSOLUTE_LINES = 3;
const NEAR_FRACTION = 0.2;

type CanonMap = Map<string, string>;

function canon(
  slice: string,
  memo: CanonMap,
  form: (slice: string) => string = canonical
): string {
  let c = memo.get(slice);
  if (c === undefined) {
    c = form(slice);
    memo.set(slice, c);
  }
  return c;
}

function countsOf(values: string[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const v of values) {
    m.set(v, (m.get(v) ?? 0) + 1);
  }
  return m;
}

/** The first index of each distinct value. */
function firstIndexOf(values: string[]): Map<string, number> {
  const m = new Map<string, number>();
  values.forEach((v, i) => {
    if (!m.has(v)) {
      m.set(v, i);
    }
  });
  return m;
}

interface MustResult {
  must: Array<[number, number]>;
  duplicateClass: number;
}

/** must-pairs: canonical forms unique on BOTH sides; plus the rows
 * excluded as the duplicate class. */
function mustPairs(priorCanon: string[], freshCanon: string[]): MustResult {
  const pc = countsOf(priorCanon);
  const fc = countsOf(freshCanon);
  const firstFresh = firstIndexOf(freshCanon);
  const inDuplicateClass = (c: string) =>
    (pc.get(c) ?? 0) > 1 || (fc.get(c) ?? 0) > 1;
  const must: Array<[number, number]> = [];
  let duplicateClass = 0;
  priorCanon.forEach((c, i) => {
    if ((pc.get(c) ?? 0) === 1 && (fc.get(c) ?? 0) === 1) {
      must.push([i, firstFresh.get(c) ?? -1]);
    } else if (inDuplicateClass(c)) {
      duplicateClass += 1;
    }
  });
  for (const c of freshCanon) {
    if (inDuplicateClass(c)) {
      duplicateClass += 1;
    }
  }
  return { must, duplicateClass };
}

function classify(
  priorCanon: string,
  freshCanon: string
): "identical" | "near" | "far" {
  if (priorCanon === freshCanon) {
    return "identical";
  }
  const maxLines = Math.max(
    priorCanon.split("\n").length,
    freshCanon.split("\n").length
  );
  const mass = lineDiffMass(priorCanon, freshCanon);
  // Near: the edit mass is small in absolute terms AND relative to the
  // function's size — the changed-but-should-match tier.
  const nearBar = Math.max(
    NEAR_ABSOLUTE_LINES,
    Math.floor(NEAR_FRACTION * maxLines)
  );
  return mass <= nearBar ? "near" : "far";
}

function slicePreview(s: string): string {
  return s.length > FAR_SLICE_CHARS ? `${s.slice(0, FAR_SLICE_CHARS)}…` : s;
}

function spanIndex(rows: StatementRow[]): Map<string, number> {
  const m = new Map<string, number>();
  for (let i = 0; i < rows.length; i += 1) {
    const r = rows[i];
    m.set(`${r.start}:${r.end}`, i);
  }
  return m;
}

interface StatementScore {
  priorCount: number;
  freshCount: number;
  mustMatch: number;
  proposedOfMust: number;
  bridgedOfMust: number;
  recall: number | null;
  duplicateClass: number;
}

/** Statement twins: the must-set, which of it was proposed as a twin,
 * and how much of that bridged slots. */
function scoreStatements(file: DumpFile, memo: CanonMap): StatementScore {
  const priorCanon = file.twins.prior.map((s) => canon(s.slice, memo));
  const freshCanon = file.twins.fresh.map((s) => canon(s.slice, memo));
  const pc = countsOf(priorCanon);
  const fc = countsOf(freshCanon);
  const priorIdx = spanIndex(file.twins.prior);
  const freshIdx = spanIndex(file.twins.fresh);
  // A twin PROPOSAL (any gate outcome) is the recognition event; the
  // `bridged` outcome adds that slots were actually transferred.
  const proposed = new Set<string>();
  const bridged = new Set<string>();
  for (const row of file.twins.gates.rows) {
    const p = priorIdx.get(`${row.prior.start}:${row.prior.end}`);
    const f = freshIdx.get(`${row.fresh.start}:${row.fresh.end}`);
    if (p === undefined || f === undefined) {
      continue;
    }
    proposed.add(`${p}:${f}`);
    if (row.outcome === "bridged") {
      bridged.add(`${p}:${f}`);
    }
  }
  const firstFresh = firstIndexOf(freshCanon);
  let mustMatch = 0;
  let proposedOfMust = 0;
  let bridgedOfMust = 0;
  let duplicateClass = 0;
  priorCanon.forEach((c, i) => {
    const pCount = pc.get(c) ?? 0;
    const fCount = fc.get(c) ?? 0;
    if (pCount !== 1 || fCount !== 1) {
      if (pCount > 1 || fCount > 1) {
        duplicateClass += 1;
      }
      return;
    }
    mustMatch += 1;
    const key = `${i}:${firstFresh.get(c) ?? -1}`;
    if (proposed.has(key)) {
      proposedOfMust += 1;
      if (bridged.has(key)) {
        bridgedOfMust += 1;
      }
    }
  });
  return {
    priorCount: file.twins.prior.length,
    freshCount: file.twins.fresh.length,
    mustMatch,
    proposedOfMust,
    bridgedOfMust,
    recall: mustMatch === 0 ? null : proposedOfMust / mustMatch,
    duplicateClass
  };
}

function tierAttribution(
  must: Array<[number, number]>,
  tierOf: Map<string, string>,
  prior: InventoryRow[],
  fresh: InventoryRow[]
): {
  tierCounts: Record<string, number>;
  matchedOfMust: number;
  missedMust: Scorecard["functions"]["missedMust"];
} {
  const tierCounts: Record<string, number> = {};
  const missedMust: Scorecard["functions"]["missedMust"] = [];
  let matchedOfMust = 0;
  for (const [p, f] of must) {
    const key = `${p}:${f}`;
    const tier = tierOf.get(key);
    if (tier !== undefined) {
      matchedOfMust += 1;
      tierCounts[tier] = (tierCounts[tier] ?? 0) + 1;
    } else {
      missedMust.push({
        priorName: prior[p].name,
        freshName: fresh[f].name,
        priorSlice: slicePreview(prior[p].slice)
      });
    }
  }
  const sorted: Record<string, number> = {};
  for (const key of Object.keys(tierCounts).sort()) {
    sorted[key] = tierCounts[key];
  }
  return { tierCounts: sorted, matchedOfMust, missedMust };
}

function reportedBreakdown(
  pairs: PairRow[],
  priorCanon: string[],
  freshCanon: string[],
  prior: InventoryRow[],
  fresh: InventoryRow[]
): {
  classes: { identical: number; near: number; far: number };
  farPairs: Scorecard["functions"]["farPairs"];
} {
  const classes = { identical: 0, near: 0, far: 0 };
  const farPairs: Scorecard["functions"]["farPairs"] = [];
  for (const pair of pairs) {
    const kind = classify(priorCanon[pair.prior], freshCanon[pair.fresh]);
    classes[kind] += 1;
    if (kind === "far") {
      farPairs.push({
        priorName: prior[pair.prior].name,
        freshName: fresh[pair.fresh].name,
        tier: pair.tier,
        priorSlice: slicePreview(prior[pair.prior].slice),
        freshSlice: slicePreview(fresh[pair.fresh].slice)
      });
    }
  }
  return { classes, farPairs };
}

function scoreShould(
  should: Array<[number, number]>,
  reported: Set<string>,
  prior: InventoryRow[],
  fresh: InventoryRow[]
): {
  matchedOfShould: number;
  missedShould: Scorecard["functions"]["missedShould"];
} {
  const missedShould: Scorecard["functions"]["missedShould"] = [];
  let matchedOfShould = 0;
  for (const [p, f] of should) {
    if (reported.has(`${p}:${f}`)) {
      matchedOfShould += 1;
    } else {
      missedShould.push({
        priorName: prior[p].name,
        freshName: fresh[f].name,
        priorSlice: slicePreview(prior[p].slice)
      });
    }
  }
  return { matchedOfShould, missedShould };
}

/** Score one `humanify match` dump. Throws loudly on a foreign shape. */
export function scoreMatchDump(dump: unknown): Scorecard {
  if (typeof dump !== "object" || dump === null) {
    throw new Error("the match dump is not a JSON object");
  }
  const d = dump as { schemaVersion?: unknown; files?: unknown };
  if (d.schemaVersion !== 1) {
    throw new Error(
      `unsupported match dump schemaVersion: ${String(d.schemaVersion)} (expected 1)`
    );
  }
  const fileCount = Array.isArray(d.files) ? (d.files as unknown[]).length : 0;
  if (fileCount !== 1) {
    throw new Error(
      `expected exactly one file section in the dump, got ${fileCount} (multi-file unpacks are not scored yet)`
    );
  }
  const file = (d.files as DumpFile[])[0];
  const memo: CanonMap = new Map();
  const looseMemo: CanonMap = new Map();
  const priorRows = file.functions.prior;
  const freshRows = file.functions.fresh;
  const priorCanon = priorRows.map((r) => canon(r.slice, memo));
  const freshCanon = freshRows.map((r) => canon(r.slice, memo));
  const { must, duplicateClass } = mustPairs(priorCanon, freshCanon);

  // Tier 2 (should-match, advisory): loose-canonical pairs — literal
  // differences blurred — outside the tier-1 must set.
  const priorLoose = priorRows.map((r) =>
    canon(r.slice, looseMemo, canonicalLoose)
  );
  const freshLoose = freshRows.map((r) =>
    canon(r.slice, looseMemo, canonicalLoose)
  );
  const tier1Keys = new Set(must.map(([p, f]) => `${p}:${f}`));
  const should = mustPairs(priorLoose, freshLoose).must.filter(
    ([p, f]) => !tier1Keys.has(`${p}:${f}`)
  );

  const reported = new Set(
    file.functions.pairs.map((p) => `${p.prior}:${p.fresh}`)
  );
  const tierOf = new Map(
    file.functions.pairs.map((p) => [`${p.prior}:${p.fresh}`, p.tier])
  );
  const { tierCounts, matchedOfMust, missedMust } = tierAttribution(
    must,
    tierOf,
    priorRows,
    freshRows
  );
  const { classes, farPairs } = reportedBreakdown(
    file.functions.pairs,
    priorCanon,
    freshCanon,
    priorRows,
    freshRows
  );
  const { matchedOfShould, missedShould } = scoreShould(
    should,
    reported,
    priorRows,
    freshRows
  );

  return {
    schemaVersion: SCORECARD_SCHEMA_VERSION,
    path: file.path,
    functions: {
      priorCount: priorRows.length,
      freshCount: freshRows.length,
      priorNames: priorRows.map((r) => r.name),
      mustMatch: must.length,
      matchedOfMust,
      recall: must.length === 0 ? null : matchedOfMust / must.length,
      duplicateClass,
      shouldMatch: should.length,
      matchedOfShould,
      shouldRecall:
        should.length === 0 ? null : matchedOfShould / should.length,
      reported: file.functions.pairs.length,
      reportedClasses: classes,
      tierCounts,
      missedMust,
      missedShould,
      farPairs
    },
    statements: scoreStatements(file, memo)
  };
}

export interface PackageScorecard {
  package: string;
  oldVersion: string;
  newVersion: string;
  file: string;
  scorecard: Scorecard;
}

export interface Aggregate {
  packages: number;
  mustMatch: number;
  matchedOfMust: number;
  recall: number | null;
  shouldMatch: number;
  matchedOfShould: number;
  shouldRecall: number | null;
  reported: number;
  reportedClasses: { identical: number; near: number; far: number };
  statementMustMatch: number;
  statementProposedOfMust: number;
  statementBridgedOfMust: number;
  statementRecall: number | null;
}

/** Totals over per-package scorecards (exp092's baseline row). */
export function aggregate(cards: PackageScorecard[]): Aggregate {
  const reportedClasses = { identical: 0, near: 0, far: 0 };
  let mustMatch = 0;
  let matchedOfMust = 0;
  let shouldMatch = 0;
  let matchedOfShould = 0;
  let reported = 0;
  let statementMustMatch = 0;
  let statementProposed = 0;
  let statementBridged = 0;
  for (const { scorecard: c } of cards) {
    mustMatch += c.functions.mustMatch;
    matchedOfMust += c.functions.matchedOfMust;
    shouldMatch += c.functions.shouldMatch;
    matchedOfShould += c.functions.matchedOfShould;
    reported += c.functions.reported;
    reportedClasses.identical += c.functions.reportedClasses.identical;
    reportedClasses.near += c.functions.reportedClasses.near;
    reportedClasses.far += c.functions.reportedClasses.far;
    statementMustMatch += c.statements.mustMatch;
    statementProposed += c.statements.proposedOfMust;
    statementBridged += c.statements.bridgedOfMust;
  }
  return {
    packages: cards.length,
    mustMatch,
    matchedOfMust,
    recall: mustMatch === 0 ? null : matchedOfMust / mustMatch,
    shouldMatch,
    matchedOfShould,
    shouldRecall: shouldMatch === 0 ? null : matchedOfShould / shouldMatch,
    reported,
    reportedClasses,
    statementMustMatch,
    statementProposedOfMust: statementProposed,
    statementBridgedOfMust: statementBridged,
    statementRecall:
      statementMustMatch === 0 ? null : statementProposed / statementMustMatch
  };
}
