/**
 * Does a name carry identity, measured FROM THE BUNDLE rather than from a
 * vocabulary list?
 *
 * exp042 gated its preempt on `hasMintedNumber` and cleared every statement it
 * recognised. The residue is what that predicate does not see, and reading it
 * (experiments/042-anchor-preempt, --all) shows the predicate is too narrow in
 * three distinct ways, all on the SAME mechanism:
 *
 *   initializeApp256        "256" is a KNOWN unit token (sha256, base64), so a
 *                           counter that lands on one is invisible. This is the
 *                           exitPlanMode exemplar the series is named after.
 *   initializeEnvironment9  a ONE-digit counter; the rule needs 2+ digits.
 *   setupApplicationVar     the ladder's OTHER decoration -- DECORATION_WORDS
 *   initializeModulesData   (Val/Var/Ref/Item/Data/Result/Value) in
 *                           src/llm/validation.ts, minted by the same
 *                           `findWithSuffixes` that mints the counters.
 *
 * The obvious fix -- "strip a trailing counter or decoration word" -- is WRONG,
 * and one case proves it: `managedAgentsDocsVal` (678 git lines on 197->198)
 * wears a `Val`, but `managedAgentsDocs` is a real, unique name. Preferring the
 * anchor there is the coin flip exp042's brief refused to take.
 *
 * So measure genericity instead of asserting it. Strip the ladder decoration,
 * then ask HOW MANY OTHER module-level bindings in the same release share that
 * stem. `initializeApp` labels hundreds of unrelated lazy-init blocks -- which
 * one holds which counter is decided by processing order, so the name is a SLOT
 * IN A FAMILY, not an identity. `managedAgentsDocs` labels exactly one thing.
 *
 * That is program-agnostic: it needs no vocabulary of this codebase's minted
 * stems, and it self-calibrates to whatever the renamer happens to mint.
 *
 * Usage: npx tsx family-size.ts <priorOutDir> <freshOutDir> <label>
 */
import {
  AnchorIndex,
  loadSide,
  readLedger,
  recoveredLines,
  type Stmt
} from "../041-content-anchor/replay-lib.js";
import { stripLadderDecoration } from "./ladder.js";

/** How many module-level bindings in this release share a decoration-stripped
 * stem. 1 = the name labels one thing. Large = the name is a slot in a family
 * the renamer reuses, and which member holds which slot is processing order. */
function familySizes(stmts: Stmt[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const s of stmts) {
    for (const n of s.outerNames) {
      const stem = stripLadderDecoration(n);
      counts.set(stem, (counts.get(stem) ?? 0) + 1);
    }
  }
  return counts;
}

function main(): void {
  const [priorDir, freshDir, label] = process.argv.slice(2);
  const prior = loadSide(priorDir, readLedger(priorDir));
  const fresh = loadSide(freshDir, readLedger(freshDir));
  const anchors = new AnchorIndex(prior, fresh);
  const freshFamily = familySizes(fresh);
  const priorFamily = familySizes(prior);

  console.log(
    `=== NAME-FAMILY SIZE ON ANCHOR DISAGREEMENTS — ${label ?? ""} ===`
  );
  console.log(
    "  ln     family  name -> stem                          (family = bindings sharing the stem)"
  );
  const rows: Array<{ f: Stmt; twin: Stmt; ln: number; fam: number }> = [];
  for (const f of fresh) {
    const twin = anchors.verdict(f);
    if (!twin || twin.file === f.file) continue;
    if (f.outerNames.length === 0) continue;
    // The whole statement is a slot only if EVERY name it declares is one, so
    // the family size that matters is the SMALLEST across its names.
    const fam = Math.min(
      ...f.outerNames.map((n) => {
        const stem = stripLadderDecoration(n);
        return Math.max(freshFamily.get(stem) ?? 0, priorFamily.get(stem) ?? 0);
      })
    );
    rows.push({ f, twin, ln: recoveredLines(f, twin), fam });
  }
  rows.sort((a, b) => b.ln - a.ln);
  for (const r of rows) {
    const n = r.f.outerNames[0];
    console.log(
      `  ${String(r.ln).padStart(5)}  ${String(r.fam).padStart(6)}  ${n} -> ${stripLadderDecoration(n)}`
    );
  }

  // Sweep the threshold: at each cut, how many lines would a family-size gate
  // claim, and how many would it leave to the name vote?
  console.log("  threshold sweep (family >= T is treated as a SLOT):");
  for (const T of [2, 3, 5, 10, 25, 50, 100]) {
    const fires = rows.filter((r) => r.fam >= T);
    console.log(
      `    T=${String(T).padStart(3)}  fires on ${String(fires.length).padStart(2)} statements / ${String(
        fires.reduce((a, r) => a + r.ln, 0)
      ).padStart(5)} ln   leaves ${rows.length - fires.length} / ${rows
        .filter((r) => r.fam < T)
        .reduce((a, r) => a + r.ln, 0)} ln`
    );
  }
  console.log(
    `ROW|${label ?? ""}|${rows.length}|${rows.reduce((a, r) => a + r.ln, 0)}`
  );
}

main();
