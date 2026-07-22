/**
 * Shape of the residual noise: for every unique 1:1 hash-twin noise
 * statement, is it PURE RENAME SHAPE (token streams align positionally,
 * all separators equal) — and where do the differing tokens sit?
 *
 *   aligned-locals    — every diff position is a non-property word:
 *                       recoverable by positional slot transfer inside
 *                       the twin statement (twin-local transfer).
 *   aligned-props     — aligned but some diff positions are property
 *                       names (`.x` / `x:`): input drift or API change,
 *                       NOT rename-recoverable.
 *   misaligned        — streams differ in shape: not pure rename noise.
 *
 *   npx tsx diagnose-residual-shape.ts <fresh.js> <prior.js>
 */
import * as fs from "node:fs";
import { statementsOf } from "./statements.js";

interface Stmt {
  hash: string;
  text: string;
  lines: number;
}

const WORD = /[A-Za-z_$][\w$]*/g;

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

interface Tally {
  st: number;
  ln: number;
}
const tally = (): Tally => ({ st: 0, ln: 0 });

function main() {
  const [freshPath, priorPath] = process.argv.slice(2);
  const fresh = statementsOf(fs.readFileSync(freshPath, "utf8")) as Stmt[];
  const prior = statementsOf(fs.readFileSync(priorPath, "utf8")) as Stmt[];

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

  const alignedLocals = tally();
  const alignedProps = tally();
  const misaligned = tally();
  const family = tally();
  let alignedDiffTokens = 0;
  let alignedStatementsBig = 0; // >=10 differing positions
  const sampleLocals: string[] = [];
  const sampleMisaligned: string[] = [];

  for (const s of fresh) {
    const twins = priorByHash.get(s.hash);
    if (!twins || twins.some((t) => t.text === s.text)) continue;
    if (twins.length !== 1 || freshHashCount.get(s.hash) !== 1) {
      family.st++;
      family.ln += s.lines;
      continue;
    }
    const twin = twins[0];
    const ft = tokenize(s.text);
    const pt = tokenize(twin.text);
    let aligned = ft.words.length === pt.words.length;
    if (aligned) {
      for (let i = 0; i <= ft.words.length; i++) {
        if ((ft.seps[i] ?? "") !== (pt.seps[i] ?? "")) {
          aligned = false;
          break;
        }
      }
    }
    if (!aligned) {
      misaligned.st++;
      misaligned.ln += s.lines;
      if (sampleMisaligned.length < 4) {
        sampleMisaligned.push(
          `${s.lines} ln: ${s.text.split("\n", 1)[0].slice(0, 90)}`
        );
      }
      continue;
    }
    let diffs = 0;
    let propDiffs = 0;
    const pairs: string[] = [];
    for (let i = 0; i < ft.words.length; i++) {
      if (ft.words[i] === pt.words[i]) continue;
      diffs++;
      if (isPropertyPosition(ft, i) || isPropertyPosition(pt, i)) propDiffs++;
      else if (pairs.length < 3) pairs.push(`${ft.words[i]}<-${pt.words[i]}`);
    }
    if (propDiffs > 0) {
      alignedProps.st++;
      alignedProps.ln += s.lines;
    } else {
      alignedLocals.st++;
      alignedLocals.ln += s.lines;
      alignedDiffTokens += diffs;
      if (diffs >= 10) alignedStatementsBig++;
      if (sampleLocals.length < 6) {
        sampleLocals.push(
          `${s.lines} ln, ${diffs} slots: ${pairs.join(" ")} | ${s.text
            .split("\n", 1)[0]
            .slice(0, 70)}`
        );
      }
    }
  }

  console.log(
    `aligned-locals: ${alignedLocals.st} st / ${alignedLocals.ln} ln  <- twin-local transfer ceiling`
  );
  console.log(
    `  (${alignedDiffTokens} differing slots total, ${alignedStatementsBig} statements with >=10)`
  );
  console.log(`aligned-props:  ${alignedProps.st} st / ${alignedProps.ln} ln`);
  console.log(`misaligned:     ${misaligned.st} st / ${misaligned.ln} ln`);
  console.log(`family buckets: ${family.st} st / ${family.ln} ln`);
  console.log("\nsample aligned-locals statements:");
  for (const e of sampleLocals) console.log(`  ${e}`);
  console.log("\nsample misaligned statements:");
  for (const e of sampleMisaligned) console.log(`  ${e}`);
}

main();
