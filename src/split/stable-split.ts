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

import * as t from "@babel/types";
import { findWrapperFunction } from "../analysis/wrapper-detection.js";
import { parseFileAst } from "../babel-utils.js";

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

/** Stems that make bad file names (placeholder/minted-ish/decorated). */
const BAD_STEM =
  /^(noop\d*|initializeModule\d+|placeholder\w*|_+\d*|reactLib\d+|\w+Val\d*)$/i;

/** Names too generic to be a file/folder name — a specific-but-imperfect
 * mechanical stem beats these (exp024 smoke-probe failure mode). */
const GENERIC_NAMES = new Set([
  "utils",
  "util",
  "helpers",
  "helper",
  "misc",
  "core",
  "common",
  "lib",
  "libs",
  "main",
  "index",
  "shared",
  "module",
  "modules",
  "code",
  "src",
  "functions"
]);

/** kebab/snake → camelCase so the whole tree uses one convention
 * regardless of which the model returned. */
function toCamelCase(name: string): string {
  return name.replace(/[-_]+([A-Za-z0-9])/g, (_m, ch: string) =>
    ch.toUpperCase()
  );
}

/** Validate a namer proposal and normalize it to camelCase, or null when
 * it is not identifier-ish, is generic, or is minted/placeholder-shaped.
 * Shape checks run on the normalized form so a kebab spelling of a bad
 * name (`react-lib-48`) is caught too. */
function acceptProposedName(name: string): string | null {
  if (!/^[A-Za-z_$][A-Za-z0-9_$-]{1,39}$/.test(name)) return null;
  const camel = toCamelCase(name);
  if (GENERIC_NAMES.has(camel.toLowerCase())) return null;
  if (BAD_STEM.test(camel)) return null;
  return camel;
}

/**
 * Optional namer for NEW files/folders (exp024). Called only on the
 * fresh-grouping path — inherited paths never rename (renames are churn).
 * Returning null keeps the mechanical stem. The proposal is validated by
 * acceptProposedName; naming-only by construction (the namer never sees
 * or edits code placement).
 */
export interface SplitNameRequest {
  kind: "file" | "folder";
  mechanicalStem: string;
  /** Sibling stems in the same folder (files) or folder stems (folders). */
  siblings: string[];
  /** Top declared bindings, inbound-reference weighted. */
  bindings: string[];
  /** For folders: the (already-named) member file stems. */
  members?: string[];
}

export type SplitNamer = (request: SplitNameRequest) => Promise<string | null>;

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
  /** Optional namer for NEW files/folders (fresh grouping only). */
  namer?: SplitNamer;
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

/** Top declared bindings of a segment, inbound-weighted, for namer
 * prompts: "function handleMessage (12 refs)". */
function segmentBindings(
  body: t.Statement[],
  refs: Array<Set<number>>,
  segStart: number,
  segEnd: number,
  limit: number
): string[] {
  const inbound = inboundCounts(refs, segStart, segEnd);
  const rows: Array<{ name: string; kind: string; count: number }> = [];
  for (let i = segStart; i < segEnd; i++) {
    const names = declaredNames(body[i]);
    if (names.length === 0) continue;
    const kind = t.isFunctionDeclaration(body[i])
      ? "function"
      : t.isClassDeclaration(body[i])
        ? "class"
        : "var";
    rows.push({ name: names[0], kind, count: inbound.get(i) ?? 0 });
  }
  rows.sort((a, b) => b.count - a.count);
  return rows
    .slice(0, limit)
    .map((r) => `${r.kind} ${r.name} (${r.count} refs)`);
}

interface FreshSegment {
  seg: number;
  stem: string;
}

interface FreshFolder {
  firstSeg: number;
  lastSeg: number;
  stem: string;
  files: FreshSegment[];
}

/** Ask the namer for a better stem; keep the mechanical one on decline,
 * invalid, or generic proposals (skip, never force). */
async function maybeRename(
  namer: SplitNamer,
  request: SplitNameRequest
): Promise<string> {
  const proposal = await namer(request);
  const accepted = proposal ? acceptProposedName(proposal) : null;
  return accepted ?? request.mechanicalStem;
}

/** Optional naming pass over fresh folders/files: files first (their
 * final stems feed the folder prompts), then folders. */
async function renameFresh(
  folders: FreshFolder[],
  namer: SplitNamer,
  body: t.Statement[],
  refs: Array<Set<number>>,
  fileCuts: number[]
): Promise<void> {
  for (const folder of folders) {
    await Promise.all(
      folder.files.map(async (file) => {
        file.stem = await maybeRename(namer, {
          kind: "file",
          mechanicalStem: file.stem,
          siblings: folder.files.filter((f) => f !== file).map((f) => f.stem),
          bindings: segmentBindings(
            body,
            refs,
            fileCuts[file.seg],
            fileCuts[file.seg + 1],
            12
          )
        });
      })
    );
  }
  await Promise.all(
    folders.map(async (folder) => {
      folder.stem = await maybeRename(namer, {
        kind: "folder",
        mechanicalStem: folder.stem,
        siblings: folders.filter((f) => f !== folder).map((f) => f.stem),
        bindings: segmentBindings(
          body,
          refs,
          fileCuts[folder.firstSeg],
          fileCuts[folder.lastSeg],
          10
        ),
        members: folder.files.map((f) => f.stem)
      });
    })
  );
}

/** Fresh grouping: boundary-detected files inside boundary-detected
 * folders, named from their most-public binding (optionally polished by
 * the namer). */
async function assignFresh(
  body: t.Statement[],
  budgets: StableSplitBudgets,
  namer?: SplitNamer
): Promise<string[]> {
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

  const folders: FreshFolder[] = [];
  for (let d = 0; d < folderCuts.length - 1; d++) {
    const firstSeg = folderCuts[d];
    const lastSeg = folderCuts[d + 1];
    const files: FreshSegment[] = [];
    for (let s = firstSeg; s < lastSeg; s++) {
      files.push({
        seg: s,
        stem: segmentStem(body, refs, fileCuts[s], fileCuts[s + 1])
      });
    }
    folders.push({
      firstSeg,
      lastSeg,
      stem: segmentStem(body, refs, fileCuts[firstSeg], fileCuts[lastSeg]),
      files
    });
  }

  if (namer) await renameFresh(folders, namer, body, refs, fileCuts);

  const assignment: string[] = new Array(body.length);
  const usedFolders = new Set<string>();
  for (const folder of folders) {
    const folderName = uniqueName(folder.stem, "", usedFolders);
    const usedFiles = new Set<string>();
    for (const file of folder.files) {
      const path = `${folderName}/${uniqueName(file.stem, ".js", usedFiles)}`;
      for (let i = fileCuts[file.seg]; i < fileCuts[file.seg + 1]; i++) {
        assignment[i] = path;
      }
    }
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

/** Byte-slice one emitted file back into its statement texts. A leading
 * bare-string statement re-parses into program.directives, so directives
 * and body are merged in source order — both are wrapper-body statements
 * to the ledger. */
function fileStatementSlices(file: string, content: string): string[] {
  const ast = parseFileAst(content);
  if (!ast) throw new Error(`reconstruct: ${file} failed to parse`);
  const nodes: Array<t.Statement | t.Directive> = [
    ...ast.program.directives,
    ...ast.program.body
  ];
  nodes.sort((a, b) => (a.start ?? 0) - (b.start ?? 0));
  return nodes.map((s) => {
    if (s.start == null || s.end == null) {
      throw new Error(`reconstruct: ${file} statement missing offsets`);
    }
    return content.slice(s.start, s.end);
  });
}

/**
 * Reconstruct the wrapper-body statement sequence from an emitted tree +
 * its ledger — the concat-equivalence guarantee (exp025). Each file's
 * statements are re-sliced by re-parsing (exact bytes, no generator
 * drift); `order` replays which file each statement came from, so the
 * per-file FIFO cursors rebuild the original statement order. The result
 * is every statement exactly once, in order, byte-identical — a pure
 * reformat of the original body (indentation aside). Wrapping it back in
 * the IIFE yields a runnable single file semantically identical to the
 * input. Throws whenever the tree and ledger disagree IN EITHER
 * DIRECTION — a file short of the statements `order` expects, a file
 * holding statements beyond them, or a file the ledger does not know
 * (e.g. a --split-runnable tree with its require headers and accessor
 * footers) — which is the invariant firing.
 */
export function reconstructBody(
  fileContents: Map<string, string>,
  ledger: StableSplitLedger
): string {
  const partsByFile = new Map<string, string[]>();
  for (const [file, content] of fileContents) {
    partsByFile.set(file, fileStatementSlices(file, content));
  }
  const cursor = new Map<string, number>();
  const ordered: string[] = [];
  for (const file of ledger.order) {
    const parts = partsByFile.get(file);
    const at = cursor.get(file) ?? 0;
    if (!parts || at >= parts.length) {
      throw new Error(`reconstruct: ${file} is short of statement ${at}`);
    }
    ordered.push(parts[at]);
    cursor.set(file, at + 1);
  }
  for (const [file, parts] of partsByFile) {
    const consumed = cursor.get(file) ?? 0;
    if (consumed !== parts.length) {
      throw new Error(
        `reconstruct: ${file} has ${parts.length - consumed} statement(s) beyond the ledger`
      );
    }
  }
  return ordered.join("\n");
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
export async function stableSplitFromCode(
  code: string,
  options: StableSplitOptions = {}
): Promise<StableSplitResult | null> {
  const ast = parseFileAst(code);
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
    assignment = await assignFresh(body, budgets, options.namer);
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
