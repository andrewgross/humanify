/**
 * Ceiling for the family-rotation / head-flip repair lever: how much of
 * the residual noise is statements whose ENTIRE difference from their
 * prior twin is the names DECLARED in the statement itself (head flips) —
 * i.e. recoverable by a binding-level inherit with no text ambiguity —
 * and under what pairing.
 *
 * Classes measured per pair:
 *   unique-twin head-flip-only  — 1:1 hash twin; substituting the
 *     positionally-paired declared names makes the texts byte-equal.
 *     Split by SAFE (every prior name dead on the fresh side, every
 *     fresh name novel) vs collision-risky.
 *   family pairable             — non-unique hash bucket; members paired
 *     by reciprocal-unique descriptive-token overlap, then the same
 *     head-flip test.
 *   residual                    — noise this lever cannot express.
 *
 *   npx tsx ceiling-family-rotation.ts <fresh.js> <prior.js>
 */
import * as fs from "node:fs";
import { statementsOf } from "./statements.js";

const words = (t: string): string[] => t.match(/[A-Za-z_$][\w$]*/g) ?? [];
const isMintish = (w: string) =>
  w.length <= 4 || /^[A-Za-z]?[\w]?\d+_?$/.test(w) || /[_$]\d*$/.test(w);

/** Declared names in first-occurrence order (multi-declarator aware). */
function declaredNames(text: string): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  const add = (name: string) => {
    if (!seen.has(name)) {
      seen.add(name);
      out.push(name);
    }
  };
  const headRe =
    /(?:(?:^|\s)function\s+|(?:^|\s)class\s+)([A-Za-z_$][\w$]*)/g;
  for (const m of text.matchAll(headRe)) add(m[1]);
  const listRe = /\b(?:var|let|const)\s+([^;{()]*)/g;
  for (const m of text.matchAll(listRe)) {
    for (const part of m[1].split(",")) {
      const id = part.trim().match(/^([A-Za-z_$][\w$]*)/);
      if (id) add(id[1]);
    }
  }
  return out;
}

/** Word-boundary rename of `from`→`to` across a text. */
function renameAll(text: string, mapping: Map<string, string>): string {
  if (mapping.size === 0) return text;
  return text.replace(/[A-Za-z_$][\w$]*/g, (w) => mapping.get(w) ?? w);
}

interface Stmt {
  hash: string;
  text: string;
  lines: number;
}

/** Positional declared-name mapping; null when shapes disagree. */
function headMapping(fresh: string, prior: string): Map<string, string> | null {
  const freshDecls = declaredNames(fresh);
  const priorDecls = declaredNames(prior);
  if (freshDecls.length !== priorDecls.length) return null;
  const mapping = new Map<string, string>();
  for (let i = 0; i < freshDecls.length; i++) {
    if (freshDecls[i] !== priorDecls[i]) {
      mapping.set(freshDecls[i], priorDecls[i]);
    }
  }
  return mapping;
}

function rootKind(text: string): string {
  const head = text.trimStart();
  if (head.startsWith("function") || head.startsWith("async function")) {
    return "fn-decl";
  }
  if (head.startsWith("class")) return "class";
  if (/^(var|let|const)\b/.test(head)) return "var-decl";
  return "other";
}

interface Tally {
  st: number;
  ln: number;
}
const tally = (): Tally => ({ st: 0, ln: 0 });
const bump = (t: Tally, s: Stmt) => {
  t.st++;
  t.ln += s.lines;
};

function main() {
  const [freshPath, priorPath] = process.argv.slice(2);
  const freshText = fs.readFileSync(freshPath, "utf8");
  const priorText = fs.readFileSync(priorPath, "utf8");
  const fresh = statementsOf(freshText) as Stmt[];
  const prior = statementsOf(priorText) as Stmt[];
  const freshCensus = new Set(words(freshText));
  const priorCensus = new Set(words(priorText));

  const priorByHash = new Map<string, Stmt[]>();
  for (const s of prior) {
    const list = priorByHash.get(s.hash) ?? [];
    if (list.length === 0) priorByHash.set(s.hash, list);
    list.push(s);
  }
  const freshByHash = new Map<string, Stmt[]>();
  for (const s of fresh) {
    const list = freshByHash.get(s.hash) ?? [];
    if (list.length === 0) freshByHash.set(s.hash, list);
    list.push(s);
  }

  const total = tally();
  const uniqueFlipSafe = tally();
  const uniqueFlipRisky = tally();
  const uniqueOther = tally();
  const familyPairableFlip = tally();
  const familyUnpairable = tally();
  const byKind = new Map<string, Tally>();
  const flipExamples: string[] = [];

  /** Is every mapped rename safe (prior name dead here, fresh name novel)? */
  const mappingSafe = (mapping: Map<string, string>): boolean => {
    for (const [from, to] of mapping) {
      if (freshCensus.has(to) || priorCensus.has(from)) return false;
    }
    return true;
  };

  const classifyFlip = (
    s: Stmt,
    twin: Stmt,
    intoSafe: Tally,
    intoRisky: Tally,
    intoOther: Tally
  ): void => {
    const mapping = headMapping(s.text, twin.text);
    if (mapping && renameAll(s.text, mapping) === twin.text) {
      const safe = mappingSafe(mapping);
      bump(safe ? intoSafe : intoRisky, s);
      const kind = rootKind(s.text);
      const k = byKind.get(kind) ?? tally();
      bump(k, s);
      byKind.set(kind, k);
      if (safe && flipExamples.length < 8) {
        const pair = [...mapping.entries()][0];
        flipExamples.push(
          `${pair?.[0]} -> ${pair?.[1]}  (${s.lines} ln, ${rootKind(s.text)})`
        );
      }
      return;
    }
    bump(intoOther, s);
  };

  // Greedy reciprocal-unique pairing inside a family bucket by shared
  // descriptive tokens.
  const pairBucket = (fs_: Stmt[], ps: Stmt[]): Array<[Stmt, Stmt]> => {
    const score = (a: Stmt, b: Stmt): number => {
      const wa = new Set(words(a.text).filter((w) => !isMintish(w)));
      let n = 0;
      for (const w of new Set(words(b.text).filter((x) => !isMintish(x)))) {
        if (wa.has(w)) n++;
      }
      return n;
    };
    const pairs: Array<[Stmt, Stmt]> = [];
    const usedP = new Set<Stmt>();
    for (const f of fs_) {
      let best: Stmt | null = null;
      let bestScore = 0;
      let tied = false;
      for (const p of ps) {
        if (usedP.has(p)) continue;
        const sc = score(f, p);
        if (sc > bestScore) {
          best = p;
          bestScore = sc;
          tied = false;
        } else if (sc === bestScore && sc > 0) tied = true;
      }
      if (best && !tied && bestScore >= 2) {
        usedP.add(best);
        pairs.push([f, best]);
      }
    }
    return pairs;
  };

  const familySeen = new Set<Stmt>();
  for (const s of fresh) {
    const twins = priorByHash.get(s.hash);
    if (!twins || twins.some((t) => t.text === s.text)) continue;
    bump(total, s);
    const freshBucket = freshByHash.get(s.hash) ?? [];
    if (twins.length === 1 && freshBucket.length === 1) {
      classifyFlip(s, twins[0], uniqueFlipSafe, uniqueFlipRisky, uniqueOther);
    }
  }
  // Family buckets handled per-bucket (pairing is bucket-global).
  for (const [hash, freshBucket] of freshByHash) {
    const priorBucket = priorByHash.get(hash);
    if (!priorBucket) continue;
    if (freshBucket.length === 1 && priorBucket.length === 1) continue;
    const noisy = freshBucket.filter(
      (s) => !priorBucket.some((t) => t.text === s.text)
    );
    if (noisy.length === 0) continue;
    const pairs = pairBucket(noisy, priorBucket);
    const paired = new Set(pairs.map(([f]) => f));
    for (const [f, p] of pairs) {
      familySeen.add(f);
      classifyFlip(f, p, familyPairableFlip, familyUnpairable, familyUnpairable);
    }
    for (const f of noisy) {
      if (!paired.has(f)) {
        familySeen.add(f);
        bump(familyUnpairable, f);
      }
    }
  }

  console.log(`noise total:              ${total.st} st / ${total.ln} ln`);
  console.log(
    `unique-twin flip SAFE:    ${uniqueFlipSafe.st} st / ${uniqueFlipSafe.ln} ln  <- recoverable ceiling`
  );
  console.log(
    `unique-twin flip risky:   ${uniqueFlipRisky.st} st / ${uniqueFlipRisky.ln} ln`
  );
  console.log(
    `unique-twin other:        ${uniqueOther.st} st / ${uniqueOther.ln} ln`
  );
  console.log(
    `family pairable flip:     ${familyPairableFlip.st} st / ${familyPairableFlip.ln} ln  <- recoverable ceiling`
  );
  console.log(
    `family unpairable/other:  ${familyUnpairable.st} st / ${familyUnpairable.ln} ln`
  );
  console.log("\nflip mass by statement kind:");
  for (const [kind, t] of [...byKind].sort((a, b) => b[1].ln - a[1].ln)) {
    console.log(`  ${kind.padEnd(9)} ${t.st} st / ${t.ln} ln`);
  }
  console.log("\nsample safe flips:");
  for (const e of flipExamples) console.log(`  ${e}`);
}

main();
