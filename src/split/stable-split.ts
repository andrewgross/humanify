/**
 * Stable split: statement-level splitting of a Bun wrapper bundle into
 * folders/files whose assignments PERSIST across releases (exp023).
 *
 * The bundle's app code is one wrapper IIFE whose body statements preserve
 * the original emission order (renaming is pure). Two regimes:
 *
 *   - FRESH (no prior ledger — the first split release): boundary
 *     detection over the statement sequence. Cross-reference density dips
 *     mark the original module seams; cut at the least-cohesive position
 *     inside statement- and line-budgeted windows; folders are a coarser
 *     cut of the same signal. Files and folders are named after their
 *     most externally-referenced binding.
 *   - PRIOR-CARRIED (every release after): each statement inherits the
 *     file its declared names had last release, read from the persisted
 *     ledger. Bare names are not unique keys (Bun bundles legally
 *     redeclare `var`s), so votes resolve per name: all prior occurrences
 *     in one file → that file; equal occurrence counts across releases →
 *     the kth declaration inherits the kth prior file; anything else
 *     abstains (exp020's unequal-count refusal). Unanimous votes inherit;
 *     disagreements and genuinely-new statements follow their preceding
 *     neighbor, so new code lands beside the code that uses it.
 *
 * Precision over recall, file axis: moving code between files across
 * releases is this stage's false positive — a matched statement never
 * moves, and every ambiguous case defaults to locality, never a guess.
 *
 * Emission slices the ORIGINAL rendered text by statement byte offsets
 * (exact bytes, no re-generation drift). The module parses the code it is
 * given privately, so offsets always align. Import/export generation is a
 * later stage; the emitted tree is the review artifact and the ledger
 * (`_split-ledger.json`) records the full statement order for
 * reconstruction and for the NEXT release's inheritance.
 */

import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { findWrapperFunction } from "../analysis/wrapper-detection.js";

/** Segmentation budgets; tests inject small ones. */
export interface StableSplitBudgets {
  /** Statements per file (min/max). */
  minSeg: number;
  maxSeg: number;
  /** Line budget per file — giant functions otherwise blow the size cap. */
  maxLines: number;
  /** Window (statements each side) for boundary cohesion scoring. */
  window: number;
  /** Files per folder (min/max). */
  minFolder: number;
  maxFolder: number;
}

const DEFAULT_BUDGETS: StableSplitBudgets = {
  minSeg: 80,
  maxSeg: 400,
  maxLines: 4000,
  window: 40,
  minFolder: 8,
  maxFolder: 20
};

/** Stems that make bad file names (placeholder/minted-ish). */
const BAD_STEM = /^(noop\d*|initializeModule\d+|placeholder\w*|_+\d*)$/i;

export const SPLIT_LEDGER_FILENAME = "_split-ledger.json";

/**
 * The persisted split ledger — the cross-release memory. `nameToFiles`
 * holds, per declared name, the ORDERED file list of its declaration
 * occurrences; `order` holds each wrapper-body statement's file, in
 * statement order (the reconstruction manifest).
 */
export interface StableSplitLedger {
  version: 1;
  files: string[];
  nameToFiles: Record<string, string[]>;
  order: string[];
}

export interface StableSplitStats {
  statements: number;
  files: number;
  folders: number;
  inherited: number;
  inheritedViaOrdinal: number;
  conflictDisagree: number;
  noVote: number;
  residueLocality: number;
}

export interface StableSplitResult {
  /** Relative path ("folder/name.js") → file content. */
  fileContents: Map<string, string>;
  ledger: StableSplitLedger;
  stats: StableSplitStats;
}

export interface StableSplitOptions {
  prior?: StableSplitLedger;
  budgets?: Partial<StableSplitBudgets>;
}

function declaredNames(stmt: t.Statement): string[] {
  return Object.keys(t.getBindingIdentifiers(stmt, false));
}

function countOccurrences(body: t.Statement[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const stmt of body) {
    for (const n of declaredNames(stmt)) {
      counts.set(n, (counts.get(n) ?? 0) + 1);
    }
  }
  return counts;
}

// ---------------------------------------------------------------------------
// Prior-carried assignment
// ---------------------------------------------------------------------------

interface Vote {
  file?: string;
  kind: "all-same" | "ordinal" | "abstain";
}

/** One name's vote for its kth declaration occurrence. */
function voteFor(
  name: string,
  ordinal: number,
  priorNames: Map<string, string[]>,
  newCounts: Map<string, number>
): Vote {
  const files = priorNames.get(name);
  if (!files || files.length === 0) return { kind: "abstain" };
  if (files.every((f) => f === files[0])) {
    return { file: files[0], kind: "all-same" };
  }
  if (newCounts.get(name) === files.length && ordinal < files.length) {
    return { file: files[ordinal], kind: "ordinal" };
  }
  return { kind: "abstain" };
}

interface TransferOutcome {
  assignment: string[];
  stats: Omit<StableSplitStats, "statements" | "files" | "folders">;
}

/** Vote across a statement's declared names; track per-name ordinals. */
function statementVotes(
  stmt: t.Statement,
  seen: Map<string, number>,
  priorNames: Map<string, string[]>,
  newCounts: Map<string, number>
): { votes: Set<string>; usedOrdinal: boolean } {
  const votes = new Set<string>();
  let usedOrdinal = false;
  for (const name of declaredNames(stmt)) {
    const ordinal = seen.get(name) ?? 0;
    seen.set(name, ordinal + 1);
    const vote = voteFor(name, ordinal, priorNames, newCounts);
    if (vote.file) {
      votes.add(vote.file);
      if (vote.kind === "ordinal") usedOrdinal = true;
    }
  }
  return { votes, usedOrdinal };
}

/** Inherit prior assignments; residue follows its preceding neighbor. */
function assignWithPrior(
  body: t.Statement[],
  prior: StableSplitLedger
): TransferOutcome {
  // Own-properties only: bindings named `constructor`/`toString` collide
  // with Object.prototype on a plain-object map.
  const priorNames = new Map(Object.entries(prior.nameToFiles));
  const newCounts = countOccurrences(body);
  const seen = new Map<string, number>();
  const assignment: string[] = new Array(body.length);
  const stats: TransferOutcome["stats"] = {
    inherited: 0,
    inheritedViaOrdinal: 0,
    conflictDisagree: 0,
    noVote: 0,
    residueLocality: 0
  };

  for (let i = 0; i < body.length; i++) {
    const { votes, usedOrdinal } = statementVotes(
      body[i],
      seen,
      priorNames,
      newCounts
    );
    if (votes.size === 1) {
      assignment[i] = [...votes][0];
      stats.inherited++;
      if (usedOrdinal) stats.inheritedViaOrdinal++;
      continue;
    }
    if (votes.size > 1) stats.conflictDisagree++;
    else stats.noVote++;
    assignment[i] = i > 0 ? assignment[i - 1] : prior.files[0];
    stats.residueLocality++;
  }
  return { assignment, stats };
}

// ---------------------------------------------------------------------------
// Fresh grouping (release 1): reference-locality boundary detection
// ---------------------------------------------------------------------------

/** Per statement: indices of wrapper-body declarations it references.
 * Approximate on purpose (no shadow analysis) — symmetric noise a
 * boundary score tolerates. */
function referenceIndices(body: t.Statement[]): Array<Set<number>> {
  const declIndex = new Map<string, number>();
  for (let i = 0; i < body.length; i++) {
    for (const n of declaredNames(body[i])) {
      if (!declIndex.has(n)) declIndex.set(n, i);
    }
  }
  return body.map((stmt, i) => {
    const own = new Set(declaredNames(stmt));
    const refs = new Set<number>();
    t.traverseFast(stmt, (node) => {
      if (!t.isIdentifier(node) || own.has(node.name)) return;
      const idx = declIndex.get(node.name);
      if (idx !== undefined && idx !== i) refs.add(idx);
    });
    return refs;
  });
}

/** References crossing the cut at `c` within the scoring window. */
function boundaryScore(
  refs: Array<Set<number>>,
  c: number,
  window: number
): number {
  const lo = Math.max(0, c - window);
  const hi = Math.min(refs.length, c + window);
  let crossing = 0;
  for (let j = c; j < hi; j++) {
    for (const r of refs[j]) if (r >= lo && r < c) crossing++;
  }
  for (let i = lo; i < c; i++) {
    for (const r of refs[i]) if (r >= c && r < hi) crossing++;
  }
  return crossing;
}

function stmtLineCounts(body: t.Statement[]): number[] {
  return body.map((s) => (s.loc ? s.loc.end.line - s.loc.start.line + 1 : 1));
}

/** Furthest segment end from `start` under both budgets. */
function segmentReach(
  start: number,
  lineCounts: number[],
  budgets: StableSplitBudgets
): number {
  let end = start + 1;
  let lines = lineCounts[start];
  while (
    end < lineCounts.length &&
    end - start < budgets.maxSeg &&
    lines + lineCounts[end] <= budgets.maxLines
  ) {
    lines += lineCounts[end];
    end++;
  }
  return end;
}

/** Greedy segmentation under both budgets: cut at the least-cohesive
 * position in the allowed range (leftmost minimum — deterministic). */
function segmentBoundaries(
  refs: Array<Set<number>>,
  lineCounts: number[],
  budgets: StableSplitBudgets
): number[] {
  const cuts: number[] = [];
  let start = 0;
  while (start < refs.length) {
    const end = segmentReach(start, lineCounts, budgets);
    if (end >= refs.length) break;
    let bestCut = end;
    let bestScore = Number.POSITIVE_INFINITY;
    for (let c = Math.min(start + budgets.minSeg, end); c <= end; c++) {
      const score = boundaryScore(refs, c, budgets.window);
      if (score < bestScore) {
        bestScore = score;
        bestCut = c;
      }
    }
    cuts.push(bestCut);
    start = bestCut;
  }
  return cuts;
}

/** Inbound references per statement of [segStart, segEnd), from outside. */
function inboundCounts(
  refs: Array<Set<number>>,
  segStart: number,
  segEnd: number
): Map<number, number> {
  const inbound = new Map<number, number>();
  for (let i = 0; i < refs.length; i++) {
    if (i >= segStart && i < segEnd) continue;
    for (const r of refs[i]) {
      if (r >= segStart && r < segEnd) {
        inbound.set(r, (inbound.get(r) ?? 0) + 1);
      }
    }
  }
  return inbound;
}

/** Prefer function/class stems over var noise for near-tied counts. */
function betterStem(
  candidate: { count: number; isFnClass: boolean },
  best: { count: number; isFnClass: boolean } | null
): boolean {
  if (!best) return true;
  if (candidate.isFnClass === best.isFnClass) {
    return candidate.count > best.count;
  }
  return candidate.isFnClass
    ? candidate.count * 2 >= best.count
    : candidate.count > best.count * 2;
}

/** Segment stem: its most externally-referenced non-placeholder binding. */
function segmentStem(
  body: t.Statement[],
  refs: Array<Set<number>>,
  segStart: number,
  segEnd: number
): string {
  const inbound = inboundCounts(refs, segStart, segEnd);
  let best: { idx: number; count: number; isFnClass: boolean } | null = null;
  for (let i = segStart; i < segEnd; i++) {
    const names = declaredNames(body[i]);
    if (names.length === 0 || BAD_STEM.test(names[0])) continue;
    const candidate = {
      idx: i,
      count: inbound.get(i) ?? 0,
      isFnClass:
        t.isFunctionDeclaration(body[i]) || t.isClassDeclaration(body[i])
    };
    if (betterStem(candidate, best)) best = candidate;
  }
  const idx = best?.idx ?? segStart;
  return declaredNames(body[idx])[0] ?? `segment_${segStart}`;
}

function uniqueName(stem: string, ext: string, used: Set<string>): string {
  let name = `${stem}${ext}`;
  for (let k = 2; used.has(name); k++) name = `${stem}-${k}${ext}`;
  used.add(name);
  return name;
}

/** Folder boundaries: coarser greedy segmentation over file boundaries. */
function folderBoundaries(
  refs: Array<Set<number>>,
  fileCuts: number[],
  budgets: StableSplitBudgets
): number[] {
  const cuts: number[] = [];
  let start = 0;
  const segCount = fileCuts.length - 1;
  while (start < segCount) {
    const end = Math.min(start + budgets.maxFolder, segCount);
    if (end >= segCount) break;
    let bestCut = Math.min(start + budgets.minFolder, end);
    let bestScore = Number.POSITIVE_INFINITY;
    for (let f = Math.min(start + budgets.minFolder, end); f <= end; f++) {
      const score = boundaryScore(refs, fileCuts[f], budgets.window);
      if (score < bestScore) {
        bestScore = score;
        bestCut = f;
      }
    }
    cuts.push(bestCut);
    start = bestCut;
  }
  return cuts;
}

/** Assign one folder's segments to named files. */
function assignFolder(
  body: t.Statement[],
  refs: Array<Set<number>>,
  fileCuts: number[],
  firstSeg: number,
  lastSeg: number,
  folder: string,
  assignment: string[]
): void {
  const usedFiles = new Set<string>();
  for (let s = firstSeg; s < lastSeg; s++) {
    const stem = segmentStem(body, refs, fileCuts[s], fileCuts[s + 1]);
    const file = `${folder}/${uniqueName(stem, ".js", usedFiles)}`;
    for (let i = fileCuts[s]; i < fileCuts[s + 1]; i++) {
      assignment[i] = file;
    }
  }
}

/** Fresh grouping: boundary-detected files inside boundary-detected
 * folders, both named from their most-public binding. */
function assignFresh(
  body: t.Statement[],
  budgets: StableSplitBudgets
): string[] {
  const refs = referenceIndices(body);
  const lineCounts = stmtLineCounts(body);
  const fileCuts = [
    0,
    ...segmentBoundaries(refs, lineCounts, budgets),
    body.length
  ];
  const folderCuts = [
    0,
    ...folderBoundaries(refs, fileCuts, budgets),
    fileCuts.length - 1
  ];

  const assignment: string[] = new Array(body.length);
  const usedFolders = new Set<string>();
  for (let d = 0; d < folderCuts.length - 1; d++) {
    const firstSeg = folderCuts[d];
    const lastSeg = folderCuts[d + 1];
    const stem = segmentStem(body, refs, fileCuts[firstSeg], fileCuts[lastSeg]);
    const folder = uniqueName(stem, "", usedFolders);
    assignFolder(body, refs, fileCuts, firstSeg, lastSeg, folder, assignment);
  }
  return assignment;
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

/** Slice each statement's exact source text and group into files. */
function emitFiles(
  body: t.Statement[],
  assignment: string[],
  code: string
): Map<string, string[]> {
  const byFile = new Map<string, string[]>();
  for (let i = 0; i < body.length; i++) {
    const { start, end } = body[i];
    if (start == null || end == null) {
      throw new Error(`statement ${i} is missing byte offsets`);
    }
    const parts = byFile.get(assignment[i]) ?? [];
    parts.push(code.slice(start, end));
    byFile.set(assignment[i], parts);
  }
  return byFile;
}

function buildLedger(
  body: t.Statement[],
  assignment: string[],
  files: string[]
): StableSplitLedger {
  const nameFiles = new Map<string, string[]>();
  for (let i = 0; i < body.length; i++) {
    for (const n of declaredNames(body[i])) {
      const list = nameFiles.get(n) ?? [];
      list.push(assignment[i]);
      nameFiles.set(n, list);
    }
  }
  return {
    version: 1,
    files,
    nameToFiles: Object.fromEntries(nameFiles),
    order: assignment
  };
}

/**
 * Split a rendered bundle into a stable folder/file tree. Returns null
 * when the code is not a single wrapper IIFE (the caller falls back to
 * the legacy splitter). Parses privately so byte offsets always align
 * with the given text.
 */
export function stableSplitFromCode(
  code: string,
  options: StableSplitOptions = {}
): StableSplitResult | null {
  const ast = parseSync(code, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  }) as t.File | null;
  if (!ast) return null;
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) return null;
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode)) return null;
  const body = bodyNode.body;
  if (body.length < 2) return null;

  const budgets = { ...DEFAULT_BUDGETS, ...options.budgets };
  let assignment: string[];
  let transfer: TransferOutcome["stats"] | undefined;
  if (options.prior) {
    ({ assignment, stats: transfer } = assignWithPrior(body, options.prior));
  } else {
    assignment = assignFresh(body, budgets);
  }

  const byFile = emitFiles(body, assignment, code);
  const fileContents = new Map<string, string>();
  for (const [file, parts] of byFile) {
    fileContents.set(file, `${parts.join("\n")}\n`);
  }
  const files = [...byFile.keys()].sort();
  const ledger = buildLedger(body, assignment, files);
  const folders = new Set(files.map((f) => f.split("/")[0]));

  return {
    fileContents,
    ledger,
    stats: {
      statements: body.length,
      files: files.length,
      folders: folders.size,
      inherited: transfer?.inherited ?? 0,
      inheritedViaOrdinal: transfer?.inheritedViaOrdinal ?? 0,
      conflictDisagree: transfer?.conflictDisagree ?? 0,
      noVote: transfer?.noVote ?? 0,
      residueLocality: transfer?.residueLocality ?? 0
    }
  };
}
