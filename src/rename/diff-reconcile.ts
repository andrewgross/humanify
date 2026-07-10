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
 *   - Renames are tiered by what they overwrite: minified → descriptive
 *     (asymmetric) overwrites nothing meaningful; descriptive →
 *     descriptive additionally requires the binding's declaration line to
 *     be a clean rename-noise pair whose ONLY differing token is the
 *     binding's own name (evidence the value is computed the same way);
 *     renaming TO a minified name is never useful (reroll / downgrade).
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
import * as t from "@babel/types";
import { traverse } from "../babel-utils.js";
import { createIsEligible, type IsEligibleFn } from "./rename-eligibility.js";
import {
  attemptValidatedRename,
  getRenameRejection
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
}

export type RenameKind = "asymmetric" | "descriptive";

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
 */
export function computeNormalDiff(priorText: string, newText: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "humanify-reconcile-"));
  try {
    const priorPath = path.join(dir, "prior.js");
    const newPath = path.join(dir, "new.js");
    fs.writeFileSync(priorPath, priorText);
    fs.writeFileSync(newPath, newText);
    const proc = spawnSync("diff", [priorPath, newPath], {
      encoding: "utf-8",
      maxBuffer: DIFF_MAX_BUFFER
    });
    // diff exits 0 (identical) or 1 (differences); anything else is an error
    if (proc.status !== 0 && proc.status !== 1) {
      throw new Error(
        `diff failed (status ${proc.status}): ${proc.stderr ?? proc.error}`
      );
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
  /** New-output lines belonging to each hunk index. */
  hunkNewLines: Map<number, number[]>;
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
    hunkNewLines: new Map(),
    stats: { changed: 0, genuine: 0, oversized: 0, noiseHunks: 0 }
  };
  for (let hunkIndex = 0; hunkIndex < hunks.length; hunkIndex++) {
    const hunk = hunks[hunkIndex];
    const newLineNumbers = hunk.newLines.map((_, k) => hunk.newStart + k);
    for (const line of newLineNumbers) analysis.changedNewLines.add(line);
    analysis.hunkNewLines.set(hunkIndex, newLineNumbers);
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
 * references, or a write target inside a constant violation. Property
 * names, object keys, labels, and free identifiers all fail — the hunk
 * that proposed them is not pure rename noise.
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
  const violationNodes = new Set(binding.constantViolations.map((v) => v.node));
  if (path.findParent((p) => violationNodes.has(p.node))) return binding;
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

/** Port of attribute-noise.py's heuristic: minified survivor vs LLM name. */
function isMinifiedName(name: string): boolean {
  if (name.includes("$")) return true;
  if (name.length <= 3) return true;
  if (name.length <= 4 && !/[a-z]{3}/.test(name)) return true;
  return !/[a-z]{3}/.test(name);
}

interface BindingGroup {
  binding: Binding;
  fromName: string;
  votesByName: Map<string, number>;
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
        totalVotes: 0
      };
      groups.set(binding, group);
    }
    group.votesByName.set(
      candidate.toName,
      (group.votesByName.get(candidate.toName) ?? 0) + 1
    );
    group.totalVotes++;
  }
  return [...groups.values()];
}

interface Survivor {
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

/** Lines of a violation's write-target identifiers named `name`, or null
 * when any of them is missing a loc. */
function violationWriteLines(
  violation: NodePath,
  name: string
): number[] | null {
  const lines: number[] = [];
  const ids = t.getBindingIdentifiers(violation.node, true);
  for (const entry of Object.values(ids)) {
    for (const id of Array.isArray(entry) ? entry : [entry]) {
      if (id.name !== name) continue;
      if (!id.loc) return null;
      lines.push(id.loc.start.line);
    }
  }
  return lines;
}

/**
 * Descriptive-tier extra gate: the declaration line must itself be a clean
 * rename-noise pair whose ONLY differing token is this binding's own name.
 * That is textual proof the value is computed the same way, not just used
 * the same way — the brief's `getStartTime()` vs `getReconnectTime()` trap
 * has a second differing position and fails here.
 */
function hasCleanDeclaration(
  survivorDecl: { line: number; col: number },
  analysis: HunkAnalysis,
  taintedHunks: Set<number>
): boolean {
  const info = analysis.noiseLines.get(survivorDecl.line);
  if (!info || taintedHunks.has(info.hunkIndex)) return false;
  return info.diffs.length === 1 && info.diffs[0].col === survivorDecl.col;
}

function gateGroup(
  group: BindingGroup,
  analysis: HunkAnalysis,
  taintedHunks: Set<number>,
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
  if (!opts.isEligible(group.fromName)) {
    return skipOf(group, toName, "not-eligible");
  }
  if (isMinifiedName(toName)) {
    const reason = isMinifiedName(group.fromName) ? "reroll" : "name-downgrade";
    return skipOf(group, toName, reason);
  }
  const kind: RenameKind = isMinifiedName(group.fromName)
    ? "asymmetric"
    : "descriptive";
  if (kind === "descriptive" && !opts.descriptiveTier) {
    return skipOf(group, toName, "descriptive-tier-disabled");
  }
  return gateGroupLocations(group, toName, kind, analysis, taintedHunks);
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
  analysis: HunkAnalysis,
  taintedHunks: Set<number>
): GateOutcome {
  const declLoc = group.binding.identifier.loc;
  const occurrenceLines = declLoc
    ? collectOccurrenceLines(group.binding)
    : null;
  if (!declLoc || !occurrenceLines) {
    return skipOf(group, toName, "missing-loc");
  }
  if (!occurrenceLines.every((line) => analysis.changedNewLines.has(line))) {
    return skipOf(group, toName, "occurrence-outside-diff");
  }
  const decl = { line: declLoc.start.line, col: declLoc.start.column };
  const declNoise = analysis.noiseLines.get(decl.line);
  if (declNoise && taintedHunks.has(declNoise.hunkIndex)) {
    return skipOf(group, toName, "decl-tainted");
  }
  if (
    kind === "descriptive" &&
    !hasCleanDeclaration(decl, analysis, taintedHunks)
  ) {
    return skipOf(group, toName, "decl-not-clean");
  }
  return {
    survivor: {
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

// ---------------------------------------------------------------------------
// Application
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

/** Dry-run: predict with getRenameRejection, mutate nothing. */
function dryRunSurvivors(survivors: Survivor[]): {
  renames: ReconcileRename[];
  skips: ReconcileSkip[];
} {
  const renames: ReconcileRename[] = [];
  const skips: ReconcileSkip[] = [];
  for (const survivor of survivors) {
    const reason = getRenameRejection(
      survivor.binding.scope,
      survivor.fromName,
      survivor.toName
    );
    if (reason) skips.push(toSkip(survivor, `rename-rejected:${reason}`));
    else renames.push(toRename(survivor, false));
  }
  return { renames, skips };
}

/**
 * Apply survivors through attemptValidatedRename, looping while progress
 * is made: a rename rejected because its target name is still taken can
 * succeed once the blocking binding is itself renamed away.
 */
function applySurvivors(survivors: Survivor[]): {
  renames: ReconcileRename[];
  skips: ReconcileSkip[];
} {
  const renames: ReconcileRename[] = [];
  let pending = [...survivors];
  let progress = true;
  const lastReason = new Map<Survivor, string>();
  while (progress && pending.length > 0) {
    progress = false;
    const stillPending: Survivor[] = [];
    for (const survivor of pending) {
      if (
        survivor.binding.scope.bindings[survivor.fromName] !== survivor.binding
      ) {
        lastReason.set(survivor, "stale-binding");
        stillPending.push(survivor);
        continue;
      }
      const attempt = attemptValidatedRename(
        survivor.binding.scope,
        survivor.fromName,
        survivor.toName
      );
      if (attempt.applied) {
        renames.push(toRename(survivor, true));
        progress = true;
      } else {
        lastReason.set(survivor, `rename-rejected:${attempt.reason}`);
        stillPending.push(survivor);
      }
    }
    pending = stillPending;
  }
  const skips = pending.map((survivor) =>
    toSkip(survivor, lastReason.get(survivor) ?? "rename-rejected:unknown")
  );
  return { renames, skips };
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

const DEFAULT_IS_ELIGIBLE = createIsEligible(undefined, undefined);

const DEFAULT_OPTIONS: ReconcileOptions = {
  apply: false,
  descriptiveTier: false,
  maxHunkLines: 10,
  isEligible: DEFAULT_IS_ELIGIBLE
};

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
  const analysis = analyzeHunks(hunks, opts.maxHunkLines);
  const resolution = resolveCandidates(ast, analysis.candidates);

  const skipped: ReconcileSkip[] = [];
  const survivors: Survivor[] = [];
  for (const group of groupByBinding(resolution)) {
    const outcome = gateGroup(group, analysis, resolution.taintedHunks, opts);
    if ("survivor" in outcome) survivors.push(outcome.survivor);
    else skipped.push(outcome.skip);
  }
  survivors.sort((a, b) => a.declLine - b.declLine || a.declCol - b.declCol);

  const { renames, skips } = opts.apply
    ? applySurvivors(survivors)
    : dryRunSurvivors(survivors);
  skipped.push(...skips);

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
