/**
 * Echo-web fixpoint ceiling: how much residual noise clears under
 * ITERATED rename inheritance, and does it converge?
 *
 * Round model (text-level, LLM-free, order-independent):
 *   1. Pair unique 1:1 hash-twin noise statements (statementHash is
 *      rename-invariant, so pairing survives rounds and — unlike the
 *      line-diff reconcile — is immune to bundle reorders).
 *   2. Positionally compare token streams (twins are masked-identical;
 *      equal-length streams with all non-word chunks equal are
 *      rename-noise; differing word positions propose from→to pairs).
 *      Property positions (`.name` / `name:`) never vote.
 *   3. A pair is PROVABLE when: witnessed in ≥2 distinct statements,
 *      bijective this round (from↔to unanimous both directions), `to`
 *      dead in the fresh text, `from` dead in the prior text.
 *   4. Apply all provable renames everywhere in the fresh text (echoes
 *      in family buckets and novel statements clear passively), then
 *      repeat — chains unlock as mixed statements lose resolved tokens.
 *
 * Reports per round and the final noise delta. Hashes are never
 * recomputed: identifier renames cannot change a masked hash.
 *
 *   npx tsx ceiling-echo-web.ts <fresh.js> <prior.js> [maxRounds]
 */
import * as fs from "node:fs";
import { statementsOf } from "./statements.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
}

const WORD = /[A-Za-z_$][\w$]*/g;

/** Split into alternating chunks: [sep, word, sep, word, ...]. */
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

/** Is the word at position i a property (`.name`) or object key (`name:`)? */
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

interface PairVotes {
  votes: number;
  statements: Set<number>;
}

function main() {
  const [freshPath, priorPath, maxRoundsArg] = process.argv.slice(2);
  const maxRounds = Number(maxRoundsArg ?? 12);
  const priorText = fs.readFileSync(priorPath, "utf8");
  const fresh = statementsOf(fs.readFileSync(freshPath, "utf8")) as Stmt[];
  const prior = statementsOf(priorText) as Stmt[];

  const priorByHash = new Map<string, Stmt[]>();
  for (const s of prior) {
    const list = priorByHash.get(s.hash) ?? [];
    if (list.length === 0) priorByHash.set(s.hash, list);
    list.push(s);
  }
  const freshHashCount = new Map<string, number>();
  for (const s of fresh) {
    freshHashCount.set(s.hash, (freshHashCount.get(s.hash) ?? 0) + 1);
  }
  const priorCensus = new Set(priorText.match(WORD) ?? []);

  const noiseNow = (): { st: number; ln: number } => {
    let st = 0;
    let ln = 0;
    for (const s of fresh) {
      const twins = priorByHash.get(s.hash);
      if (!twins || twins.some((t) => t.text === s.text)) continue;
      st++;
      ln += s.lines;
    }
    return { st, ln };
  };

  const start = noiseNow();
  console.log(`start: ${start.st} noise st / ${start.ln} noise ln`);

  let totalRenames = 0;
  for (let round = 1; round <= maxRounds; round++) {
    // Fresh census recomputed per round (earlier renames freed names).
    const freshCensus = new Set<string>();
    for (const s of fresh) for (const w of s.text.match(WORD) ?? []) {
      freshCensus.add(w);
    }

    // Collect candidate pairs from unique-twin rename-noise statements.
    const forward = new Map<string, Map<string, PairVotes>>();
    const claimants = new Map<string, Set<string>>();
    fresh.forEach((s, idx) => {
      const twins = priorByHash.get(s.hash);
      if (!twins || twins.length !== 1) return;
      if (freshHashCount.get(s.hash) !== 1) return;
      const twin = twins[0];
      if (s.text === twin.text) return;
      const ft = tokenize(s.text);
      const pt = tokenize(twin.text);
      if (ft.words.length !== pt.words.length) return;
      for (let i = 0; i <= ft.words.length; i++) {
        if ((ft.seps[i] ?? "") !== (pt.seps[i] ?? "")) return;
      }
      for (let i = 0; i < ft.words.length; i++) {
        const from = ft.words[i];
        const to = pt.words[i];
        if (from === to) continue;
        if (isPropertyPosition(ft, i) || isPropertyPosition(pt, i)) continue;
        let byTo = forward.get(from);
        if (!byTo) {
          byTo = new Map();
          forward.set(from, byTo);
        }
        let pv = byTo.get(to);
        if (!pv) {
          pv = { votes: 0, statements: new Set() };
          byTo.set(to, pv);
        }
        pv.votes++;
        pv.statements.add(idx);
        let owners = claimants.get(to);
        if (!owners) {
          owners = new Set();
          claimants.set(to, owners);
        }
        owners.add(from);
      }
    });

    // Gate: bijective, >=2 witness statements, to dead in fresh, from
    // dead in prior.
    const mapping = new Map<string, string>();
    for (const [from, byTo] of forward) {
      if (byTo.size !== 1) continue;
      const [to, pv] = [...byTo][0];
      if (pv.statements.size < 2) continue;
      if ((claimants.get(to)?.size ?? 0) !== 1) continue;
      if (freshCensus.has(to)) continue;
      if (priorCensus.has(from)) continue;
      mapping.set(from, to);
    }
    if (mapping.size === 0) {
      console.log(`round ${round}: fixpoint (no provable renames)`);
      break;
    }
    for (const s of fresh) {
      s.text = s.text.replace(WORD, (w) => mapping.get(w) ?? w);
    }
    totalRenames += mapping.size;
    const now = noiseNow();
    console.log(
      `round ${round}: ${mapping.size} renames -> ${now.st} st / ${now.ln} ln`
    );
  }

  const end = noiseNow();
  console.log(
    `\nfixpoint: ${totalRenames} total renames; noise ${start.st}->${end.st} st ` +
      `(${(((start.st - end.st) / start.st) * 100).toFixed(1)}%), ` +
      `${start.ln}->${end.ln} ln (${(((start.ln - end.ln) / start.ln) * 100).toFixed(1)}%)`
  );
}

main();
