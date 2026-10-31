/**
 * APP vs VENDOR: every change-vs-noise KPI, split by surface (exp170).
 *
 * Andrew, 2026-10-31: "every change-vs-noise KPI must be reported separately
 * for app code (src/) and vendor code (vendor/)". A reader who trusts vendor
 * identification ignores the vendor side of the diff, so the app numbers must
 * stand on their own — with no vendor contribution mixed in — and code that
 * crosses the boundary must be visible.
 *
 * ## The rule every split here obeys
 *
 * **A PARTITION of the frozen field: `app + vendor + crossBoundary = old`.**
 * The old fields are never touched (the RAW/CLEAN precedent, 034 README):
 * this module only says which surface each unit of an existing charge belongs
 * to. A unit that changed surface between `v-1` and `v` goes to
 * `crossBoundary`, once, as a move — never to both sides.
 *
 * ## What "the surface" of each KPI family means
 *
 * - **statement axis** (clean/noise/novel/noiseLn/realLn, scored on the
 *   BUNDLE `.humanify/humanified.js`): the bundle holds every statement
 *   that is not a CJS library factory — app code, and library code the
 *   bundler inlined as ES modules (CJS factory bodies never enter it,
 *   exp168). Each statement is filed in a tree file by the split ledger
 *   (`order[i]`, index-parallel to the bundle's statements); its surface is
 *   that file's. So `vendor` here is the bundle code the pipeline's library
 *   placement filed into vendor/ — right when the placement is right (an
 *   ESM-inlined library), app code otherwise (exp164/165's jose junk) —
 *   and exactly the mass a vendor-blind reader never sees. Cross = a matched statement whose every prior copy (same
 *   hash) was filed on the other surface.
 * - **name axis** (reloc/newName): each fresh name's home is
 *   `nameToFiles[name][0]` (the field's own rule); cross = the home changed
 *   surface.
 * - **relocSt**: the 1:1 hash-identified statements `treeChurn` compares;
 *   cross = prior and fresh file on different surfaces.
 * - **mints**: the pipeline's census is a list of NAMES with no position, so
 *   it is attributed by name: a census name is forced onto a surface only as
 *   far as the bundle's binding counts force it (k census copies, s app and v
 *   vendor bindings of that name: at least k-v are app, k-s vendor); the rest
 *   is `unattributed`, never guessed. app + vendor + unattributed = mints.
 * - **tree lines** (treeLn & co. for src/, vendorLn/vendorReal for vendor/):
 *   the two surfaces already have their own scorers (`composeDiff`, the
 *   vendor scorer), and NEITHER looks at the other tree — so a move is booked
 *   as an app removal AND a vendor addition, both real. `boundaryLines`
 *   pairs those by content (the identifier-blind `statementHash` of the
 *   tree files' top-level statements, net flow per hash) and takes each
 *   paired statement's lines out of BOTH surfaces' real charge into
 *   `crossBoundary`. Each side's take is capped at what that file was
 *   actually charged, so the partition is exact.
 *
 * No line is counted here: statement line counts are the statement owners'
 * (`statements.ts`, `diff-composition.ts`), file charges are `composeFile`'s
 * and the vendor scorer's rows. This module only attributes.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import * as t from "@babel/types";
import {
  composeFile,
  statementsOf as fileStatementsOf,
  type Stmt as FileStmt
} from "../037-noise-source-decomposition/diff-composition.js";
import type { VendorChurn } from "../046-vendor-noise/vendor-churn.js";
import type { Stmt } from "./statements.js";

export type Side = "app" | "vendor";

/** The emitted tree's vendor root. Everything else (src/, index.js) is app. */
const VENDOR_ROOT = "vendor/";

export function sideOf(file: string): Side {
  return file.startsWith(VENDOR_ROOT) ? "vendor" : "app";
}

export interface Split3<T> {
  app: T;
  vendor: T;
  crossBoundary: T;
}

export interface Ledger {
  nameToFiles: Record<string, string[]>;
  order?: string[];
  hashes?: string[];
}

// ---------------------------------------------------------------- statements

export type StatementKind = "clean" | "noise" | "novel";

/**
 * THE statement classification behind clean/noise/novel (analyze.ts `churn`
 * totals it; the split below partitions it). A fresh statement is CLEAN iff
 * its exact text is present under its hash in the prior, NOISE if the hash
 * is present with other text, NOVEL if the hash is new.
 */
export function statementKinds(fresh: Stmt[], prior: Stmt[]): StatementKind[] {
  const priorByHash = new Map<string, Set<string>>();
  for (const s of prior) {
    let set = priorByHash.get(s.hash);
    if (!set) priorByHash.set(s.hash, (set = new Set()));
    set.add(s.text);
  }
  return fresh.map((s) => {
    const texts = priorByHash.get(s.hash);
    if (!texts) return "novel";
    return texts.has(s.text) ? "clean" : "noise";
  });
}

export interface StatementCounts {
  total: number;
  unchangedClean: number;
  unchangedChurned: number;
  novel: number;
  namingNoiseLines: number;
  realLines: number;
}

const zeroStatements = (): StatementCounts => ({
  total: 0,
  unchangedClean: 0,
  unchangedChurned: 0,
  novel: 0,
  namingNoiseLines: 0,
  realLines: 0
});

/**
 * The statement axis, partitioned by the file each statement is filed in.
 * `undefined` when a ledger's order does not cover its bundle one-to-one —
 * an unattributable split is refused, never approximated.
 */
export function statementSplit(
  fresh: Stmt[],
  prior: Stmt[],
  kinds: StatementKind[],
  freshOrder: string[] | undefined,
  priorOrder: string[] | undefined
):
  | (Split3<StatementCounts> & {
      crossBoundary: StatementCounts & { toVendor: number; toApp: number };
    })
  | undefined {
  if (!freshOrder || !priorOrder) return undefined;
  if (freshOrder.length !== fresh.length) return undefined;
  if (priorOrder.length !== prior.length) return undefined;
  const priorSides = new Map<string, Set<Side>>();
  prior.forEach((s, i) => {
    let set = priorSides.get(s.hash);
    if (!set) priorSides.set(s.hash, (set = new Set()));
    set.add(sideOf(priorOrder[i]));
  });
  const out = {
    app: zeroStatements(),
    vendor: zeroStatements(),
    crossBoundary: { ...zeroStatements(), toVendor: 0, toApp: 0 }
  };
  fresh.forEach((s, i) => {
    const side = sideOf(freshOrder[i]);
    const kind = kinds[i];
    const was = priorSides.get(s.hash);
    const crossed = kind !== "novel" && was !== undefined && !was.has(side);
    const b = crossed ? out.crossBoundary : out[side];
    if (crossed) {
      if (side === "vendor") out.crossBoundary.toVendor++;
      else out.crossBoundary.toApp++;
    }
    b.total++;
    if (kind === "clean") b.unchangedClean++;
    else if (kind === "noise") {
      b.unchangedChurned++;
      b.namingNoiseLines += s.lines;
    } else {
      b.novel++;
      b.realLines += s.lines;
    }
  });
  return out;
}

// --------------------------------------------------------------------- names

export interface NameCounts {
  freshNames: number;
  novelNames: number;
  sameNameMovedFile: number;
}

/** reloc / newName, partitioned by each fresh name's home file. */
export function nameSplit(
  freshNameToFiles: Record<string, string[]>,
  priorNameToFiles: Record<string, string[]>
): Split3<NameCounts> {
  const zero = (): NameCounts => ({
    freshNames: 0,
    novelNames: 0,
    sameNameMovedFile: 0
  });
  const out = { app: zero(), vendor: zero(), crossBoundary: zero() };
  for (const [name, files] of Object.entries(freshNameToFiles)) {
    const file = files[0];
    const priorFile = priorNameToFiles[name]?.[0];
    const side = sideOf(file);
    const crossed = priorFile !== undefined && sideOf(priorFile) !== side;
    const b = crossed ? out.crossBoundary : out[side];
    b.freshNames++;
    if (priorFile === undefined) b.novelNames++;
    else if (priorFile !== file) b.sameNameMovedFile++;
  }
  return out;
}

// ------------------------------------------------------------------- relocSt

/**
 * The statements `treeChurn` can compare: a hash that occurs exactly once in
 * each ledger, with its prior and fresh file. ONE definition, used by
 * analyze.ts's `treeChurn` and by the split below.
 */
export function uniqueStatementPairs(
  fresh: Ledger,
  prior: Ledger
): Array<{ priorFile: string; freshFile: string }> {
  const fh = fresh.hashes ?? [];
  const ph = prior.hashes ?? [];
  const fo = fresh.order ?? [];
  const po = prior.order ?? [];
  const count = (hs: string[]) => {
    const m = new Map<string, number>();
    for (const h of hs) m.set(h, (m.get(h) ?? 0) + 1);
    return m;
  };
  const fc = count(fh);
  const pc = count(ph);
  const priorFileByHash = new Map<string, string>();
  ph.forEach((h, i) => {
    if (pc.get(h) === 1) priorFileByHash.set(h, po[i]);
  });
  const out: Array<{ priorFile: string; freshFile: string }> = [];
  fh.forEach((h, i) => {
    if (fc.get(h) !== 1) return;
    const priorFile = priorFileByHash.get(h);
    if (priorFile === undefined) return;
    out.push({ priorFile, freshFile: fo[i] });
  });
  return out;
}

export interface RelocCounts {
  statementsCompared: number;
  relocatedStatements: number;
}

export function relocSplit(fresh: Ledger, prior: Ledger): Split3<RelocCounts> {
  const zero = (): RelocCounts => ({
    statementsCompared: 0,
    relocatedStatements: 0
  });
  const out = { app: zero(), vendor: zero(), crossBoundary: zero() };
  for (const { priorFile, freshFile } of uniqueStatementPairs(fresh, prior)) {
    const side = sideOf(freshFile);
    const b = sideOf(priorFile) !== side ? out.crossBoundary : out[side];
    b.statementsCompared++;
    if (priorFile !== freshFile) b.relocatedStatements++;
  }
  return out;
}

// --------------------------------------------------------------------- mints

/** Binding names a statement declares (functions' ids and params, variable
 * declarators, class ids, catch params), each occurrence once. */
function declaredNames(stmt: t.Node): string[] {
  const out: string[] = [];
  const take = (n: t.Node | null | undefined) => {
    if (!n) return;
    for (const name of Object.keys(t.getBindingIdentifiers(n))) out.push(name);
  };
  t.traverseFast(stmt, (node) => {
    if (t.isFunction(node)) {
      for (const p of node.params) take(p);
      if ("id" in node) take(node.id);
    } else if (t.isVariableDeclarator(node)) take(node.id);
    else if (t.isClass(node)) take(node.id);
    else if (t.isCatchClause(node)) take(node.param);
  });
  return out;
}

export type DeclaredBySide = Record<Side, Map<string, number>>;

/** Per surface, how many bindings of each name the bundle declares — the
 * statements filed by the ledger's `order`. Needs `Stmt.node`. */
export function declaredBySide(stmts: Stmt[], order: string[]): DeclaredBySide {
  const out: DeclaredBySide = { app: new Map(), vendor: new Map() };
  stmts.forEach((s, i) => {
    if (!s.node) throw new Error("declaredBySide needs statement nodes");
    const m = out[sideOf(order[i])];
    for (const n of declaredNames(s.node)) m.set(n, (m.get(n) ?? 0) + 1);
  });
  return out;
}

export interface MintCounts {
  app: number;
  vendor: number;
  unattributed: number;
}

/** The census attributed only as far as the binding counts force it. */
export function mintSplit(
  censusNames: string[],
  declared: DeclaredBySide
): MintCounts {
  const census = new Map<string, number>();
  for (const n of censusNames) census.set(n, (census.get(n) ?? 0) + 1);
  const out: MintCounts = { app: 0, vendor: 0, unattributed: 0 };
  for (const [name, k] of census) {
    const s = declared.app.get(name) ?? 0;
    const v = declared.vendor.get(name) ?? 0;
    if (s + v === 0) {
      out.unattributed += k;
      continue;
    }
    const app = Math.min(k, Math.max(0, k - v));
    const vendor = Math.min(k - app, Math.max(0, k - s));
    out.app += app;
    out.vendor += vendor;
    out.unattributed += k - app - vendor;
  }
  return out;
}

// ---------------------------------------------------------------- tree lines

export interface AppLines {
  churnLines: number;
  churnLinesClean?: number;
  real: number;
  realClean?: number;
  fileAddRemove: number;
  noise: number;
  naming: number;
  alias: number;
  reorder: number;
}

export interface VendorLines {
  churnLines: number;
  noise: number;
  real: number;
}

export interface CrossMove {
  /** Tree-relative paths, `src/...` / `vendor/...`. */
  from: string;
  to: string;
  statements: number;
  /** Both sides' statement lines (delete + add), before the per-file cap. */
  lines: number;
}

export interface CrossLines {
  /** Lines taken out of the two surfaces' charges: appSide + vendorSide. */
  churnLines: number;
  appSide: number;
  /** The part of `appSide` taken out of src/'s whole-file charge
   * (`fileAddRemove`); the rest came out of src/'s `real`. */
  appSideFromFiles: number;
  vendorSide: number;
  toVendor: { statements: number; lines: number };
  toApp: { statements: number; lines: number };
  /** Statement lines the per-file caps refused (a move charged less than
   * its own lines, e.g. a tier-3 edit pairing). Advisory. */
  capped: number;
  /** Lines of one-line statements that crossed by hash but were NOT paired
   * (`MIN_PAIR_LINES`): a shape too generic to prove a move. They stay
   * charged on both sides. Advisory. */
  refused: number;
  /** Real-charged vendor files that did not parse as modules: their
   * statements could not be offered for pairing. */
  unparsedVendorFiles: number;
  moves: CrossMove[];
}

/** The layout block's fields this split reads (analyze.ts `layoutChurn`). */
interface LayoutIn {
  churnLines: number;
  churnLinesClean?: number;
  real: number;
  realClean?: number;
  fileAddRemove: number;
  noise: number;
  naming: number;
  alias: number;
  reorder: number;
}

/** A statement instance one side lost or gained, by tree-relative file. */
interface Instance {
  file: string;
  lines: number;
}

interface Flows {
  out: Map<string, Instance[]>;
  in: Map<string, Instance[]>;
}

const push = (m: Map<string, Instance[]>, h: string, x: Instance) => {
  const l = m.get(h);
  if (l) l.push(x);
  else m.set(h, [x]);
};

/**
 * Per hash, the statement instances `prior` has beyond `fresh` (removed) and
 * vice versa (added). Exact-text copies are matched first, so the instances
 * reported are the ones that genuinely left or arrived.
 */
function fileFlows(
  rel: string,
  prior: FileStmt[],
  fresh: FileStmt[],
  flows: Flows
): void {
  const group = (ss: FileStmt[]) => {
    const m = new Map<string, FileStmt[]>();
    for (const s of ss) {
      const l = m.get(s.hash);
      if (l) l.push(s);
      else m.set(s.hash, [s]);
    }
    return m;
  };
  const p = group(prior);
  const f = group(fresh);
  const surplus = (a: FileStmt[], b: FileStmt[] | undefined): FileStmt[] => {
    const k = a.length - (b?.length ?? 0);
    if (k <= 0) return [];
    const bTexts = new Map<string, number>();
    for (const s of b ?? []) bTexts.set(s.text, (bTexts.get(s.text) ?? 0) + 1);
    const unmatched: FileStmt[] = [];
    const matched: FileStmt[] = [];
    for (const s of a) {
      const n = bTexts.get(s.text) ?? 0;
      if (n > 0) {
        bTexts.set(s.text, n - 1);
        matched.push(s);
      } else unmatched.push(s);
    }
    return [...unmatched, ...matched].slice(0, k);
  };
  for (const [h, ss] of p) {
    for (const s of surplus(ss, f.get(h)))
      push(flows.out, h, { file: rel, lines: s.lines.length });
  }
  for (const [h, ss] of f) {
    for (const s of surplus(ss, p.get(h)))
      push(flows.in, h, { file: rel, lines: s.lines.length });
  }
}

function listJs(dir: string, base = dir, out: string[] = []): string[] {
  if (!fs.existsSync(dir)) return out;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) listJs(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out.sort();
}

const readOr = (p: string): string | undefined =>
  fs.existsSync(p) ? fs.readFileSync(p, "utf8") : undefined;

/** src/ flows over every file that differs, plus each file's charge cap. */
function srcFlows(priorSrc: string, freshSrc: string) {
  const flows: Flows = { out: new Map(), in: new Map() };
  const files = [...new Set([...listJs(priorSrc), ...listJs(freshSrc)])].sort();
  const texts = new Map<string, { prior?: string; fresh?: string }>();
  for (const rel of files) {
    const prior = readOr(path.join(priorSrc, rel));
    const fresh = readOr(path.join(freshSrc, rel));
    if (prior === fresh) continue;
    texts.set(rel, { prior, fresh });
    fileFlows(
      `src/${rel}`,
      prior === undefined ? [] : fileStatementsOf(prior),
      fresh === undefined ? [] : fileStatementsOf(fresh),
      flows
    );
  }
  /** What composeDiff charged this file, and in which bucket. */
  const cap = (treeRel: string): { lines: number; bucket: "real" | "file" } => {
    const tx = texts.get(treeRel.slice("src/".length));
    if (!tx) return { lines: 0, bucket: "real" };
    if (tx.prior === undefined || tx.fresh === undefined) {
      // A one-sided file is charged whole to fileAddRemove, by composeDiff's
      // own rule (its text's line count).
      const text = (tx.prior ?? tx.fresh) as string;
      return { lines: text.split("\n").length, bucket: "file" };
    }
    return { lines: composeFile(tx.prior, tx.fresh).real, bucket: "real" };
  };
  return { flows, cap };
}

const VENDOR_REAL = new Set(["realChange", "trulyAdded", "trulyRemoved"]);

/** vendor/ flows over the files the vendor scorer charged as REAL, and each
 * file's real charge (the scorer's rows) as its cap. */
function vendorFlows(
  priorVendor: string,
  freshVendor: string,
  raw: VendorChurn
) {
  const flows: Flows = { out: new Map(), in: new Map() };
  const capOf = new Map<string, number>();
  let unparsed = 0;
  const parse = (p: string): FileStmt[] | null => {
    try {
      return fileStatementsOf(fs.readFileSync(p, "utf8"));
    } catch {
      return null;
    }
  };
  for (const row of raw.fileRows) {
    if (!VENDOR_REAL.has(row.bucket)) continue;
    const prior = row.prior ? parse(path.join(priorVendor, row.prior)) : [];
    const fresh = row.fresh ? parse(path.join(freshVendor, row.fresh)) : [];
    if (prior === null || fresh === null) {
      unparsed++;
      continue;
    }
    // A paired row (relocated / changed in place) carries one charge for
    // both paths; both keys point at it so either side's take is capped.
    const key = `vendor/${row.fresh ?? row.prior}`;
    capOf.set(key, (capOf.get(key) ?? 0) + row.lines);
    if (row.prior && row.fresh && row.prior !== row.fresh) {
      capOf.set(`vendor/${row.prior}`, capOf.get(key) as number);
    }
    if (row.prior && row.fresh) {
      // Same file on both sides: only the per-file surplus moved.
      const rel = `vendor/${row.fresh}`;
      fileFlows(rel, prior, fresh, flows);
    } else if (row.prior) fileFlows(`vendor/${row.prior}`, prior, [], flows);
    else fileFlows(`vendor/${row.fresh}`, [], fresh, flows);
  }
  return { flows, capOf, unparsed };
}

/**
 * The smallest statement a content pairing trusts. `statementHash` blanks
 * every name, so a one-line shape — `var x = () => {};`, `var a, b;` — is
 * the same hash in unrelated code: on exp164 such one-liners "paired" a
 * src no-op with an unrelated vendor no-op five times. Precision over
 * recall: a one-line statement that moved stays charged on both sides, and
 * the refused lines are reported (`refused`), never silently dropped.
 */
const MIN_PAIR_LINES = 2;

/** Net flow of a hash from one surface to the other, paired in file order. */
function pairFlows(
  from: Flows,
  to: Flows,
  onPair: (a: Instance, b: Instance) => void
): number {
  let refused = 0;
  for (const [h, gone] of from.out) {
    const arrived = to.in.get(h);
    if (!arrived) continue;
    const net = (xs: Instance[] | undefined) => xs?.length ?? 0;
    const k = Math.min(
      gone.length - net(from.in.get(h)),
      arrived.length - net(to.out.get(h))
    );
    for (let i = 0; i < k; i++) {
      if (gone[i].lines < MIN_PAIR_LINES || arrived[i].lines < MIN_PAIR_LINES) {
        refused += gone[i].lines + arrived[i].lines;
      } else onPair(gone[i], arrived[i]);
    }
  }
  return refused;
}

export function boundaryLines(input: {
  priorSrc: string;
  freshSrc: string;
  priorVendor: string;
  freshVendor: string;
  layout: LayoutIn;
  vendorRaw: VendorChurn;
  vendor: VendorLines;
}): { app: AppLines; vendor: VendorLines; crossBoundary: CrossLines } {
  const src = srcFlows(input.priorSrc, input.freshSrc);
  const ven = vendorFlows(
    input.priorVendor,
    input.freshVendor,
    input.vendorRaw
  );

  const appTake = new Map<string, number>();
  const vendorTake = new Map<string, number>();
  const moves = new Map<string, CrossMove>();
  const cross: CrossLines = {
    churnLines: 0,
    appSide: 0,
    appSideFromFiles: 0,
    vendorSide: 0,
    toVendor: { statements: 0, lines: 0 },
    toApp: { statements: 0, lines: 0 },
    capped: 0,
    refused: 0,
    unparsedVendorFiles: ven.unparsed,
    moves: []
  };
  const add = (m: Map<string, number>, f: string, n: number) =>
    m.set(f, (m.get(f) ?? 0) + n);
  const record = (
    a: Instance,
    b: Instance,
    dir: "toVendor" | "toApp",
    appInst: Instance,
    venInst: Instance
  ) => {
    add(appTake, appInst.file, appInst.lines);
    add(vendorTake, venInst.file, venInst.lines);
    cross[dir].statements++;
    cross[dir].lines += a.lines + b.lines;
    const key = `${a.file}\u0000${b.file}`;
    const m = moves.get(key) ?? {
      from: a.file,
      to: b.file,
      statements: 0,
      lines: 0
    };
    m.statements++;
    m.lines += a.lines + b.lines;
    moves.set(key, m);
  };
  cross.refused =
    pairFlows(src.flows, ven.flows, (a, b) => record(a, b, "toVendor", a, b)) +
    pairFlows(ven.flows, src.flows, (a, b) => record(a, b, "toApp", b, a));

  // Take each file's statement lines out of what that file was CHARGED,
  // never more — so the partition is exact.
  let appReal = 0;
  let appFile = 0;
  let wanted = 0;
  for (const [file, lines] of appTake) {
    const c = src.cap(file);
    const take = Math.min(lines, c.lines);
    wanted += lines;
    if (c.bucket === "file") appFile += take;
    else appReal += take;
  }
  let vendorReal = 0;
  for (const [file, lines] of vendorTake) {
    wanted += lines;
    vendorReal += Math.min(lines, ven.capOf.get(file) ?? 0);
  }
  cross.appSide = appReal + appFile;
  cross.appSideFromFiles = appFile;
  cross.vendorSide = vendorReal;
  cross.churnLines = cross.appSide + cross.vendorSide;
  cross.capped = wanted - cross.churnLines;
  cross.moves = [...moves.values()].sort(
    (x, y) => y.lines - x.lines || x.from.localeCompare(y.from)
  );

  const l = input.layout;
  const app: AppLines = {
    churnLines: l.churnLines - cross.appSide,
    ...(l.churnLinesClean !== undefined
      ? { churnLinesClean: l.churnLinesClean - cross.appSide }
      : {}),
    real: l.real - appReal,
    ...(l.realClean !== undefined ? { realClean: l.realClean - appReal } : {}),
    fileAddRemove: l.fileAddRemove - appFile,
    noise: l.noise,
    naming: l.naming,
    alias: l.alias,
    reorder: l.reorder
  };
  const vendor: VendorLines = {
    churnLines: input.vendor.churnLines - vendorReal,
    noise: input.vendor.noise,
    real: input.vendor.real - vendorReal
  };
  return { app, vendor, crossBoundary: cross };
}
