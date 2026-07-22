/**
 * Ceiling for the "below-floor prior names are not names" lever: minted
 * leftovers in the PRIOR act as fixed points — same-name matching keeps
 * them forever, their votes never tally (node settled), and they collide
 * with real inheritance (fresh vRm -> prior __m rejected target-in-scope
 * while fresh __m holds the token). Count the poisoned population and
 * the noise it touches.
 *
 *   carried mints    — minted-pattern names present in BOTH trees
 *                      (the self-perpetuating fixed points)
 *   fresh-only mints — this hop's new leftovers
 *   prior-only mints — leftovers this hop happened to clear
 *   noise touched    — noise statements whose diff tokens include a
 *                      carried or drifted mint (upper bound this lever
 *                      plus mint-drift naming could clear)
 *
 *   npx tsx ceiling-mint-poison.ts <fresh.js> <prior.js>
 */
import * as fs from "node:fs";
import { statementsOf } from "./statements.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
}

const WORD = /[A-Za-z_$][\w$]*/g;

/** Minted-looking short token (naming-floor shapes: __m, _ci, q7x, A28). */
function isMintish(w: string): boolean {
  if (/[a-z]{3}/.test(w)) return false; // a real lowercase word
  if (/^[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+$/.test(w)) return false; // CONSTANT
  return w.length <= 4 || /^_{1,2}[A-Za-z0-9]{1,3}$/.test(w);
}

function census(text: string): Set<string> {
  return new Set(text.match(WORD) ?? []);
}

function main() {
  const [freshPath, priorPath] = process.argv.slice(2);
  const freshText = fs.readFileSync(freshPath, "utf8");
  const priorText = fs.readFileSync(priorPath, "utf8");
  const fresh = statementsOf(freshText) as Stmt[];
  const prior = statementsOf(priorText) as Stmt[];

  const freshMints = new Set([...census(freshText)].filter(isMintish));
  const priorMints = new Set([...census(priorText)].filter(isMintish));
  const carried = [...freshMints].filter((m) => priorMints.has(m));
  const freshOnly = [...freshMints].filter((m) => !priorMints.has(m));
  const priorOnly = [...priorMints].filter((m) => !freshMints.has(m));

  const priorByHash = new Map<string, Stmt[]>();
  for (const s of prior) {
    const list = priorByHash.get(s.hash) ?? [];
    if (list.length === 0) priorByHash.set(s.hash, list);
    list.push(s);
  }

  const carriedSet = new Set(carried);
  let touchedSt = 0;
  let touchedLn = 0;
  let mintDiffSt = 0;
  let mintDiffLn = 0;
  const counted = (tokens: string[]): Map<string, number> => {
    const m = new Map<string, number>();
    for (const w of tokens) m.set(w, (m.get(w) ?? 0) + 1);
    return m;
  };
  for (const s of fresh) {
    const twins = priorByHash.get(s.hash);
    if (!twins || twins.some((t) => t.text === s.text)) continue;
    const twin = twins.length === 1 ? twins[0] : null;
    if (!twin) continue;
    const ca = counted(s.text.match(WORD) ?? []);
    const cb = counted(twin.text.match(WORD) ?? []);
    const diff = new Set<string>();
    for (const [w, n] of ca) if ((cb.get(w) ?? 0) !== n) diff.add(w);
    for (const [w, n] of cb) if ((ca.get(w) ?? 0) !== n) diff.add(w);
    const diffTokens = [...diff];
    if (diffTokens.some((w) => carriedSet.has(w))) {
      touchedSt++;
      touchedLn += s.lines;
    }
    if (diffTokens.length > 0 && diffTokens.every((w) => isMintish(w))) {
      mintDiffSt++;
      mintDiffLn += s.lines;
    }
  }

  console.log(
    `carried mints (poisoned fixed points): ${carried.length}   fresh-only: ${freshOnly.length}   prior-only (cleared): ${priorOnly.length}`
  );
  console.log(
    `noise statements touching a carried mint: ${touchedSt} st / ${touchedLn} ln`
  );
  console.log(
    `noise statements whose diff is ONLY mint tokens: ${mintDiffSt} st / ${mintDiffLn} ln`
  );
  console.log(`sample carried: ${carried.slice(0, 15).join(", ")}`);
}

main();
