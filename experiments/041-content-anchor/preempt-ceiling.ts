/**
 * NEXT-LEVER CEILING — should the content anchor be allowed to PREEMPT a name
 * vote, the way the binding-identity tier already preempts one?
 *
 * exp041 put the anchor BELOW the name vote, so it only speaks when no name
 * voted. Measuring what is left afterwards shows why that ordering now costs:
 * 97% of the residual relocation on 85→86 is 22 statements averaging 209 lines,
 * and the largest is exp040's own exemplar — the `exitPlanMode` block, which
 * moved from `status-message.js` to `decision-reason.js`.
 *
 * The name vote did not malfunction there. It followed the name correctly:
 *
 *     2.1.85   initApp22        = the exitPlanMode block   (status-message.js)
 *              initializeApp256 = something else            (decision-reason.js)
 *     2.1.86   initializeApp256 = the exitPlanMode block    (decision-reason.js)
 *
 * Both names exist in both releases with ONE stable home each. What moved is the
 * CONTENT: the minted counters were recycled onto different blocks. A wordless
 * mint is not an identity, it is a slot number — but the name-vote tier treats
 * it as evidence and outranks the anchor, which holds 27 shared rare literals
 * proving whose content this actually is.
 *
 * So measure, per hop: how often does the anchor hold a verdict that DISAGREES
 * with where the statement landed, how many git lines would preempting recover,
 * and how much of that is confined to statements whose declared names are ALL
 * wordless mints — the safe, narrow version of the rule.
 *
 * Usage: npx tsx preempt-ceiling.ts <priorOutDir> <freshOutDir> <label>
 */
import {
  AnchorIndex,
  loadSide,
  pct,
  readLedger,
  recoveredLines
} from "./replay-lib.js";

/** A name carrying a minted counter (`initializeApp256`) — mirrors the
 * `hasMintedNumber` test in src/split/stable-split.ts. Two-plus digits that are
 * not a known meaningful number (utf8, base64, sha256 ...). */
const KNOWN_NUMBER_TOKENS = new Set([
  "8",
  "16",
  "32",
  "64",
  "128",
  "256",
  "512",
  "1024",
  "2048",
  "4096"
]);
function hasMintedNumber(name: string): boolean {
  const runs = name.match(/\d+/g);
  if (!runs) return false;
  return runs.some((run) => run.length >= 2 && !KNOWN_NUMBER_TOKENS.has(run));
}

function main(): void {
  const [priorDir, freshDir, label] = process.argv.slice(2);
  const prior = loadSide(priorDir, readLedger(priorDir));
  const fresh = loadSide(freshDir, readLedger(freshDir));
  const anchors = new AnchorIndex(prior, fresh);

  let disagree = 0;
  let disagreeLines = 0;
  let mintedOnly = 0;
  let mintedOnlyLines = 0;
  let agree = 0;
  const examples: Array<{
    ln: number;
    names: string;
    from: string;
    to: string;
  }> = [];

  for (const f of fresh) {
    const twin = anchors.verdict(f);
    if (!twin) continue;
    if (twin.file === f.file) {
      agree++;
      continue;
    }
    const ln = recoveredLines(f, twin);
    disagree++;
    disagreeLines += ln;
    // The narrow rule: only override a vote cast by names carrying a MINTED
    // COUNTER. `isWordlessMintShape` is the wrong predicate here -- these names
    // do contain words (`initializeApp256`); what makes them meaningless is the
    // recycled numeric suffix, which the renamer reassigns to a different block
    // every release. Same test `isRejectedStem` uses to refuse a stem.
    const names = f.outerNames;
    const allMinted = names.length > 0 && names.every(hasMintedNumber);
    if (allMinted) {
      mintedOnly++;
      mintedOnlyLines += ln;
    }
    examples.push({
      ln,
      names: `${names.slice(0, 2).join(",")}${allMinted ? " [minted]" : ""}`,
      from: f.file,
      to: twin.file
    });
  }

  console.log(`=== ANCHOR-PREEMPT CEILING — ${label ?? ""} ===`);
  console.log(
    `  anchor verdicts that AGREE with today's placement: ${agree} (no action)`
  );
  console.log(
    `  anchor verdicts that DISAGREE: ${disagree} statements / ${disagreeLines} git lines`
  );
  console.log(
    `    of which every declared name carries a MINTED COUNTER: ${mintedOnly} / ${mintedOnlyLines} ln` +
      ` (${pct(mintedOnlyLines, disagreeLines)} of the disagreements)`
  );
  console.log("  largest disagreements (EYEBALL BEFORE BELIEVING):");
  for (const e of examples.sort((a, b) => b.ln - a.ln).slice(0, 6)) {
    console.log(`    ${String(e.ln).padStart(5)} ln  ${e.names}`);
    console.log(`        today:  ${e.from}\n        anchor: ${e.to}`);
  }
  console.log(
    `ROW|${label ?? ""}|${disagree}|${disagreeLines}|${mintedOnly}|${mintedOnlyLines}`
  );
}

main();
