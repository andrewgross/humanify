/**
 * Diff-guided reconciliation: collapse cross-version rename noise.
 *
 * Every upstream matching mechanism (exact match, close match, votes,
 * context stages) works per-function or per-binding, blind to where a
 * binding sits in the rendered file. The plaintext diff between the prior
 * humanified output and the new one carries a signal none of them see:
 * LCS alignment anchored on identical neighboring lines. A change hunk
 * whose sides are identical after blanking identifier tokens is rename
 * noise — structurally the same code, different names — and each differing
 * identifier position is a proposal to snap the new leg's name back to the
 * prior leg's.
 *
 * Safety design (precision over recall — a wrong name applied confidently
 * is worse than a minified leftover):
 *
 *   - The unit of decision is the Babel BINDING, never the text position.
 *     Every differing position must resolve through scope to a binding's
 *     declaration, reference, or write; positions that resolve to property
 *     names, object keys, or free identifiers taint their whole hunk.
 *   - All of a binding's proposals must agree on ONE prior name
 *     (unanimity); disagreement means the alignment slipped.
 *   - Every occurrence of the binding must sit on a diff-covered line —
 *     renaming a binding with occurrences on unchanged lines would CREATE
 *     new diff hunks (and signals the evidence is an alignment artifact).
 *   - The declaration must itself sit in a clean aligned rename-noise pair
 *     (every tier): a binding's declaration always contains its name, so a
 *     genuinely-changed declaration line means the alignment can't be
 *     trusted. Export-involved bindings are skipped (Babel's renamer would
 *     split the export declaration, creating hunks).
 *   - Renames are tiered by what they overwrite: minified → descriptive
 *     (asymmetric) overwrites nothing meaningful; descriptive →
 *     descriptive additionally requires the declaration's OTHER differing
 *     tokens to be already-reconciled bindings (evidence the value is
 *     computed the same way); renaming TO a minified name is never useful
 *     (reroll / downgrade).
 *   - When the prior text is too dissimilar to be the same file (corpus
 *     gate), the pass abstains — aligned pairs would be coincidence.
 *   - Application goes through attemptValidatedRename — capture, reserved
 *     words, collisions, and shadowing are rejected there, and rejection
 *     means skip, never force.
 *
 * The module is deterministic and LLM-free. It reconciles two specific
 * rendered outputs — it makes THIS diff reviewable, it does not improve
 * the naming lineage going forward.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import type { Binding, NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import {
  collectEvalWithTaint,
  type EvalWithTaint,
  isBindingEvalTaintFrozen
} from "../analysis/soundness.js";
import { traverse, violationWriteTargetPaths } from "../babel-utils.js";
import { isHalfMintHead, isWordlessMintShape } from "./minted-census.js";
import { createIsEligible, type IsEligibleFn } from "./rename-eligibility.js";
import { strategyTrail } from "./strategy-trail.js";
import {
  attemptValidatedRename,
  getRenameRejection,
  isExportInvolved
} from "./validated-rename.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface ReconcileOptions {
  /** Mutate the AST via attemptValidatedRename; false = dry-run dump. */
  apply: boolean;
  /**
   * Enable descriptive → descriptive renames (transfer-gap tier). Off by
   * default: both names are deliberate, so the extra declaration-clean
   * gate must hold before we trust the diff alignment.
   */
  descriptiveTier: boolean;
  /** Change hunks with more lines than this are never candidates. */
  maxHunkLines: number;
  /** Eligibility of the CURRENT (new-leg) name; skip-listed names stay. */
  isEligible: IsEligibleFn;
  /**
   * Total line count of the prior text. When provided and the file is
   * large enough to judge, the pass abstains entirely if too few prior
   * lines survive unchanged — the shared-lineage premise (the two legs are
   * ~95% identical) does not hold, so aligned pairs would be coincidence
   * (e.g. a bundle unpacked to many files, each diffed against one prior
   * file). Undefined disables the corpus gate.
   */
  priorLineCount?: number;
  /**
   * Enable the consumer tier: a binding whose DECLARATION genuinely
   * changed (changed-leaf — aligned-declaration proof can never hold) may
   * still inherit its prior name when unchanged consumers testify from
   * ≥2 distinct clean rename-noise hunks, the prior name has exactly one
   * claimant and is dead in the new output, and the fresh name is novel
   * this hop. Requires `priorNames`.
   */
  consumerTier: boolean;
  /** Word tokens of the prior text, for the consumer tier's novelty gate. */
  priorNames?: ReadonlySet<string>;
}

/** Below this prior size the corpus-similarity gate is not meaningful. */
const MIN_CORPUS_LINES = 8;
/** Fraction of prior lines that must survive unchanged to trust alignment. */
const MIN_CORPUS_SIMILARITY = 0.5;

export type RenameKind = "asymmetric" | "descriptive" | "consumer";

/** All word-shaped tokens of a text — the consumer tier's name censuses. */
export function collectWordTokens(text: string): Set<string> {
  return new Set(text.match(/[A-Za-z_$][\w$]*/g) ?? []);
}

/** Every Identifier name in the AST (bindings, references, and property
 * keys alike — over-approximating keeps the liveness gate conservative). */
function collectIdentifierNames(ast: t.File): Set<string> {
  const names = new Set<string>();
  traverse(ast, {
    Identifier(pathArg: NodePath<t.Identifier>) {
      names.add(pathArg.node.name);
    }
  });
  return names;
}

export interface ReconcileRename {
  fromName: string;
  toName: string;
  /** Corroborating differing positions across all rename-noise hunks. */
  votes: number;
  kind: RenameKind;
  /** 1-based line (new output) of the binding's declaration identifier. */
  declLine: number;
  /** True when the rename was applied to the AST (always false in dry-run). */
  applied: boolean;
}

export interface ReconcileSkip {
  fromName: string;
  toName: string;
  reason: string;
  votes: number;
}

export interface ReconcileHunkStats {
  /** Change (c) hunks in the diff. */
  changed: number;
  /** Rename-noise hunks that produced candidates. */
  noise: number;
  /** Hunks with any difference beyond identifier tokens — never touched. */
  genuine: number;
  /** Norm-clean hunks skipped for exceeding maxHunkLines. */
  oversized: number;
  /** Norm-clean hunks with a differing position that failed to resolve
   * to a local binding (property/key/free/quasi) — never touched. */
  tainted: number;
}

export interface ReconcileResult {
  renames: ReconcileRename[];
  skipped: ReconcileSkip[];
  /** True when the corpus gate abstained (prior text too dissimilar). */
  priorTooDissimilar?: boolean;
  hunks: ReconcileHunkStats;
}

// ---------------------------------------------------------------------------
// System diff (normal format)
// ---------------------------------------------------------------------------

const DIFF_MAX_BUFFER = 512 * 1024 * 1024;

/**
 * Line-diff two texts with the system `diff` (normal format). An
 * in-process Myers diff chokes on 370k-line bundles; the two legs are
 * ~95% identical so `diff` is fast.
 *
 * Both texts are CRLF-normalized first: a prior file checked out with
 * autocrlf against LF babel-generator output would otherwise differ on
 * every line (trailing \r), collapsing the whole file into non-noise.
 * Throws on a genuine `diff` failure (missing binary, oversized output);
 * callers treat this optional pass's failure as skip, not fatal.
 */
export function computeNormalDiff(priorText: string, newText: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "humanify-reconcile-"));
  try {
    const priorPath = path.join(dir, "prior.js");
    const newPath = path.join(dir, "new.js");
    fs.writeFileSync(priorPath, priorText.replace(/\r\n/g, "\n"));
    fs.writeFileSync(newPath, newText.replace(/\r\n/g, "\n"));
    const proc = spawnSync("diff", [priorPath, newPath], {
      encoding: "utf-8",
      maxBuffer: DIFF_MAX_BUFFER
    });
    // diff exits 0 (identical) or 1 (differences); anything else — including
    // a spawn failure (status null, e.g. no `diff` on PATH) — is an error.
    if (proc.status !== 0 && proc.status !== 1) {
      const detail = proc.error?.message || proc.stderr || "unknown error";
      throw new Error(`diff failed (status ${proc.status}): ${detail}`);
    }
    return proc.stdout;
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

export interface DiffHunk {
  op: "a" | "c" | "d";
  /** 1-based first line of the hunk in the prior text. */
  priorStart: number;
  /** 1-based first line of the hunk in the new text. */
  newStart: number;
  priorLines: string[];
  newLines: string[];
}

const HUNK_HEADER = /^(\d+)(?:,\d+)?([acd])(\d+)(?:,\d+)?$/;

function parseHunkHeader(line: string): DiffHunk | null {
  const header = HUNK_HEADER.exec(line);
  if (!header) return null;
  return {
    op: header[2] as DiffHunk["op"],
    priorStart: Number(header[1]),
    newStart: Number(header[3]),
    priorLines: [],
    newLines: []
  };
}

/** Content of a `< `/`> ` hunk line, or null when it is not one. */
function hunkContent(line: string, marker: "<" | ">"): string | null {
  if (line === marker) return "";
  if (line.startsWith(`${marker} `)) return line.slice(2);
  return null;
}

/** Parse normal-format `diff` output into hunks. */
export function parseNormalDiff(diffText: string): DiffHunk[] {
  const hunks: DiffHunk[] = [];
  let current: DiffHunk | null = null;
  for (const line of diffText.split("\n")) {
    const started = parseHunkHeader(line);
    if (started) {
      current = started;
      hunks.push(current);
      continue;
    }
    // "---" separators and "\ No newline..." markers fall through both
    if (!current) continue;
    const prior = hunkContent(line, "<");
    if (prior !== null) {
      current.priorLines.push(prior);
      continue;
    }
    const added = hunkContent(line, ">");
    if (added !== null) current.newLines.push(added);
  }
  return hunks;
}

// ---------------------------------------------------------------------------
// Line tokenizer
// ---------------------------------------------------------------------------

export interface LineToken {
  kind: "ident" | "text";
  text: string;
  /** 0-based column of the token's first character. */
  col: number;
}

const IDENT_START = /[A-Za-z_$]/;
const IDENT_CONT = /[A-Za-z0-9_$]/;
const NUMBER_CONT = /[0-9A-Za-z_$.]/;

/**
 * Words kept verbatim so a keyword change (`return` → `throw`) reads as a
 * genuine change. Contextual keywords (async/of/get/set/static/let) are
 * included: treating them as opaque only costs recall, never precision.
 */
const RESERVED_TOKEN_WORDS = new Set([
  ..."break case catch class const continue debugger default delete do else enum export extends false finally for function if import in instanceof new null return super switch this throw true try typeof var void while with yield".split(
    " "
  ),
  "let",
  "static",
  "async",
  "await",
  "of",
  "get",
  "set"
]);

/** Tokens after which a `/` is division, not the start of a regex. */
const DIVISION_PRECEDERS = new Set([
  ")",
  "]",
  "this",
  "true",
  "false",
  "null",
  "super"
]);

interface TokenizerState {
  line: string;
  i: number;
  tokens: LineToken[];
  /** Open template contexts: "quasi" = inside template text, a number =
   * brace depth of a `${}` expression. Empty = top-level code. */
  frames: Array<"quasi" | number>;
  /** Last non-whitespace token, for the regex/division heuristic. */
  prev: LineToken | null;
  failed: boolean;
}

function emit(
  st: TokenizerState,
  kind: LineToken["kind"],
  start: number
): void {
  const token = { kind, text: st.line.slice(start, st.i), col: start };
  st.tokens.push(token);
  if (token.text.trim().length > 0) st.prev = token;
}

function scanSimpleString(st: TokenizerState, quote: string): void {
  const start = st.i;
  st.i++;
  while (st.i < st.line.length) {
    const ch = st.line[st.i];
    if (ch === "\\") {
      st.i += 2;
      continue;
    }
    if (ch === quote) {
      st.i++;
      emit(st, "text", start);
      return;
    }
    st.i++;
  }
  st.failed = true; // unterminated (or line-continuation) — not self-contained
}

function scanQuasi(st: TokenizerState): void {
  const start = st.i;
  while (st.i < st.line.length) {
    const ch = st.line[st.i];
    if (ch === "\\") {
      st.i += 2;
      continue;
    }
    if (ch === "`") {
      st.i++;
      emit(st, "text", start);
      st.frames.pop();
      return;
    }
    if (ch === "$" && st.line[st.i + 1] === "{") {
      st.i += 2;
      emit(st, "text", start);
      st.frames.push(0);
      return;
    }
    st.i++;
  }
  st.failed = true; // template continues on the next line
}

function scanSlash(st: TokenizerState): void {
  const next = st.line[st.i + 1];
  if (next === "/") {
    const start = st.i;
    st.i = st.line.length;
    emit(st, "text", start); // line comment: rest is opaque
    return;
  }
  if (next === "*") {
    const end = st.line.indexOf("*/", st.i + 2);
    if (end === -1) {
      st.failed = true; // block comment continues past the line
      return;
    }
    const start = st.i;
    st.i = end + 2;
    emit(st, "text", start);
    return;
  }
  if (isDivisionContext(st.prev)) {
    const start = st.i;
    st.i++;
    emit(st, "text", start);
    return;
  }
  scanRegex(st);
}

function isDivisionContext(prev: LineToken | null): boolean {
  if (!prev) return false;
  if (prev.kind === "ident") return true;
  if (DIVISION_PRECEDERS.has(prev.text)) return true;
  return /^[0-9]/.test(prev.text); // number literal
}

function scanRegex(st: TokenizerState): void {
  const start = st.i;
  st.i++;
  let inClass = false;
  while (st.i < st.line.length) {
    const ch = st.line[st.i];
    if (ch === "\\") {
      st.i += 2;
      continue;
    }
    if (ch === "[") inClass = true;
    else if (ch === "]") inClass = false;
    else if (ch === "/" && !inClass) {
      st.i++;
      while (st.i < st.line.length && IDENT_CONT.test(st.line[st.i])) st.i++; // flags
      emit(st, "text", start);
      return;
    }
    st.i++;
  }
  st.failed = true; // unterminated regex — misdetection or broken line
}

function scanWord(st: TokenizerState): void {
  const start = st.i;
  st.i++;
  while (st.i < st.line.length && IDENT_CONT.test(st.line[st.i])) st.i++;
  const word = st.line.slice(start, st.i);
  emit(st, RESERVED_TOKEN_WORDS.has(word) ? "text" : "ident", start);
}

function scanNumber(st: TokenizerState): void {
  const start = st.i;
  st.i++;
  while (st.i < st.line.length && NUMBER_CONT.test(st.line[st.i])) st.i++;
  emit(st, "text", start);
}

function scanWhitespace(st: TokenizerState): void {
  const start = st.i;
  while (st.i < st.line.length && /\s/.test(st.line[st.i])) st.i++;
  emit(st, "text", start);
}

function scanBacktick(st: TokenizerState): void {
  const start = st.i;
  st.i++;
  emit(st, "text", start);
  st.frames.push("quasi");
}

function scanPunct(st: TokenizerState): void {
  const start = st.i;
  st.i++;
  emit(st, "text", start);
}

type CodeScanner = (st: TokenizerState) => void;

function pickScanner(ch: string): CodeScanner {
  if (ch === '"' || ch === "'") return (st) => scanSimpleString(st, ch);
  if (ch === "`") return scanBacktick;
  if (ch === "/") return scanSlash;
  if (IDENT_START.test(ch)) return scanWord;
  if (/[0-9]/.test(ch)) return scanNumber;
  if (/\s/.test(ch)) return scanWhitespace;
  if (ch === "{" || ch === "}") return (st) => stepBrace(st, ch);
  return scanPunct;
}

function stepCode(st: TokenizerState): void {
  pickScanner(st.line[st.i])(st);
}

function stepBrace(st: TokenizerState, ch: "{" | "}"): void {
  const top = st.frames[st.frames.length - 1];
  if (typeof top === "number") {
    if (ch === "{") st.frames[st.frames.length - 1] = top + 1;
    else if (top === 0)
      st.frames.pop(); // closes `${`, back to quasi text
    else st.frames[st.frames.length - 1] = top - 1;
  }
  const start = st.i;
  st.i++;
  emit(st, "text", start);
}

/**
 * Tokenize a single line of generator output into identifier and opaque
 * text tokens. Returns null when the line is not self-contained (open
 * string/template/comment) — callers must treat such lines as genuine.
 *
 * The failure direction is safe by construction: any misreading produces
 * token-stream mismatches or unresolvable positions, both of which make
 * the pass skip, never rename.
 */
export function tokenizeLine(line: string): LineToken[] | null {
  const st: TokenizerState = {
    line,
    i: 0,
    tokens: [],
    frames: [],
    prev: null,
    failed: false
  };
  while (st.i < line.length && !st.failed) {
    if (st.frames[st.frames.length - 1] === "quasi") scanQuasi(st);
    else stepCode(st);
  }
  if (st.failed || st.frames.length > 0) return null;
  return st.tokens;
}

// ---------------------------------------------------------------------------
// Hunk analysis
// ---------------------------------------------------------------------------

/** One differing identifier position within a clean line pair. */
interface PairDiff {
  /** 0-based column in the NEW line. */
  col: number;
  /** New-leg token at the position. */
  fromName: string;
  /** Prior-leg token at the position. */
  toName: string;
}

type PairComparison =
  | { status: "clean"; diffs: PairDiff[] }
  | { status: "dirty" };

/**
 * A line pair is rename-noise iff the token streams align 1:1 with every
 * non-identifier token byte-identical. Differing identifier tokens are the
 * rename proposals.
 */
function compareLinePair(priorLine: string, newLine: string): PairComparison {
  const priorTokens = tokenizeLine(priorLine);
  const newTokens = tokenizeLine(newLine);
  if (!priorTokens || !newTokens) return { status: "dirty" };
  if (priorTokens.length !== newTokens.length) return { status: "dirty" };
  const diffs: PairDiff[] = [];
  for (let k = 0; k < priorTokens.length; k++) {
    const prior = priorTokens[k];
    const next = newTokens[k];
    if (prior.kind !== next.kind) return { status: "dirty" };
    if (prior.text === next.text) continue;
    if (prior.kind === "text") return { status: "dirty" };
    diffs.push({ col: next.col, fromName: next.text, toName: prior.text });
  }
  return { status: "clean", diffs };
}

/** A rename proposal at a concrete position in the new output. */
interface PositionCandidate {
  /** 1-based line in the new output. */
  line: number;
  /** 0-based column in the new output. */
  col: number;
  fromName: string;
  toName: string;
  hunkIndex: number;
}

interface NoiseLineInfo {
  hunkIndex: number;
  diffs: PairDiff[];
}

interface HunkAnalysis {
  candidates: PositionCandidate[];
  /** New-output line → its clean rename-noise pair info. */
  noiseLines: Map<number, NoiseLineInfo>;
  /** Every new-output line covered by any diff hunk (changed or added). */
  changedNewLines: Set<number>;
  stats: {
    changed: number;
    genuine: number;
    oversized: number;
    noiseHunks: number;
  };
}

function classifyChangeHunk(
  hunk: DiffHunk,
  maxHunkLines: number
):
  | { type: "genuine" }
  | { type: "oversized" }
  | { type: "noise"; pairs: PairDiff[][] } {
  if (hunk.priorLines.length !== hunk.newLines.length)
    return { type: "genuine" };
  if (hunk.newLines.length > maxHunkLines) return { type: "oversized" };
  const pairs: PairDiff[][] = [];
  for (let k = 0; k < hunk.newLines.length; k++) {
    const cmp = compareLinePair(hunk.priorLines[k], hunk.newLines[k]);
    if (cmp.status === "dirty") return { type: "genuine" };
    pairs.push(cmp.diffs);
  }
  return { type: "noise", pairs };
}

function analyzeHunks(hunks: DiffHunk[], maxHunkLines: number): HunkAnalysis {
  const analysis: HunkAnalysis = {
    candidates: [],
    noiseLines: new Map(),
    changedNewLines: new Set(),
    stats: { changed: 0, genuine: 0, oversized: 0, noiseHunks: 0 }
  };
  for (let hunkIndex = 0; hunkIndex < hunks.length; hunkIndex++) {
    const hunk = hunks[hunkIndex];
    for (let k = 0; k < hunk.newLines.length; k++) {
      analysis.changedNewLines.add(hunk.newStart + k);
    }
    if (hunk.op !== "c") continue;
    analysis.stats.changed++;
    const classified = classifyChangeHunk(hunk, maxHunkLines);
    if (classified.type === "genuine") {
      analysis.stats.genuine++;
      continue;
    }
    if (classified.type === "oversized") {
      analysis.stats.oversized++;
      continue;
    }
    analysis.stats.noiseHunks++;
    for (let k = 0; k < classified.pairs.length; k++) {
      const line = hunk.newStart + k;
      analysis.noiseLines.set(line, { hunkIndex, diffs: classified.pairs[k] });
      for (const diff of classified.pairs[k]) {
        analysis.candidates.push({
          line,
          col: diff.col,
          fromName: diff.fromName,
          toName: diff.toName,
          hunkIndex
        });
      }
    }
  }
  return analysis;
}

// ---------------------------------------------------------------------------
// Binding resolution
// ---------------------------------------------------------------------------

interface ResolvedOccurrence {
  binding: Binding;
  candidate: PositionCandidate;
}

interface Resolution {
  occurrences: ResolvedOccurrence[];
  taintedHunks: Set<number>;
}

/**
 * A position is an occurrence of a binding only when the binding's own
 * bookkeeping says so: it IS the declaration identifier, one of the
 * references, or a WRITE-TARGET identifier inside a constant violation.
 *
 * The write-target check is by node identity against
 * getBindingIdentifiers, NOT subtree containment: an assignment like
 * `accountId = cfg.accountId` is a constant violation whose subtree also
 * contains the property token `.accountId`, and a `x = { x: 1 }` self-init
 * contains the object KEY — both share the binding's name but are genuine
 * property/key positions that must taint the hunk, not vote. (Matches
 * violationWriteLines, which drives the no-new-hunks gate — the two must
 * agree on exactly which identifiers a rename rewrites.)
 */
function resolveOccurrence(
  path: NodePath<t.Identifier>,
  name: string
): Binding | null {
  const binding = path.scope.getBinding(name);
  if (!binding) return null;
  if (binding.identifier === path.node) return binding;
  if (binding.referencePaths.some((ref) => ref.node === path.node)) {
    return binding;
  }
  for (const violation of binding.constantViolations) {
    if (violationWriteTargets(violation, name).has(path.node)) return binding;
  }
  return null;
}

function resolveCandidates(
  ast: t.File,
  candidates: PositionCandidate[]
): Resolution {
  const byPos = new Map<string, PositionCandidate>();
  for (const candidate of candidates) {
    byPos.set(`${candidate.line}:${candidate.col}`, candidate);
  }
  const occurrences: ResolvedOccurrence[] = [];
  const taintedHunks = new Set<number>();
  const visited = new Set<string>();
  traverse(ast, {
    Identifier(pathArg: NodePath<t.Identifier>) {
      const loc = pathArg.node.loc;
      if (!loc) return;
      const key = `${loc.start.line}:${loc.start.column}`;
      const candidate = byPos.get(key);
      if (!candidate || pathArg.node.name !== candidate.fromName) return;
      visited.add(key);
      const binding = resolveOccurrence(pathArg, candidate.fromName);
      if (!binding) {
        taintedHunks.add(candidate.hunkIndex);
        return;
      }
      occurrences.push({ binding, candidate });
    }
  });
  // A candidate position with no matching Identifier node at all (string
  // or template-quasi content, mid-template line) also taints its hunk.
  for (const [key, candidate] of byPos) {
    if (!visited.has(key)) taintedHunks.add(candidate.hunkIndex);
  }
  return { occurrences, taintedHunks };
}

// ---------------------------------------------------------------------------
// Grouping and gates
// ---------------------------------------------------------------------------

/**
 * Drives the risk tiering: a minified fromName gets the weaker asymmetric
 * gate; a minified toName is never worth restoring (downgrade/reroll).
 * The shape itself now lives in the shared vocabulary module — see
 * `isWordlessMintShape` in minted-census.ts for its exact semantics and
 * how it deliberately differs from `isBunToken`.
 */
const isMinifiedName = isWordlessMintShape;

interface BindingGroup {
  binding: Binding;
  fromName: string;
  votesByName: Map<string, number>;
  /** Distinct hunk indexes voting for each name (consumer diversity). */
  hunksByName: Map<string, Set<number>>;
  totalVotes: number;
}

function groupByBinding(resolution: Resolution): BindingGroup[] {
  const groups = new Map<Binding, BindingGroup>();
  for (const { binding, candidate } of resolution.occurrences) {
    if (resolution.taintedHunks.has(candidate.hunkIndex)) continue;
    let group = groups.get(binding);
    if (!group) {
      group = {
        binding,
        fromName: candidate.fromName,
        votesByName: new Map(),
        hunksByName: new Map(),
        totalVotes: 0
      };
      groups.set(binding, group);
    }
    group.votesByName.set(
      candidate.toName,
      (group.votesByName.get(candidate.toName) ?? 0) + 1
    );
    let hunkSet = group.hunksByName.get(candidate.toName);
    if (!hunkSet) {
      hunkSet = new Set();
      group.hunksByName.set(candidate.toName, hunkSet);
    }
    hunkSet.add(candidate.hunkIndex);
    group.totalVotes++;
  }
  return [...groups.values()];
}

interface Survivor {
  group: BindingGroup;
  binding: Binding;
  fromName: string;
  toName: string;
  votes: number;
  kind: RenameKind;
  declLine: number;
  declCol: number;
}

type GateOutcome = { survivor: Survivor } | { skip: ReconcileSkip };

function skipOf(
  group: BindingGroup,
  toName: string,
  reason: string
): GateOutcome {
  return {
    skip: { fromName: group.fromName, toName, reason, votes: group.totalVotes }
  };
}

/**
 * Every line the rename would rewrite: the declaration identifier, each
 * reference, and each write target. Returns null when any loc is missing.
 */
function collectOccurrenceLines(binding: Binding): number[] | null {
  const declLoc = binding.identifier.loc;
  if (!declLoc) return null;
  const lines: number[] = [declLoc.start.line];
  for (const ref of binding.referencePaths) {
    const loc = ref.node.loc;
    if (!loc) return null;
    lines.push(loc.start.line);
  }
  for (const violation of binding.constantViolations) {
    const writeLines = violationWriteLines(violation, binding.identifier.name);
    if (!writeLines) return null;
    lines.push(...writeLines);
  }
  return lines;
}

/**
 * The identifier nodes a constant violation actually WRITES to that carry
 * `name` — i.e. exactly what fastRenameBinding will rewrite (LHS binding
 * positions only; RHS reads, member properties, and object keys in the
 * violation subtree are excluded). Node view of the shared
 * violationWriteTargetPaths definition in babel-utils, which also backs
 * the runnable-split rewriter — keep that one single-source.
 */
function violationWriteTargets(
  violation: NodePath,
  name: string
): Set<t.Identifier> {
  return new Set(violationWriteTargetPaths(violation, name).map((p) => p.node));
}

/** Lines of a violation's write-target identifiers named `name`, or null
 * when any of them is missing a loc. */
function violationWriteLines(
  violation: NodePath,
  name: string
): number[] | null {
  const lines: number[] = [];
  for (const id of violationWriteTargets(violation, name)) {
    if (!id.loc) return null;
    lines.push(id.loc.start.line);
  }
  return lines;
}

/** Everything the gates need beyond the group itself. */
interface GateContext {
  analysis: HunkAnalysis;
  taintedHunks: Set<number>;
  /** Resolved clean-hunk candidate position ("line:col") → its binding. */
  positionBindings: Map<string, Binding>;
  /** Bindings renamed (or, in dry-run, predicted renameable) so far. */
  appliedBindings: Set<Binding>;
  /** eval/with taint — frozen bindings must keep their original names. */
  evalTaint: EvalWithTaint;
  /** How many groups vote for each prior name (consumer injectivity). */
  toNameClaimants: Map<string, number>;
  /** Identifier names in the new output pre-reconcile (consumer tier);
   *  empty when the tier is off. */
  newNameCensus: ReadonlySet<string>;
}

/**
 * The declaration must sit in a clean, untainted rename-noise pair with the
 * binding's own name among the differing positions — required for EVERY
 * tier. A binding's declaration line always contains its name, so if that
 * line is not a clean pair the declaration's own context genuinely changed
 * (different init, added/removed tokens) and the alignment cannot be
 * trusted; snapping on a lone reference vote would pin a prior name onto a
 * differently-computed value. Returns the decl's noise-line info when
 * aligned, else null.
 */
function alignedDeclaration(
  decl: { line: number; col: number },
  ctx: GateContext
): NoiseLineInfo | null {
  const info = ctx.analysis.noiseLines.get(decl.line);
  if (!info || ctx.taintedHunks.has(info.hunkIndex)) return null;
  if (!info.diffs.some((diff) => diff.col === decl.col)) return null;
  return info;
}

/**
 * Descriptive-tier extra gate: beyond being aligned, the declaration's
 * OTHER differing tokens must all be already-reconciled bindings (fixpoint
 * over earlier rounds) — textual proof the value is computed the same way,
 * not just used the same way. The brief's `getStartTime()` vs
 * `getReconnectTime()` trap has an unreconciled second position and fails.
 */
function declarationDependenciesClean(
  info: NoiseLineInfo,
  decl: { line: number; col: number },
  ctx: GateContext
): boolean {
  return info.diffs.every((diff) => {
    if (diff.col === decl.col) return true;
    const dependency = ctx.positionBindings.get(`${decl.line}:${diff.col}`);
    return dependency !== undefined && ctx.appliedBindings.has(dependency);
  });
}

function gateGroup(
  group: BindingGroup,
  ctx: GateContext,
  opts: ReconcileOptions
): GateOutcome {
  const names = [...group.votesByName.keys()];
  if (names.length !== 1) {
    return skipOf(group, names.sort().join("|"), "disagreement");
  }
  const toName = names[0];
  if (group.binding.identifier.name !== group.fromName) {
    return skipOf(group, toName, "stale-binding");
  }
  if (isBindingEvalTaintFrozen(group.binding, ctx.evalTaint)) {
    return skipOf(group, toName, "eval-taint-frozen");
  }
  // Export-involved bindings force attemptValidatedRename onto Babel's
  // scope.rename, which splits `export const X` into a declaration plus an
  // `export { _ as X }` specifier — a structural edit that CREATES diff
  // hunks and breaks the pure-rename contract. Never reconcile them.
  if (isExportInvolved(group.binding)) {
    return skipOf(group, toName, "export-involved");
  }
  if (!opts.isEligible(group.fromName)) {
    return skipOf(group, toName, "not-eligible");
  }
  if (isMinifiedName(toName)) {
    const reason = isMinifiedName(group.fromName) ? "reroll" : "name-downgrade";
    return skipOf(group, toName, reason);
  }
  // A half-mint fossil (do7Function) passes isMinifiedName because of its
  // word tail, but it must never overwrite a DESCRIPTIVE fresh name — the
  // fresh LLM name is strictly better (exp035 task C). Over a minted
  // fresh name the restore stands: the coverage sweep re-names the fossil
  // afterwards, while a blocked restore would leave a raw mint the census
  // cannot even see.
  if (isHalfMintHead(toName) && !isMinifiedName(group.fromName)) {
    return skipOf(group, toName, "half-mint-restore");
  }
  const kind: RenameKind = isMinifiedName(group.fromName)
    ? "asymmetric"
    : "descriptive";
  if (kind === "descriptive" && !opts.descriptiveTier) {
    return skipOf(group, toName, "descriptive-tier-disabled");
  }
  return gateGroupLocations(group, toName, kind, ctx, opts);
}

/**
 * Location gates: every occurrence must sit on a diff-covered line, the
 * declaration's hunk must not be tainted, and descriptive renames need a
 * clean declaration pair.
 */
function gateGroupLocations(
  group: BindingGroup,
  toName: string,
  kind: RenameKind,
  ctx: GateContext,
  opts: ReconcileOptions
): GateOutcome {
  const declLoc = group.binding.identifier.loc;
  const occurrenceLines = declLoc
    ? collectOccurrenceLines(group.binding)
    : null;
  if (!declLoc || !occurrenceLines) {
    return skipOf(group, toName, "missing-loc");
  }
  if (
    !occurrenceLines.every((line) => ctx.analysis.changedNewLines.has(line))
  ) {
    return skipOf(group, toName, "occurrence-outside-diff");
  }
  const decl = { line: declLoc.start.line, col: declLoc.start.column };
  const declInfo = alignedDeclaration(decl, ctx);
  if (!declInfo) {
    return gateConsumerTier(group, toName, decl, ctx, opts);
  }
  if (
    kind === "descriptive" &&
    !declarationDependenciesClean(declInfo, decl, ctx)
  ) {
    return skipOf(group, toName, "decl-not-clean");
  }
  return survivorOf(group, toName, kind, decl);
}

function survivorOf(
  group: BindingGroup,
  toName: string,
  kind: RenameKind,
  decl: { line: number; col: number }
): GateOutcome {
  return {
    survivor: {
      group,
      binding: group.binding,
      fromName: group.fromName,
      toName,
      votes: group.totalVotes,
      kind,
      declLine: decl.line,
      declCol: decl.col
    }
  };
}

/**
 * Consumer tier: the declaration genuinely changed (changed-leaf), so
 * text alignment cannot prove identity — the binding's unchanged
 * CONSUMERS prove it instead. Gates, in order: testimony diversity (≥2
 * distinct clean hunks — one repeated statement is one witness),
 * injectivity (exactly one group claims the prior name), the prior name
 * must be dead in the new output (else we would steal a live name), and
 * the fresh name must be novel this hop (a name that survived from the
 * prior is deliberate, not a re-mint). The pure-rename invariant and
 * attemptValidatedRename still guard the apply.
 */
function gateConsumerTier(
  group: BindingGroup,
  toName: string,
  decl: { line: number; col: number },
  ctx: GateContext,
  opts: ReconcileOptions
): GateOutcome {
  if (!opts.consumerTier || !opts.priorNames) {
    return skipOf(group, toName, "decl-not-aligned");
  }
  const hunkCount = group.hunksByName.get(toName)?.size ?? 0;
  if (hunkCount < 2) {
    return skipOf(group, toName, "consumer-single-hunk");
  }
  if (ctx.toNameClaimants.get(toName) !== 1) {
    return skipOf(group, toName, "consumer-name-conflict");
  }
  if (ctx.newNameCensus.has(toName)) {
    return skipOf(group, toName, "consumer-to-name-live");
  }
  if (opts.priorNames.has(group.fromName)) {
    return skipOf(group, toName, "consumer-from-not-novel");
  }
  return survivorOf(group, toName, "consumer", decl);
}

// ---------------------------------------------------------------------------
// Application (round-based fixpoint)
// ---------------------------------------------------------------------------

function toRename(survivor: Survivor, applied: boolean): ReconcileRename {
  return {
    fromName: survivor.fromName,
    toName: survivor.toName,
    votes: survivor.votes,
    kind: survivor.kind,
    declLine: survivor.declLine,
    applied
  };
}

function toSkip(survivor: Survivor, reason: string): ReconcileSkip {
  return {
    fromName: survivor.fromName,
    toName: survivor.toName,
    reason,
    votes: survivor.votes
  };
}

/**
 * One rename attempt. Dry-run predicts with getRenameRejection and mutates
 * nothing (so later predictions cannot see earlier ones freeing names —
 * dry-run under-reports collision-chain renames relative to apply).
 */
function attemptOne(survivor: Survivor, apply: boolean): string | null {
  const { binding, fromName, toName } = survivor;
  if (binding.scope.bindings[fromName] !== binding) return "stale-binding";
  if (!apply) {
    const rejection = getRenameRejection(binding.scope, fromName, toName);
    return rejection ? `rename-rejected:${rejection}` : null;
  }
  const attempt = attemptValidatedRename(binding.scope, fromName, toName);
  return attempt.applied ? null : `rename-rejected:${attempt.reason}`;
}

interface RoundResult {
  applied: Survivor[];
  rejected: Array<{ survivor: Survivor; reason: string }>;
}

function attemptSurvivors(survivors: Survivor[], apply: boolean): RoundResult {
  const applied: Survivor[] = [];
  const rejected: RoundResult["rejected"] = [];
  for (const survivor of survivors) {
    const reason = attemptOne(survivor, apply);
    if (reason) rejected.push({ survivor, reason });
    else applied.push(survivor);
  }
  return { applied, rejected };
}

/**
 * Gate → attempt → repeat. Two things can unlock between rounds: a
 * declaration becomes clean once its dependency bindings are reconciled
 * (fixpoint), and a rename blocked by a target-name collision succeeds
 * once the blocking binding is renamed away. Terminates: every round
 * either applies at least one rename (and each binding applies at most
 * once) or ends the loop.
 */
function runReconcileRounds(
  groups: BindingGroup[],
  ctx: GateContext,
  opts: ReconcileOptions
): { renames: ReconcileRename[]; skipped: ReconcileSkip[] } {
  const renames: ReconcileRename[] = [];
  const skipped: ReconcileSkip[] = [];
  // The trail must reflect real mutations only — dry-run records nothing.
  const trail = (
    binding: Binding,
    fromName: string,
    attempt: Parameters<typeof strategyTrail.recordPostPass>[2]
  ) => {
    if (opts.apply) strategyTrail.recordPostPass(binding, fromName, attempt);
  };
  let remaining = groups;
  while (remaining.length > 0) {
    const survivors: Survivor[] = [];
    const deferred: Array<{ group: BindingGroup; skip: ReconcileSkip }> = [];
    for (const group of remaining) {
      const outcome = gateGroup(group, ctx, opts);
      if ("survivor" in outcome) survivors.push(outcome.survivor);
      else if (outcome.skip.reason === "decl-not-clean") {
        deferred.push({ group, skip: outcome.skip });
      } else {
        skipped.push(outcome.skip);
        trail(group.binding, group.fromName, {
          strategy: "reconcile",
          outcome: "abstained",
          reason: outcome.skip.reason,
          newName: outcome.skip.toName
        });
      }
    }
    survivors.sort((a, b) => a.declLine - b.declLine || a.declCol - b.declCol);
    const round = attemptSurvivors(survivors, opts.apply);
    for (const survivor of round.applied) {
      renames.push(toRename(survivor, opts.apply));
      ctx.appliedBindings.add(survivor.binding);
      trail(survivor.binding, survivor.fromName, {
        strategy: `reconcile-${survivor.kind}`,
        outcome: "applied",
        newName: survivor.toName
      });
    }
    if (round.applied.length === 0) {
      skipped.push(...deferred.map((d) => d.skip));
      skipped.push(...round.rejected.map((r) => toSkip(r.survivor, r.reason)));
      for (const r of round.rejected) {
        trail(r.survivor.binding, r.survivor.fromName, {
          strategy: `reconcile-${r.survivor.kind}`,
          outcome: "rejected",
          reason: r.reason,
          newName: r.survivor.toName
        });
      }
      break;
    }
    remaining = [
      ...deferred.map((d) => d.group),
      ...round.rejected.map((r) => r.survivor.group)
    ];
  }
  return { renames, skipped };
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

const DEFAULT_IS_ELIGIBLE = createIsEligible(undefined, undefined);

const DEFAULT_OPTIONS: ReconcileOptions = {
  apply: false,
  descriptiveTier: false,
  consumerTier: false,
  maxHunkLines: 10,
  isEligible: DEFAULT_IS_ELIGIBLE
};

function emptyHunkStats(): ReconcileHunkStats {
  return { changed: 0, noise: 0, genuine: 0, oversized: 0, tainted: 0 };
}

/**
 * Corpus-similarity gate: true when too few prior lines survive unchanged
 * to trust that aligned pairs are the same binding rather than coincidence.
 * Only judged for files large enough to be meaningful; disabled when the
 * prior line count is unknown.
 */
function priorTooDissimilar(
  hunks: DiffHunk[],
  priorLineCount: number | undefined
): boolean {
  if (priorLineCount === undefined || priorLineCount < MIN_CORPUS_LINES) {
    return false;
  }
  let changedPriorLines = 0;
  for (const hunk of hunks) changedPriorLines += hunk.priorLines.length;
  const unchanged = priorLineCount - changedPriorLines;
  return unchanged / priorLineCount < MIN_CORPUS_SIMILARITY;
}

/**
 * Reconcile rename noise between a freshly generated output (parsed as
 * `ast`, whose locs must be in the same coordinates as the text that was
 * diffed) and the prior version's rendered output, given their normal-
 * format diff (prior first: `diff prior new`).
 */
export function reconcileDiffNoise(
  ast: t.File,
  diffText: string,
  options: Partial<ReconcileOptions> = {}
): ReconcileResult {
  const opts: ReconcileOptions = { ...DEFAULT_OPTIONS, ...options };
  const hunks = parseNormalDiff(diffText);
  if (priorTooDissimilar(hunks, opts.priorLineCount)) {
    return {
      renames: [],
      skipped: [],
      priorTooDissimilar: true,
      hunks: emptyHunkStats()
    };
  }
  const analysis = analyzeHunks(hunks, opts.maxHunkLines);
  const resolution = resolveCandidates(ast, analysis.candidates);

  const positionBindings = new Map<string, Binding>();
  for (const { binding, candidate } of resolution.occurrences) {
    positionBindings.set(`${candidate.line}:${candidate.col}`, binding);
  }
  const groups = groupByBinding(resolution);
  const toNameClaimants = new Map<string, number>();
  for (const group of groups) {
    for (const name of group.votesByName.keys()) {
      toNameClaimants.set(name, (toNameClaimants.get(name) ?? 0) + 1);
    }
  }
  const ctx: GateContext = {
    analysis,
    taintedHunks: resolution.taintedHunks,
    positionBindings,
    appliedBindings: new Set(),
    // Only worth the traversal when there is something to gate.
    evalTaint:
      resolution.occurrences.length > 0
        ? collectEvalWithTaint(ast)
        : { taintedFunctions: new Set(), moduleTainted: false, siteCount: 0 },
    toNameClaimants,
    // Captured BEFORE any rename mutates the AST, so liveness judgments
    // are stable across rounds. Only paid when the tier can fire.
    newNameCensus:
      opts.consumerTier && opts.priorNames && groups.length > 0
        ? collectIdentifierNames(ast)
        : new Set()
  };
  const { renames, skipped } = runReconcileRounds(groups, ctx, opts);

  return {
    renames,
    skipped,
    hunks: {
      changed: analysis.stats.changed,
      noise: analysis.stats.noiseHunks - resolution.taintedHunks.size,
      genuine: analysis.stats.genuine,
      oversized: analysis.stats.oversized,
      tainted: resolution.taintedHunks.size
    }
  };
}
