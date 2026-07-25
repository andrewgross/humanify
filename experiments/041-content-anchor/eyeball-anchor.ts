/**
 * Print a fresh statement beside the prior statement the content anchor paired
 * it with, so a proposed move can be READ before it is believed. The brief's
 * standing rule: four hypotheses were refuted in the session that wrote it, one
 * of which fit the arithmetic perfectly and was still wrong.
 *
 * Usage: npx tsx eyeball-anchor.ts <priorOutDir> <freshOutDir> <freshIdx...> [--lines N]
 */
import { AnchorIndex, loadSide, readLedger } from "./replay-lib.js";

function main(): void {
  const args = process.argv.slice(2);
  const cap = args.includes("--lines")
    ? Number(args[args.indexOf("--lines") + 1])
    : 14;
  const [priorDir, freshDir, ...rest] = args;
  const indices = rest.filter((a) => /^\d+$/.test(a)).map(Number);
  const prior = loadSide(priorDir, readLedger(priorDir));
  const fresh = loadSide(freshDir, readLedger(freshDir));
  const anchors = new AnchorIndex(prior, fresh);

  for (const i of indices) {
    const f = fresh[i];
    console.log(`\n########## fresh#${i} [${f.file}] ${f.lines} ln`);
    console.log(f.text.split("\n").slice(0, cap).join("\n"));
    const twin = anchors.verdict(f);
    if (!twin) {
      console.log("  (the anchor abstains on this statement)");
      continue;
    }
    console.log(
      `\n---------- prior#${twin.idx} [${twin.file}] ${twin.lines} ln`
    );
    console.log(twin.text.split("\n").slice(0, cap).join("\n"));
    const priorLines = new Set(twin.text.split("\n"));
    const freshLines = new Set(f.text.split("\n"));
    const shared = f.literals.filter((l) => twin.literals.includes(l));
    console.log(
      `\n  fresh-only lines ${f.text.split("\n").filter((l) => !priorLines.has(l)).length}` +
        `, prior-only ${twin.text.split("\n").filter((l) => !freshLines.has(l)).length}` +
        `, shared rare literals ${new Set(shared).size}`
    );
    console.log(
      `  sample: ${[...new Set(shared)]
        .slice(0, 3)
        .map((x) => JSON.stringify(x.slice(0, 60)))
        .join(", ")}`
    );
  }
}

main();
