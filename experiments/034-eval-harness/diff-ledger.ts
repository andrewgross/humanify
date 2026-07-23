/**
 * Diff ledger: what drove the cross-version diff, line by line, in the
 * same totals-first shape as the identifier ledger. Statement-level
 * accounting (rename-invariant statementHash), so every diff line lands
 * in exactly one bucket:
 *
 *   real change      — novel fresh statements (added code) and
 *                      prior-only statements (removed code)
 *   naming noise     — hash-twin statements whose text differs, split by
 *                      shape: outer-echo (references to renamed roots),
 *                      internal (own locals), property (API-ish drift),
 *                      family buckets (ambiguous same-hash groups)
 *   file moves       — hash-twins assigned to a different file in the
 *                      split ledger (should be 0 by design)
 *
 *   npx tsx diff-ledger.ts <fresh.js> <prior.js> [freshLedger.json priorLedger.json]
 */
import * as fs from "node:fs";
import { statementsOf } from "./statements.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
}

const WORD = /[A-Za-z_$][\w$]*/g;
const fmt = (n: number) => n.toLocaleString("en-US");

function tokenize(text: string): { words: string[]; seps: string[] } {
  const words: string[] = [];
  const seps: string[] = [];
  let last = 0;
  for (const m of text.matchAll(WORD)) {
    seps.push(text.slice(last, m.index));
    words.push(m[0]);
    last = (m.index ?? 0) + m[0].length;
  }
  seps.push(text.slice(last));
  return { words, seps };
}

function isPropertyPosition(
  tok: { words: string[]; seps: string[] },
  i: number
): boolean {
  const before = tok.seps[i].trimEnd();
  if (before.endsWith(".") || before.endsWith("?.")) return true;
  const after = tok.seps[i + 1].trimStart();
  if (after.startsWith(":") && !after.startsWith("::")) return true;
  return false;
}

function declaredNames(text: string): Set<string> {
  const out = new Set<string>();
  for (const m of text.matchAll(
    /(?:\b(?:var|let|const)\s+|(?:^|\s)function\s+|(?:^|\s)class\s+)([A-Za-z_$][\w$]*)/g
  )) {
    out.add(m[1]);
  }
  for (const m of text.matchAll(/\b(?:var|let|const)\s+([^;{()]*)/g)) {
    for (const part of m[1].split(",")) {
      const id = part.trim().match(/^([A-Za-z_$][\w$]*)/);
      if (id) out.add(id[1]);
    }
  }
  for (const m of text.matchAll(/\(([^()]*)\)\s*(?:=>|\{)/g)) {
    for (const part of m[1].split(",")) {
      const id = part.trim().match(/^(?:\.\.\.)?([A-Za-z_$][\w$]*)/);
      if (id) out.add(id[1]);
    }
  }
  return out;
}

/** Set-based changed-line counts for a statement pair, both sides —
 * approximates what line `diff` would actually print for the pair. */
function editedLines(
  fresh: Stmt,
  prior: Stmt
): { fresh: number; prior: number } {
  const freshLines = fresh.text.split("\n");
  const priorLines = prior.text.split("\n");
  const priorSet = new Set(priorLines);
  const freshSet = new Set(freshLines);
  let f = 0;
  for (const line of freshLines) if (!priorSet.has(line)) f++;
  let p = 0;
  for (const line of priorLines) if (!freshSet.has(line)) p++;
  return { fresh: f, prior: p };
}

/** Classify one unique-twin noise statement by diff shape. */
function noiseShape(fresh: Stmt, prior: Stmt): string {
  const ft = tokenize(fresh.text);
  const pt = tokenize(prior.text);
  if (ft.words.length !== pt.words.length) return "misaligned";
  for (let i = 0; i <= ft.words.length; i++) {
    if ((ft.seps[i] ?? "") !== (pt.seps[i] ?? "")) return "misaligned";
  }
  const declared = declaredNames(fresh.text);
  let internal = 0;
  let outer = 0;
  let props = 0;
  for (let i = 0; i < ft.words.length; i++) {
    if (ft.words[i] === pt.words[i]) continue;
    if (isPropertyPosition(ft, i) || isPropertyPosition(pt, i)) props++;
    else if (declared.has(ft.words[i])) internal++;
    else outer++;
  }
  if (props > 0) return "property-drift";
  if (internal > 0 && outer === 0) return "internal-rename";
  if (outer > 0 && internal === 0) return "outer-echo";
  return "mixed-rename";
}

interface Ledger {
  files?: Record<string, { hashes?: string[] }>;
}

/** statement hash -> file, from a split ledger (1:1 hashes only). */
function fileByHash(ledgerPath: string): Map<string, string> {
  const ledger = JSON.parse(fs.readFileSync(ledgerPath, "utf8")) as Ledger;
  const seen = new Map<string, string | null>();
  for (const [file, entry] of Object.entries(ledger.files ?? {})) {
    for (const h of entry.hashes ?? []) {
      seen.set(h, seen.has(h) ? null : file);
    }
  }
  const out = new Map<string, string>();
  for (const [h, f] of seen) if (f !== null) out.set(h, f);
  return out;
}

export interface DiffLedger {
  /** Sum of statement lines on each side — the population denominators. */
  freshTotalLn: number;
  priorTotalLn: number;
  totalDiff: number;
  /** Modified-statement decomposition of the "real" mass: hash-flipped
   * statements paired across sides by shape (masked head + ≥50% token
   * overlap). Edited lines are a set-based estimate per pair. */
  modifiedPairs: number;
  modifiedFreshLn: number;
  modifiedPriorLn: number;
  modifiedEditedLn: number;
  modifiedEditedPriorLn: number;
  pureAddedLn: number;
  pureAddedSt: number;
  pureRemovedLn: number;
  pureRemovedSt: number;
  /** modifiedEditedLn + pureAddedLn + pureRemovedLn — the honest number. */
  honestRealLn: number;
  /** All fresh-side lines a line-diff would print. */
  diffVisibleFreshLn: number;
  /** All prior-side (removal) lines a line-diff would print. */
  diffVisiblePriorLn: number;
  /** freshTotalLn − diffVisibleFreshLn: diff-invisible, incl. unchanged
   * context inside touched statements. */
  cleanVisibleLn: number;
  addedLn: number;
  addedSt: number;
  removedLn: number;
  removedSt: number;
  noiseLn: number;
  /** Lines a line-diff would actually print for noise statements. */
  noiseEditedLn: number;
  noiseEditedPriorLn: number;
  shapes: Array<{ shape: string; st: number; ln: number; editedLn: number }>;
  familySt: number;
  familyLn: number;
  familyEditedLn: number;
  movedSt: number;
  movedMeasured: boolean;
  cleanLn: number;
}

export function computeDiffLedger(
  freshPath: string,
  priorPath: string,
  freshLedger?: string,
  priorLedger?: string
): DiffLedger {
  const fresh = statementsOf(fs.readFileSync(freshPath, "utf8")) as Stmt[];
  const prior = statementsOf(fs.readFileSync(priorPath, "utf8")) as Stmt[];

  const priorByHash = new Map<string, Stmt[]>();
  for (const s of prior) {
    const list = priorByHash.get(s.hash) ?? [];
    if (list.length === 0) priorByHash.set(s.hash, list);
    list.push(s);
  }
  const freshHashes = new Map<string, number>();
  for (const s of fresh) {
    freshHashes.set(s.hash, (freshHashes.get(s.hash) ?? 0) + 1);
  }

  let addedLn = 0;
  let addedSt = 0;
  const shapes = new Map<
    string,
    { st: number; ln: number; editedLn: number; editedPriorLn: number }
  >();
  let familyLn = 0;
  let familySt = 0;
  let familyEditedLn = 0;
  let familyEditedPriorLn = 0;
  let cleanLn = 0;
  for (const s of fresh) {
    const twins = priorByHash.get(s.hash);
    if (!twins) {
      addedSt++;
      addedLn += s.lines;
      continue;
    }
    if (twins.some((t) => t.text === s.text)) {
      cleanLn += s.lines;
      continue;
    }
    if (twins.length !== 1 || freshHashes.get(s.hash) !== 1) {
      familySt++;
      familyLn += s.lines;
      // Closest twin (fewest changed lines) approximates the honest cost.
      let best: { fresh: number; prior: number } | null = null;
      for (const t of twins) {
        const e = editedLines(s, t);
        if (!best || e.fresh < best.fresh) best = e;
      }
      familyEditedLn += best?.fresh ?? s.lines;
      familyEditedPriorLn += best?.prior ?? 0;
      continue;
    }
    const shape = noiseShape(s, twins[0]);
    const e = shapes.get(shape) ?? {
      st: 0,
      ln: 0,
      editedLn: 0,
      editedPriorLn: 0
    };
    const edited = editedLines(s, twins[0]);
    e.st++;
    e.ln += s.lines;
    e.editedLn += edited.fresh;
    e.editedPriorLn += edited.prior;
    shapes.set(shape, e);
  }
  let removedLn = 0;
  let removedSt = 0;
  for (const s of prior) {
    if (!freshHashes.has(s.hash)) {
      removedSt++;
      removedLn += s.lines;
    }
  }

  // Modified-pair decomposition: pair novel fresh statements with
  // removed prior statements by identifier-masked head line + best token
  // overlap (reciprocal-greedy). A pair means the statement was EDITED,
  // not added+removed — count its actually-changed lines (set-based).
  const novelStmts: Stmt[] = [];
  for (const s of fresh) if (!priorByHash.has(s.hash)) novelStmts.push(s);
  const removedStmts: Stmt[] = [];
  for (const s of prior) if (!freshHashes.has(s.hash)) removedStmts.push(s);
  const maskedHead = (s: Stmt) => s.text.split("\n", 1)[0].replace(WORD, "_");
  const tokenSet = (t: string) =>
    new Set((t.match(WORD) ?? []).filter((w) => w.length > 2));
  const removedByHead = new Map<string, Stmt[]>();
  for (const s of removedStmts) {
    const list = removedByHead.get(maskedHead(s)) ?? [];
    if (list.length === 0) removedByHead.set(maskedHead(s), list);
    list.push(s);
  }
  const usedRemoved = new Set<Stmt>();
  let modifiedPairs = 0;
  let modifiedFreshLn = 0;
  let modifiedPriorLn = 0;
  let modifiedEditedLn = 0;
  let modifiedEditedPriorLn = 0;
  let pureAddedLn = 0;
  let pureAddedSt = 0;
  for (const s of novelStmts) {
    const sw = tokenSet(s.text);
    let best: Stmt | null = null;
    let bestScore = 0;
    for (const c of removedByHead.get(maskedHead(s)) ?? []) {
      if (usedRemoved.has(c)) continue;
      const cw = tokenSet(c.text);
      let inter = 0;
      for (const w of cw) if (sw.has(w)) inter++;
      const score = inter / Math.max(sw.size, cw.size, 1);
      if (score > bestScore) {
        best = c;
        bestScore = score;
      }
    }
    if (best && bestScore >= 0.5) {
      usedRemoved.add(best);
      modifiedPairs++;
      modifiedFreshLn += s.lines;
      modifiedPriorLn += best.lines;
      const edited = editedLines(s, best);
      modifiedEditedLn += edited.fresh;
      modifiedEditedPriorLn += edited.prior;
    } else {
      pureAddedSt++;
      pureAddedLn += s.lines;
    }
  }
  let pureRemovedLn = 0;
  let pureRemovedSt = 0;
  for (const s of removedStmts) {
    if (!usedRemoved.has(s)) {
      pureRemovedSt++;
      pureRemovedLn += s.lines;
    }
  }

  // File moves via ledgers (1:1 hashes present in both).
  let movedSt = 0;
  if (freshLedger && priorLedger) {
    const ff = fileByHash(freshLedger);
    const pf = fileByHash(priorLedger);
    for (const [h, file] of ff) {
      const priorFile = pf.get(h);
      if (priorFile && priorFile !== file) movedSt++;
    }
  }

  const noiseLn = [...shapes.values()].reduce((a, e) => a + e.ln, 0) + familyLn;
  const noiseEditedLn =
    [...shapes.values()].reduce((a, e) => a + e.editedLn, 0) + familyEditedLn;
  const noiseEditedPriorLn =
    [...shapes.values()].reduce((a, e) => a + e.editedPriorLn, 0) +
    familyEditedPriorLn;
  const totalDiff = addedLn + removedLn + noiseLn;
  const freshTotalLn = cleanLn + noiseLn + addedLn;
  const priorTotalLn = prior.reduce((a, s) => a + s.lines, 0);
  const diffVisibleFreshLn = noiseEditedLn + modifiedEditedLn + pureAddedLn;
  const diffVisiblePriorLn =
    noiseEditedPriorLn + modifiedEditedPriorLn + pureRemovedLn;
  return {
    freshTotalLn,
    priorTotalLn,
    totalDiff,
    modifiedPairs,
    modifiedFreshLn,
    modifiedPriorLn,
    modifiedEditedLn,
    modifiedEditedPriorLn,
    pureAddedLn,
    pureAddedSt,
    pureRemovedLn,
    pureRemovedSt,
    honestRealLn: modifiedEditedLn + pureAddedLn + pureRemovedLn,
    diffVisibleFreshLn,
    diffVisiblePriorLn,
    cleanVisibleLn: freshTotalLn - diffVisibleFreshLn,
    addedLn,
    addedSt,
    removedLn,
    removedSt,
    noiseLn,
    noiseEditedLn,
    noiseEditedPriorLn,
    shapes: [...shapes]
      .sort((a, b) => b[1].editedLn - a[1].editedLn)
      .map(([shape, e]) => ({
        shape,
        st: e.st,
        ln: e.ln,
        editedLn: e.editedLn
      })),
    familySt,
    familyLn,
    familyEditedLn,
    movedSt,
    movedMeasured: Boolean(freshLedger && priorLedger),
    cleanLn
  };
}

function main() {
  const [freshPath, priorPath, freshLedger, priorLedger] =
    process.argv.slice(2);
  const ledger = computeDiffLedger(
    freshPath,
    priorPath,
    freshLedger,
    priorLedger
  );
  const {
    totalDiff,
    addedLn,
    addedSt,
    removedLn,
    removedSt,
    noiseLn,
    familyLn,
    familySt,
    movedSt,
    cleanLn
  } = ledger;
  const shapes = new Map(
    ledger.shapes.map((s) => [s.shape, { st: s.st, ln: s.ln }])
  );
  const pctOf = (n: number, d: number) =>
    d > 0 ? `${((n / d) * 100).toFixed(2)}%` : "-";
  console.log(
    "=== diff ledger (diff-visible lines; statement mass in parens) ==="
  );
  console.log(
    `TOTAL lines — fresh: ${fmt(ledger.freshTotalLn)}   prior: ${fmt(ledger.priorTotalLn)}`
  );
  console.log(
    `  clean (diff-invisible, incl. context in touched statements): ${fmt(ledger.cleanVisibleLn)} ln  (${pctOf(ledger.cleanVisibleLn, ledger.freshTotalLn)})`
  );
  console.log(
    `  diff-visible: +${fmt(ledger.diffVisibleFreshLn)} / -${fmt(ledger.diffVisiblePriorLn)} ln`
  );
  console.log(
    `    real: modified ~${fmt(ledger.modifiedEditedLn)} edited (${fmt(ledger.modifiedPairs)} pairs, mass ${fmt(ledger.modifiedFreshLn)}) · pure added ${fmt(ledger.pureAddedLn)} · pure removed ${fmt(ledger.pureRemovedLn)}  => honest real ~${fmt(ledger.honestRealLn)}`
  );
  console.log(
    `    naming noise: ~${fmt(ledger.noiseEditedLn)} edited (mass ${fmt(noiseLn)})`
  );

  for (const [shape, e] of [...shapes].sort((a, b) => b[1].ln - a[1].ln)) {
    console.log(
      `    ${shape.padEnd(16)} ${fmt(e.ln).padStart(9)} ln  (${fmt(e.st)} st)`
    );
  }
  console.log(
    `      ${"family-bucket".padEnd(16)} ~${fmt(ledger.familyEditedLn).padStart(7)} edited (mass ${fmt(familyLn)}, ${fmt(familySt)} st)`
  );
  console.log(
    `  file moves: ${fmt(movedSt)} statement(s)${freshLedger ? "" : "  (pass ledgers to measure)"}`
  );
  console.log(
    `REMAINING unattributed: 0 (statement accounting is exhaustive; ${fmt(cleanLn)} clean ln outside the diff)`
  );
}

// Run the CLI only when executed directly, not when imported.
if (
  process.argv[1] &&
  import.meta.url.endsWith(process.argv[1].split("/").pop() ?? "")
) {
  main();
}
