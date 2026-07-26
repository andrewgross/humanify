/**
 * TWO WITNESSES, ONE AXIS — the measurement that decides exp043's rule.
 *
 * When the content anchor and the name vote disagree, exp042 broke the tie by
 * inspecting the NAME (does it carry a minted counter?). That works, and it
 * cleared 3,246 git lines, but it is an indirect test: it asks whether the name
 * LOOKS meaningless rather than whether it is WRONG.
 *
 * Reading the residue (experiments/042-anchor-preempt/eyeball-preempt.ts --all)
 * shows the indirect test mis-sorts both ways:
 *
 *   initializeApp256      a counter that landed on a KNOWN unit token (256), so
 *                         exp042 read it as meaningful. It is the exitPlanMode
 *                         exemplar, 564 git lines, and it is a pure slot.
 *   managedAgentsReadme   a real, unique, undecorated name -- which exp042's
 *                         brief said made it a credible witness. In fact the
 *                         fresh statement is the "Managed Agents - Go" README
 *                         and the name's prior owner is the JAVA one. The name
 *                         rotated between SIBLING DOCUMENTS, exactly as a
 *                         counter rotates between lazy-init blocks.
 *
 * So test the witnesses directly instead. Both name a candidate prior
 * statement; ask which candidate the fresh statement actually RESEMBLES:
 *
 *   anchorEdit  — fraction of the fresh statement's lines that differ from the
 *                 anchor's twin. Small = "this is that code, edited".
 *   nameLits    — rare literals shared with the prior statement that owned the
 *                 NAME. Zero = the name's candidate is unrelated code.
 *
 * No threshold on a power-law distribution, no vocabulary of minted stems, no
 * knowledge of this codebase: it is the same evidence for every program.
 *
 * Usage: npx tsx two-witness.ts <priorOutDir> <freshOutDir> <label>
 */
import { editedLineCounts } from "../034-eval-harness/diff-ledger.js";
import {
  AnchorIndex,
  loadSide,
  readLedger,
  recoveredLines,
  type Stmt
} from "../041-content-anchor/replay-lib.js";

/** Prior statements that declare a given outer name. */
function indexByOuterName(stmts: Stmt[]): Map<string, Stmt[]> {
  const m = new Map<string, Stmt[]>();
  for (const s of stmts) {
    for (const n of s.outerNames) {
      const list = m.get(n) ?? [];
      list.push(s);
      m.set(n, list);
    }
  }
  return m;
}

function sharedLiterals(a: Stmt, b: Stmt): number {
  return new Set(a.literals.filter((l) => b.literals.includes(l))).size;
}

interface Row {
  ln: number;
  name: string;
  anchorEdit: number;
  anchorLits: number;
  nameLits: number;
  nameOwner: string;
}

function main(): void {
  const [priorDir, freshDir, label] = process.argv.slice(2);
  const prior = loadSide(priorDir, readLedger(priorDir));
  const fresh = loadSide(freshDir, readLedger(freshDir));
  const anchors = new AnchorIndex(prior, fresh);
  const priorByOuter = indexByOuterName(prior);

  const rows: Row[] = [];
  for (const f of fresh) {
    const twin = anchors.verdict(f);
    if (!twin || twin.file === f.file) continue;
    if (f.outerNames.length === 0) continue;
    const e = editedLineCounts(f.text, twin.text);
    // What the NAME's witness offers: the best prior statement declaring any of
    // this statement's names.
    let nameLits = 0;
    let nameOwner = "NOBODY";
    for (const n of f.outerNames) {
      for (const o of priorByOuter.get(n) ?? []) {
        const s = sharedLiterals(f, o);
        if (s >= nameLits) {
          nameLits = s;
          nameOwner = `#${o.idx}(${o.lines}ln)`;
        }
      }
    }
    rows.push({
      ln: recoveredLines(f, twin),
      name: f.outerNames[0],
      anchorEdit: e.fresh / Math.max(f.lines, 1),
      anchorLits: sharedLiterals(f, twin),
      nameLits,
      nameOwner
    });
  }
  rows.sort((a, b) => b.ln - a.ln);

  console.log(`=== TWO-WITNESS COMPARISON — ${label ?? ""} ===`);
  console.log(
    "     ln  anchorEdit  anchorLits  nameLits  name                       name's prior owner"
  );
  for (const r of rows) {
    console.log(
      `  ${String(r.ln).padStart(5)}  ${(100 * r.anchorEdit).toFixed(1).padStart(9)}%  ` +
        `${String(r.anchorLits).padStart(10)}  ${String(r.nameLits).padStart(8)}  ` +
        `${r.name.padEnd(26)} ${r.nameOwner}`
    );
  }

  console.log("  edit-fraction sweep (anchor preempts when anchorEdit <= E):");
  for (const E of [0.05, 0.1, 0.15, 0.2, 0.25, 0.33, 0.5]) {
    const fires = rows.filter((r) => r.anchorEdit <= E);
    console.log(
      `    E=${String(Math.round(100 * E)).padStart(3)}%  fires on ${String(fires.length).padStart(2)}` +
        ` / ${String(fires.reduce((a, r) => a + r.ln, 0)).padStart(5)} ln` +
        `   leaves ${rows.length - fires.length} / ${rows
          .filter((r) => r.anchorEdit > E)
          .reduce((a, r) => a + r.ln, 0)} ln`
    );
  }
  console.log(
    `ROW|${label ?? ""}|${rows.length}|${rows.reduce((a, r) => a + r.ln, 0)}`
  );
}

main();
