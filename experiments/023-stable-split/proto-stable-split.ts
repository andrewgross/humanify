/**
 * Exp023 v0 prototype: name-carried split-assignment stability, measured.
 *
 * Splits a humanified bundle's wrapper-body statements into files. With no
 * prior ledger, groups by adjacency chunks (placeholder grouping — quality
 * comes later; this prototype measures STABILITY only). With a prior
 * ledger, each statement inherits the file its declared names had in the
 * prior release (unanimous vote); statements with no usable prior follow
 * their preceding statement (locality default), so genuinely-new code
 * lands next to its neighbors.
 *
 * Emits naive per-file concatenations (no imports — NOT runnable; churn
 * measurement only) plus a _ledger.json the next release consumes.
 *
 *   npx tsx proto-stable-split.ts <humanified.js> <outDir> [--prior <ledger.json>]
 */
import fs from "node:fs";
import path from "node:path";
import { parseSync } from "@babel/core";
import * as t from "@babel/types";
import { generate } from "../../src/babel-utils.js";

/** Fresh-grouping segmentation bounds (statements per file). */
const MIN_SEG = 80;
const MAX_SEG = 400;
/** Line budget per file — giant functions otherwise blow the size cap. */
const MAX_LINES = 4000;
/** Window (statements each side) for boundary cohesion scoring. */
const WINDOW = 40;
/** Folder layer: files per folder bounds. */
const MIN_FOLDER = 8;
const MAX_FOLDER = 20;
/** Stems that make bad file names (placeholder/minted-ish). */
const BAD_STEM = /^(noop\d*|initializeModule\d+|placeholder\w*|_+\d*)$/i;

/**
 * v2 ledger: per declared name, the ORDERED list of files of its
 * declaration occurrences (statement order). Bare names are not unique
 * keys in Bun bundles (6,195 cross-file var redeclarations measured), so
 * the transfer votes per name: all occurrences in one file → that file;
 * equal occurrence counts across legs → the kth occurrence inherits the
 * kth prior file (scope-ordinal rule, exp020's unequal-count refusal);
 * anything else abstains.
 */
interface Ledger {
  nameToFiles: Record<string, string[]>;
  files: string[];
}

/** Own-properties only: bundle bindings named `constructor`/`toString`
 * collide with Object.prototype on a plain-object map. */
function ledgerMap(ledger: Ledger): Map<string, string[]> {
  return new Map(Object.entries(ledger.nameToFiles));
}

function arg(name: string): string | undefined {
  const idx = process.argv.indexOf(`--${name}`);
  return idx === -1 ? undefined : process.argv[idx + 1];
}

function wrapperBody(file: string): t.Statement[] {
  const ast = parseSync(fs.readFileSync(file, "utf-8"), {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast || ast.type !== "File") throw new Error(`parse failed: ${file}`);
  const first = ast.program.body[0];
  if (!t.isExpressionStatement(first)) throw new Error("no wrapper");
  let expr = first.expression;
  if (t.isCallExpression(expr)) expr = expr.callee as t.Expression;
  if (!t.isFunctionExpression(expr) || !t.isBlockStatement(expr.body))
    throw new Error("no wrapper fn");
  return expr.body.body;
}

function declaredNames(stmt: t.Statement): string[] {
  return Object.keys(t.getBindingIdentifiers(stmt, false));
}

/** Count declaration occurrences per name across the whole body. */
function countOccurrences(body: t.Statement[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const stmt of body) {
    for (const n of declaredNames(stmt)) {
      counts.set(n, (counts.get(n) ?? 0) + 1);
    }
  }
  return counts;
}

interface Vote {
  file?: string;
  kind: "all-same" | "ordinal" | "abstain";
}

/** One name's vote for this occurrence (the kth declaration of `name`). */
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
  // Redeclared across files: ordinals only trusted on equal counts
  // (an insertion/removal shifts every later ordinal — exp020's
  // unequal-count refusal).
  if (newCounts.get(name) === files.length && ordinal < files.length) {
    return { file: files[ordinal], kind: "ordinal" };
  }
  return { kind: "abstain" };
}

interface TransferStats {
  inherited: number;
  inheritedViaOrdinal: number;
  conflictDisagree: number;
  noVote: number;
  residueLocality: number;
}

/** Assign statements by prior inheritance; residue follows its neighbor. */
function assignWithPrior(
  body: t.Statement[],
  prior: Ledger
): { assignment: string[]; stats: TransferStats } {
  const priorNames = ledgerMap(prior);
  const newCounts = countOccurrences(body);
  const seen = new Map<string, number>();
  const assignment: string[] = new Array(body.length);
  const stats: TransferStats = {
    inherited: 0,
    inheritedViaOrdinal: 0,
    conflictDisagree: 0,
    noVote: 0,
    residueLocality: 0
  };

  for (let i = 0; i < body.length; i++) {
    const votes = new Set<string>();
    let usedOrdinal = false;
    for (const name of declaredNames(body[i])) {
      const ordinal = seen.get(name) ?? 0;
      seen.set(name, ordinal + 1);
      const vote = voteFor(name, ordinal, priorNames, newCounts);
      if (vote.file) {
        votes.add(vote.file);
        if (vote.kind === "ordinal") usedOrdinal = true;
      }
    }
    if (votes.size === 1) {
      assignment[i] = [...votes][0];
      stats.inherited++;
      if (usedOrdinal) stats.inheritedViaOrdinal++;
      continue;
    }
    if (votes.size > 1) stats.conflictDisagree++;
    else stats.noVote++;
    assignment[i] = i > 0 ? assignment[i - 1] : "new_000.js";
    stats.residueLocality++;
  }
  return { assignment, stats };
}

// ---------------------------------------------------------------------------
// Fresh grouping (release 1): reference-locality boundary detection.
// Bun emits modules sequentially and renaming is order-preserving, so file
// boundaries survive as dips in cross-reference density along the statement
// sequence. Cut at the least-cohesive position inside a size-bounded window;
// name each file after its most externally-referenced binding.
// ---------------------------------------------------------------------------

/** Per statement: indices of wrapper-body declarations it references.
 * Approximate on purpose (no shadow analysis; property names may collide
 * with top-level names) — symmetric noise a boundary score tolerates. */
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
      if (!t.isIdentifier(node)) return;
      if (own.has(node.name)) return;
      const idx = declIndex.get(node.name);
      if (idx !== undefined && idx !== i) refs.add(idx);
    });
    return refs;
  });
}

/** References crossing the cut at `c` within ±WINDOW statements. */
function boundaryScore(refs: Array<Set<number>>, c: number): number {
  const lo = Math.max(0, c - WINDOW);
  const hi = Math.min(refs.length, c + WINDOW);
  let crossing = 0;
  for (let j = c; j < hi; j++) {
    for (const r of refs[j]) if (r >= lo && r < c) crossing++;
  }
  for (let i = lo; i < c; i++) {
    for (const r of refs[i]) if (r >= c && r < hi) crossing++;
  }
  return crossing;
}

/** Lines a statement spans in the (beautified) input. */
function stmtLineCounts(body: t.Statement[]): number[] {
  return body.map((s) => (s.loc ? s.loc.end.line - s.loc.start.line + 1 : 1));
}

/** Greedy segmentation under BOTH budgets (statements and lines): cut at
 * the least-cohesive position inside the allowed range. Deterministic
 * (leftmost minimum wins). A single over-budget statement gets its own
 * segment rather than stalling. */
function segmentBoundaries(
  refs: Array<Set<number>>,
  lineCounts: number[]
): number[] {
  const cuts: number[] = [];
  let start = 0;
  while (start < refs.length) {
    // Furthest end this segment may reach under both budgets.
    let end = start + 1;
    let lines = lineCounts[start];
    while (
      end < refs.length &&
      end - start < MAX_SEG &&
      lines + lineCounts[end] <= MAX_LINES
    ) {
      lines += lineCounts[end];
      end++;
    }
    if (end >= refs.length) break; // tail fits in one segment
    let bestCut = end;
    let bestScore = Number.POSITIVE_INFINITY;
    for (let c = Math.min(start + MIN_SEG, end); c <= end; c++) {
      const score = boundaryScore(refs, c);
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

/** Inbound reference count per statement of [segStart, segEnd), counted
 * from outside the segment. */
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

/** Segment stem: the most externally-referenced binding, preferring
 * function/class declarations over var noise and skipping placeholder
 * stems. */
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
    const count = inbound.get(i) ?? 0;
    const isFnClass =
      t.isFunctionDeclaration(body[i]) || t.isClassDeclaration(body[i]);
    // Function/class stems win ties and near-ties (they read like modules).
    const better =
      !best ||
      (isFnClass === best.isFnClass
        ? count > best.count
        : isFnClass
          ? count * 2 >= best.count
          : count > best.count * 2);
    if (better) best = { idx: i, count, isFnClass };
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

/** Folder boundaries: coarser greedy segmentation over FILE boundaries,
 * scored with the same cross-reference cohesion. */
function folderBoundaries(
  refs: Array<Set<number>>,
  fileCuts: number[]
): number[] {
  const cuts: number[] = [];
  let start = 0; // index into fileCuts segments
  const segCount = fileCuts.length - 1;
  while (start < segCount) {
    const end = Math.min(start + MAX_FOLDER, segCount);
    if (end - start <= MAX_FOLDER && end >= segCount) break;
    let bestCut = Math.min(start + MIN_FOLDER, end);
    let bestScore = Number.POSITIVE_INFINITY;
    for (let f = Math.min(start + MIN_FOLDER, end); f <= end; f++) {
      const score = boundaryScore(refs, fileCuts[f]);
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

/** Fresh grouping: boundary-detected files inside boundary-detected
 * folders, both named from their most-public binding. */
function assignFresh(body: t.Statement[]): string[] {
  const refs = referenceIndices(body);
  const lineCounts = stmtLineCounts(body);
  const fileCuts = [0, ...segmentBoundaries(refs, lineCounts), body.length];
  const folderCutIdx = [
    0,
    ...folderBoundaries(refs, fileCuts),
    fileCuts.length - 1
  ];

  const assignment: string[] = new Array(body.length);
  const usedFolders = new Set<string>();
  for (let d = 0; d < folderCutIdx.length - 1; d++) {
    const firstSeg = folderCutIdx[d];
    const lastSeg = folderCutIdx[d + 1];
    const folderStem = segmentStem(
      body,
      refs,
      fileCuts[firstSeg],
      fileCuts[lastSeg]
    );
    const folder = uniqueName(folderStem, "", usedFolders);
    const usedFiles = new Set<string>();
    for (let s = firstSeg; s < lastSeg; s++) {
      const stem = segmentStem(body, refs, fileCuts[s], fileCuts[s + 1]);
      const file = `${folder}/${uniqueName(stem, ".js", usedFiles)}`;
      for (let i = fileCuts[s]; i < fileCuts[s + 1]; i++) {
        assignment[i] = file;
      }
    }
  }
  return assignment;
}

function main(): void {
  const [input, outDir] = process.argv.slice(2);
  const priorPath = arg("prior");
  const body = wrapperBody(input);
  const prior: Ledger | null = priorPath
    ? JSON.parse(fs.readFileSync(priorPath, "utf-8"))
    : null;

  let assignment: string[];
  let stats: TransferStats | undefined;
  if (prior) {
    ({ assignment, stats } = assignWithPrior(body, prior));
  } else {
    assignment = assignFresh(body);
  }

  // Emit naive per-file contents + the v2 ledger.
  fs.mkdirSync(outDir, { recursive: true });
  const byFile = new Map<string, string[]>();
  const nameFiles = new Map<string, string[]>();
  for (let i = 0; i < body.length; i++) {
    const file = assignment[i];
    if (!byFile.has(file)) byFile.set(file, []);
    byFile.get(file)?.push(generate(body[i], { compact: false }).code);
    for (const n of declaredNames(body[i])) {
      const list = nameFiles.get(n) ?? [];
      list.push(file);
      nameFiles.set(n, list);
    }
  }
  for (const [file, parts] of byFile) {
    const filePath = path.join(outDir, file);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, `${parts.join("\n")}\n`);
  }
  const ledger: Ledger = {
    nameToFiles: Object.fromEntries(nameFiles),
    files: [...byFile.keys()].sort()
  };
  fs.writeFileSync(
    path.join(outDir, "_ledger.json"),
    JSON.stringify(ledger, null, 1)
  );

  console.log(
    JSON.stringify(
      {
        statements: body.length,
        files: byFile.size,
        mode: prior ? "prior-carried" : "fresh-boundary",
        ...(stats ?? {})
      },
      null,
      2
    )
  );
}

main();
