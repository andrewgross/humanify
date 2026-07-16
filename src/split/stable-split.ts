/**
 * Stable split: statement-level splitting of a Bun wrapper bundle into
 * folders/files whose assignments PERSIST across releases (exp023).
 *
 * The bundle's app code is one wrapper IIFE whose body statements preserve
 * the original emission order (renaming is pure). Two regimes:
 *
 *   - FRESH (no prior ledger — the first split release): the clustered
 *     grouping (cluster-assign.ts, exp029). Whole vendored libraries (Bun
 *     CJS factories) are set aside in vendor/; the app statements are cut
 *     at their reference-graph SEAMS into a size-balanced nested folder
 *     tree under src/, each level named after its dominant binding
 *     (LLM-polished when a namer is given). Order-respecting (files are
 *     contiguous runs), so the prior-carried regime below stays the
 *     correct stability mechanism.
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
 * (`.humanify/split-ledger.json`) records the full statement order for
 * reconstruction and for the NEXT release's inheritance.
 */

import * as t from "@babel/types";
import type { WrapperFunctionResult } from "../analysis/wrapper-detection.js";
import { findWrapperFunction } from "../analysis/wrapper-detection.js";
import { parseFileAst } from "../babel-utils.js";
import { type ClusterConfig, assignClustered } from "./cluster-assign.js";

/** Stems that make bad file names (placeholder/minted-ish/decorated).
 * The noop/doNothing/empty-stub families are the minted names the LLM
 * gives tree-shaken stub modules — they leaked into real trees as
 * directory names (noopFunction36/, doNothing24/). */
const BAD_STEM =
  /^(no[-_]?ops?\w*|doNothing\w*|silent[-_]?noops?\w*|empty(function|callback|operation|handler)s?\d*|idle[-_]?operation\d*|initializeModule\d+|placeholder\w*|_+\d*|reactLib\d+|\w+Val\d*)$/i;

/** Digit runs that are a real part of a technical name, not a minted
 * disambiguator: bit widths, hash sizes, versions. */
const KNOWN_NUMBER_TOKENS = new Set([
  "8",
  "16",
  "32",
  "64",
  "128",
  "256",
  "512",
  "1024"
]);

/** True when a name carries a minted numeric disambiguator — a run of 2+
 * digits that is NOT a known unit token (appInitializer17, app254Initializer
 * are minted; float64Error, sha256Hasher, base64Encode are real). The
 * rename step appends these counters to near-identical modules; they must
 * never ride into a file/folder name. */
function hasMintedNumber(name: string): boolean {
  const runs = name.match(/\d+/g);
  if (!runs) return false;
  return runs.some((run) => run.length >= 2 && !KNOWN_NUMBER_TOKENS.has(run));
}

/** Leading conjunction/article — never the first word of a real module
 * name (`andTaskPipeline`, `theTaskRunner`). Matched on the first
 * camelCase token so `inputHandler`/`themeEngine`/`andrewConfig` (which
 * only PREFIX these words) and predicates (`isReverseDirection`) survive. */
const LEADING_STOPWORD = /^(and|or|but|nor|the|an|a)(?=[A-Z0-9]|$)/;

/** camelCase / PascalCase / acronym / mixed → kebab-case, the src/ tree's
 * file+folder convention (FS-safe on case-insensitive filesystems).
 * Vendor package names are NOT run through this — they are real npm names.
 * Exported for the clustered path assembly and unit tests. */
export function toKebabCase(name: string): string {
  return name
    .replace(/([a-z0-9])([A-Z])/g, "$1-$2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1-$2")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

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
export function acceptProposedName(name: string): string | null {
  if (!/^[A-Za-z_$][A-Za-z0-9_$-]{1,39}$/.test(name)) return null;
  const camel = toCamelCase(name);
  if (GENERIC_NAMES.has(camel.toLowerCase())) return null;
  if (isRejectedStem(camel)) return null;
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
  /** For folders: which tree level — top-level folders deserve short
   * domain nouns (auth, tools), and the prompt says so. */
  level?: "top" | "sub";
}

/** Batch namer: a whole sibling scope arrives as ONE call (the top level
 * is a single joint batch), returning one proposal or null per request,
 * in request order. Naming runs bottom-up — files first, so folder
 * requests carry their members' polished names as evidence. */
export type SplitNamer = (
  requests: SplitNameRequest[]
) => Promise<Array<string | null>>;

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
  /** The wrapper parsed from the input (offsets align with the code passed
   * in). Handed to emitRunnableCjs to avoid re-parsing the same string. */
  wrapper: WrapperFunctionResult;
}

export interface StableSplitOptions {
  prior?: StableSplitLedger;
  /** Optional namer for NEW files/folders (fresh grouping only). */
  namer?: SplitNamer;
  /** Clustering knobs (fresh grouping only); tests inject small ones. */
  clusterConfig?: Partial<ClusterConfig>;
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
 * boundary score tolerates. Exported for the split-quality metric harness
 * (experiments/029) so it scores the exact graph the splitter sees. */
export function referenceIndices(body: t.Statement[]): Array<Set<number>> {
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

/** A binding that must never become a file/folder stem: minted/decorated
 * (BAD_STEM), a minted numeric disambiguator, or a leading conjunction.
 * The single predicate both the mechanical stem picker and the LLM-proposal
 * validator use, so a bad name is blocked whichever produced it. */
function isRejectedStem(name: string): boolean {
  return (
    BAD_STEM.test(name) || hasMintedNumber(name) || LEADING_STOPWORD.test(name)
  );
}

/** Segment stem: its most externally-referenced non-placeholder binding.
 * Exported for the clustered assignment (cluster-assign.ts) so it names
 * files/folders the same way the budget path does. */
export function segmentStem(
  body: t.Statement[],
  refs: Array<Set<number>>,
  segStart: number,
  segEnd: number
): string {
  const inbound = inboundCounts(refs, segStart, segEnd);
  let best: { idx: number; count: number; isFnClass: boolean } | null = null;
  for (let i = segStart; i < segEnd; i++) {
    const names = declaredNames(body[i]);
    if (names.length === 0 || isRejectedStem(names[0])) continue;
    const candidate = {
      idx: i,
      count: inbound.get(i) ?? 0,
      isFnClass:
        t.isFunctionDeclaration(body[i]) || t.isClassDeclaration(body[i])
    };
    if (betterStem(candidate, best)) best = candidate;
  }
  if (best) {
    return declaredNames(body[best.idx])[0] ?? `segment_${segStart}`;
  }
  // Every named candidate was minted/banned (a stub run): "stubs" is what
  // a human calls that file — never leak a banned name into the tree.
  for (let i = segStart; i < segEnd; i++) {
    if (declaredNames(body[i]).length > 0) return "stubs";
  }
  return `segment_${segStart}`;
}

/** Top declared bindings of a segment, inbound-weighted, for namer
 * prompts: "function handleMessage (12 refs)". Exported for the clustered
 * assignment (cluster-assign.ts). */
export function segmentBindings(
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
 * (e.g. a runnable tree with its require headers and accessor footers) —
 * which is the invariant firing.
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

/** The concat-equivalence guarantee, ENFORCED on every run before the
 * tree is returned: replaying the just-emitted tree through the ledger
 * must rebuild every wrapper-body statement byte-identically, in order.
 * A mismatch is an internal invariant violation — throw so the caller
 * falls back loudly rather than shipping a silently broken split. */
function assertConcatEquivalence(
  fileContents: Map<string, string>,
  ledger: StableSplitLedger,
  body: t.Statement[],
  code: string
): void {
  const rebuilt = reconstructBody(fileContents, ledger);
  const expected = body
    .map((s) => {
      if (s.start == null || s.end == null) {
        throw new Error("stable split: statement missing offsets");
      }
      return code.slice(s.start, s.end);
    })
    .join("\n");
  if (rebuilt !== expected) {
    throw new Error(
      "stable split: emitted tree does not reconstruct the source statements (tree/ledger invariant violated)"
    );
  }
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

  let assignment: string[];
  let transfer: TransferOutcome["stats"] | undefined;
  if (options.prior) {
    ({ assignment, stats: transfer } = assignWithPrior(body, options.prior));
  } else {
    // Fresh grouping (release 1): seam-clustered nested tree, libraries aside.
    assignment = await assignClustered(body, {
      namer: options.namer,
      config: options.clusterConfig,
      code
    });
  }

  const byFile = emitFiles(body, assignment, code);
  const fileContents = new Map<string, string>();
  for (const [file, parts] of byFile) {
    fileContents.set(file, `${parts.join("\n")}\n`);
  }
  const files = [...byFile.keys()].sort();
  const ledger = buildLedger(body, assignment, files);
  assertConcatEquivalence(fileContents, ledger, body, code);
  // Distinct parent directories (paths are nested: src/<top>/<sub>/<file>).
  const folders = new Set(
    files.map((f) => (f.includes("/") ? f.slice(0, f.lastIndexOf("/")) : ""))
  );

  return {
    fileContents,
    ledger,
    wrapper,
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
