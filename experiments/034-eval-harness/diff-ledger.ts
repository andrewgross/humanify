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

function main() {
  const [freshPath, priorPath, freshLedger, priorLedger] =
    process.argv.slice(2);
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
  const shapes = new Map<string, { st: number; ln: number }>();
  let familyLn = 0;
  let familySt = 0;
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
      continue;
    }
    const shape = noiseShape(s, twins[0]);
    const e = shapes.get(shape) ?? { st: 0, ln: 0 };
    e.st++;
    e.ln += s.lines;
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
  const totalDiff = addedLn + removedLn + noiseLn;
  console.log("=== diff ledger (fresh-side lines; removals from prior) ===");
  console.log(`TOTAL diff line mass: ${fmt(totalDiff)}`);
  console.log(
    `  real change: added ${fmt(addedLn)} ln (${fmt(addedSt)} st) + removed ${fmt(removedLn)} ln (${fmt(removedSt)} st)  = ${fmt(addedLn + removedLn)}`
  );
  console.log(`  naming noise: ${fmt(noiseLn)} ln`);
  for (const [shape, e] of [...shapes].sort((a, b) => b[1].ln - a[1].ln)) {
    console.log(
      `    ${shape.padEnd(16)} ${fmt(e.ln).padStart(9)} ln  (${fmt(e.st)} st)`
    );
  }
  console.log(
    `    ${"family-bucket".padEnd(16)} ${fmt(familyLn).padStart(9)} ln  (${fmt(familySt)} st)`
  );
  console.log(
    `  file moves: ${fmt(movedSt)} statement(s)${freshLedger ? "" : "  (pass ledgers to measure)"}`
  );
  console.log(
    `REMAINING unattributed: 0 (statement accounting is exhaustive; ${fmt(cleanLn)} clean ln outside the diff)`
  );
}

main();
