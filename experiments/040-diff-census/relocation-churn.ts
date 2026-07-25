/**
 * How many git lines does CROSS-FILE RELOCATION cost, and how much of it is
 * recoverable by a content anchor?
 *
 * exp040's census proved statements move wholesale between output files (263
 * byte-identical lines of the `exitPlanMode` tool object went from 85's
 * status-message.js to 86's decision-reason.js). No name-keyed measure can see
 * that class: the code sits in a minted-name lazy-init block, so it moves file
 * AND changes name, and reads as a new name rather than a move.
 *
 * So key statements by their exact TEXT instead:
 *
 *   relocated  — the identical statement text exists on both sides, in exactly
 *                one file each, and those files differ. Git renders it as a
 *                delete in one file and an add in another: `lines * 2`.
 *   anchorable — of the relocated ones, how many carry a string literal that is
 *                RARE tree-wide (appears in <= `--rare` statements per side)?
 *                That is the ceiling for a content-anchor inheritance tier: a
 *                rare literal matching exactly one prior statement pins the file,
 *                anything ambiguous abstains.
 *
 * Text equality is deliberately strict: a statement whose names drifted is NOT
 * counted, so this is a floor on relocation, not an estimate.
 *
 * Usage: npx tsx relocation-churn.ts <priorSrcDir> <freshSrcDir> [label] [--rare N]
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { parseSync } from "@babel/core";
import { tokenSet } from "../034-eval-harness/diff-ledger.js";

interface Stmt {
  text: string;
  lines: number;
  file: string;
  literals: string[];
}

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/** String literals of 12+ chars — long enough to be distinctive prose/keys
 * rather than a flag name or a single word. */
const LITERAL = /"([^"\\\n]{12,})"|'([^'\\\n]{12,})'/g;

function statementsOf(code: string, file: string): Stmt[] {
  let ast: ReturnType<typeof parseSync>;
  try {
    ast = parseSync(code, { sourceType: "unambiguous" });
  } catch {
    return [];
  }
  if (!ast || ast.type !== "File") return [];
  return ast.program.body.map((s) => {
    const text =
      s.start != null && s.end != null ? code.slice(s.start, s.end) : "";
    const literals: string[] = [];
    for (const m of text.matchAll(LITERAL)) literals.push(m[1] ?? m[2]);
    return {
      text,
      lines: text ? text.split("\n").length : 0,
      file,
      literals
    };
  });
}

/** Statements keyed by exact text, keeping only those that occur in exactly one
 * file (so a duplicated boilerplate statement never reads as a "move"). */
function byUniqueText(dir: string): Map<string, Stmt> {
  const seen = new Map<string, Stmt | null>();
  for (const rel of walk(dir)) {
    for (const s of statementsOf(
      fs.readFileSync(path.join(dir, rel), "utf8"),
      rel
    )) {
      const prev = seen.get(s.text);
      if (prev === undefined) seen.set(s.text, s);
      else if (prev && prev.file !== s.file) seen.set(s.text, null); // ambiguous
    }
  }
  const out = new Map<string, Stmt>();
  for (const [k, v] of seen) if (v) out.set(k, v);
  return out;
}

/** The diff-ledger rule: two statement texts are the same code, edited, when
 * they share at least half their tokens. */
function looksLikeSameStatement(a: string, b: string): boolean {
  const ta = tokenSet(a);
  const tb = tokenSet(b);
  let inter = 0;
  for (const w of tb) if (ta.has(w)) inter++;
  return inter / Math.max(ta.size, tb.size, 1) >= 0.5;
}

function main() {
  const args = process.argv.slice(2);
  const [priorDir, freshDir, label] = args;
  const rareMax = args.includes("--rare")
    ? Number(args[args.indexOf("--rare") + 1])
    : 1;

  const prior = byUniqueText(priorDir);
  const fresh = byUniqueText(freshDir);

  // literal -> how many statements carry it, per side
  const priorLit = new Map<string, number>();
  for (const s of prior.values())
    for (const l of new Set(s.literals))
      priorLit.set(l, (priorLit.get(l) ?? 0) + 1);
  const freshLit = new Map<string, number>();
  for (const s of fresh.values())
    for (const l of new Set(s.literals))
      freshLit.set(l, (freshLit.get(l) ?? 0) + 1);

  // Prior statements indexed by each RARE literal they carry, so an EDITED
  // statement can still be recognised as the same one.
  const priorByRare = new Map<string, Stmt[]>();
  for (const s of prior.values()) {
    for (const l of new Set(s.literals)) {
      if ((priorLit.get(l) ?? 0) > rareMax) continue;
      const list = priorByRare.get(l) ?? [];
      list.push(s);
      priorByRare.set(l, list);
    }
  }

  let movedStmts = 0;
  let movedLines = 0;
  let anchorable = 0;
  let anchorableLines = 0;
  // Moved AND edited: no exact text match, but a rare literal identifies exactly
  // one prior statement, and it lived in another file. This is the class that
  // costs the most — the exitPlanMode object moved and changed 17 of ~280 lines,
  // so an exact-text measure cannot see it at all.
  let editedMoved = 0;
  let editedMovedLines = 0;
  const pairs = new Map<string, number>();
  const listed: Array<{
    cost: number;
    from: string;
    to: string;
    a: number;
    b: number;
  }> = [];
  for (const [text, f] of fresh) {
    const p = prior.get(text);
    if (!p) {
      const cands = new Map<string, Stmt>();
      for (const l of new Set(f.literals)) {
        if ((freshLit.get(l) ?? 0) > rareMax) continue;
        for (const c of priorByRare.get(l) ?? []) cands.set(c.text, c);
      }
      if (cands.size !== 1) continue; // ambiguous or unmatched -> abstain
      const only = [...cands.values()][0];
      if (only.file === f.file) continue;
      // A shared rare literal alone is NOT enough: a 7-line statement and a
      // 5,073-line one can share one string and are obviously not the same
      // code. Require the same >=50% token overlap diff-composition uses to
      // decide "this is an edited version of that".
      if (!looksLikeSameStatement(only.text, f.text)) continue;
      editedMoved++;
      // git prints the prior copy deleted in its old file and the fresh copy
      // added in the new one: prior.lines + fresh.lines, NOT twice that.
      editedMovedLines += f.lines + only.lines;
      listed.push({
        cost: f.lines + only.lines,
        from: only.file,
        to: f.file,
        a: only.lines,
        b: f.lines
      });
      pairs.set(
        `${only.file} => ${f.file}`,
        (pairs.get(`${only.file} => ${f.file}`) ?? 0) + 1
      );
      continue;
    }
    if (p.file === f.file) continue;
    movedStmts++;
    movedLines += f.lines * 2;
    const key = `${p.file} => ${f.file}`;
    pairs.set(key, (pairs.get(key) ?? 0) + 1);
    const hasRare = f.literals.some(
      (l) =>
        (freshLit.get(l) ?? 0) <= rareMax && (priorLit.get(l) ?? 0) <= rareMax
    );
    if (hasRare) {
      anchorable++;
      anchorableLines += f.lines * 2;
    }
  }

  console.log(`=== RELOCATION CHURN${label ? ` — ${label}` : ""} ===`);
  console.log(
    `  uniquely-placed statements: prior ${prior.size}, fresh ${fresh.size}`
  );
  console.log(
    `  RELOCATED (identical text, different file): ${movedStmts} statements`
  );
  console.log(`  cost in git lines (delete + add):           ${movedLines}`);
  console.log(
    `  of those, carrying a RARE literal (<=${rareMax} per side): ${anchorable} statements / ${anchorableLines} lines` +
      (movedLines
        ? `  = ${((100 * anchorableLines) / movedLines).toFixed(1)}% recoverable ceiling`
        : "")
  );
  console.log("  top file pairs:");
  for (const [k, n] of [...pairs.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)) {
    console.log(`    ${String(n).padStart(4)}x  ${k}`);
  }
  console.log(
    `  MOVED AND EDITED (rare-literal match, one candidate, other file):\n` +
      `    ${editedMoved} statements / ${editedMovedLines} git lines` +
      ` — invisible to any exact-text or name-keyed measure`
  );
  console.log(
    `  TOTAL relocation cost: ${movedLines + editedMovedLines} git lines`
  );
  const listN = args.includes("--list")
    ? Number(args[args.indexOf("--list") + 1])
    : 0;
  if (listN > 0) {
    console.log("  largest moved+edited pairs:");
    for (const r of listed.sort((x, y) => y.cost - x.cost).slice(0, listN)) {
      console.log(`    ${String(r.cost).padStart(5)} ln  (${r.a} -> ${r.b})`);
      console.log(`        ${r.from}\n     -> ${r.to}`);
    }
  }
  console.log(
    `ROW|${label ?? ""}|${movedStmts}|${movedLines}|${anchorable}|${anchorableLines}|${editedMoved}|${editedMovedLines}`
  );
}

main();
