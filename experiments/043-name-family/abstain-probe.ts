/**
 * WHY does the content anchor abstain?
 *
 * After exp043 the residual relocation is 1,390 git lines, and 1,046 of it is
 * 34 edited-and-moved blocks. Only SIX are anchor disagreements (measured
 * irreducible — 16%-71% rewritten, where the name is the better witness). The
 * other 28 are statements the anchor has NO verdict for at all, even though
 * `relocation-churn.ts` finds a rare-literal match for them.
 *
 * The difference is the shipped gates (src/split/content-anchor.ts), each of
 * which abstains rather than guesses:
 *
 *   noRareLiteral   the statement carries no 12+ char string literal
 *   notRareInPrior  its literals are all shared by 2+ PRIOR statements
 *   notRareInFresh  ... or by 2+ FRESH statements
 *   split           its rare literals point at DIFFERENT prior statements
 *   overlap         a candidate was found but shares <50% of its tokens
 *   contested       two fresh statements resolve to the same prior statement
 *
 * Knowing which gate refuses tells you whether there is a lever here at all: a
 * statement with no rare literal has no evidence to recover, whereas one lost
 * to `contested` or `overlap` might be recoverable with a better tiebreak.
 *
 * Usage: npx tsx abstain-probe.ts <priorOutDir> <freshOutDir> <label>
 */
import {
  AnchorIndex,
  loadSide,
  readLedger,
  type Stmt
} from "../041-content-anchor/replay-lib.js";

const RARE = /"([^"\\\n]{12,})"|'([^'\\\n]{12,})'/g;
const WORD = /[A-Za-z_$][\w$]*/g;

function literalsOf(text: string): Set<string> {
  const out = new Set<string>();
  for (const m of text.matchAll(RARE)) out.add(m[1] ?? m[2]);
  return out;
}
function tokensOf(text: string): Set<string> {
  return new Set((text.match(WORD) ?? []).filter((w) => w.length > 2));
}
function overlap(a: Set<string>, b: Set<string>): number {
  let n = 0;
  for (const w of b) if (a.has(w)) n++;
  return n / Math.max(a.size, b.size, 1);
}
/** literal -> sole owning index, dropping any literal owned by two statements. */
function soleOwners(sets: Array<Set<string>>): Map<string, number> {
  const owner = new Map<string, number>();
  for (let i = 0; i < sets.length; i++) {
    for (const lit of sets[i]) {
      const prev = owner.get(lit);
      if (prev === undefined) owner.set(lit, i);
      else if (prev !== i) owner.set(lit, -1);
    }
  }
  for (const [lit, idx] of owner) if (idx === -1) owner.delete(lit);
  return owner;
}

function main(): void {
  const [priorDir, freshDir, label] = process.argv.slice(2);
  const prior = loadSide(priorDir, readLedger(priorDir));
  const fresh = loadSide(freshDir, readLedger(freshDir));
  const anchors = new AnchorIndex(prior, fresh);

  const priorLits = prior.map((s) => literalsOf(s.text));
  const freshLits = fresh.map((s) => literalsOf(s.text));
  const priorOwn = soleOwners(priorLits);
  const freshOwn = soleOwners(freshLits);

  // Statements that MOVED and are not byte-identical: the population that costs
  // the residual lines. Paired by unique outer name, for accounting only.
  const priorByName = new Map<string, Stmt[]>();
  for (const s of prior) {
    for (const n of s.outerNames) {
      const l = priorByName.get(n) ?? [];
      l.push(s);
      priorByName.set(n, l);
    }
  }

  const reasons = new Map<string, { n: number; ln: number }>();
  const bump = (why: string, ln: number) => {
    const r = reasons.get(why) ?? { n: 0, ln: 0 };
    r.n++;
    r.ln += ln;
    reasons.set(why, r);
  };
  const examples: Array<{ why: string; ln: number; name: string }> = [];

  for (const f of fresh) {
    if (anchors.verdict(f)) continue; // the anchor spoke; not our population
    // Did it move? Use the unique-name pairing as the accounting oracle.
    let twin: Stmt | undefined;
    const cands = new Map<number, Stmt>();
    for (const n of f.outerNames) {
      for (const c of priorByName.get(n) ?? []) cands.set(c.idx, c);
    }
    if (cands.size === 1) twin = [...cands.values()][0];
    if (!twin || twin.file === f.file) continue;
    if (twin.text === f.text) continue; // identical-text class, priced separately
    const ln = twin.lines + f.lines;

    // Reproduce the gates in order and record the FIRST that refuses.
    const lits = freshLits[f.idx];
    if (lits.size === 0) {
      bump("noRareLiteral", ln);
      examples.push({ why: "noRareLiteral", ln, name: f.outerNames[0] ?? "?" });
      continue;
    }
    const rareHere = [...lits].filter((l) => freshOwn.get(l) === f.idx);
    if (rareHere.length === 0) {
      bump("notRareInFresh", ln);
      examples.push({
        why: "notRareInFresh",
        ln,
        name: f.outerNames[0] ?? "?"
      });
      continue;
    }
    const targets = new Set<number>();
    for (const l of rareHere) {
      const p = priorOwn.get(l);
      if (p !== undefined) targets.add(p);
    }
    if (targets.size === 0) {
      bump("notRareInPrior", ln);
      examples.push({
        why: "notRareInPrior",
        ln,
        name: f.outerNames[0] ?? "?"
      });
      continue;
    }
    if (targets.size > 1) {
      bump("split", ln);
      examples.push({ why: "split", ln, name: f.outerNames[0] ?? "?" });
      continue;
    }
    const p = [...targets][0];
    if (overlap(tokensOf(prior[p].text), tokensOf(f.text)) < 0.5) {
      bump("overlap", ln);
      examples.push({ why: "overlap", ln, name: f.outerNames[0] ?? "?" });
      continue;
    }
    bump("contested", ln);
    examples.push({ why: "contested", ln, name: f.outerNames[0] ?? "?" });
  }

  console.log(`=== WHY THE ANCHOR ABSTAINS — ${label ?? ""} ===`);
  const rows = [...reasons.entries()].sort((a, b) => b[1].ln - a[1].ln);
  for (const [why, r] of rows) {
    console.log(
      `  ${why.padEnd(16)} ${String(r.n).padStart(4)} statements  ${String(r.ln).padStart(6)} git lines`
    );
  }
  console.log("  largest, by reason:");
  for (const e of examples.sort((a, b) => b.ln - a.ln).slice(0, 6)) {
    console.log(
      `    ${String(e.ln).padStart(5)} ln  ${e.why.padEnd(15)} ${e.name}`
    );
  }
}

main();
