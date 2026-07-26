/**
 * What IS a name family, concretely? Two questions the threshold sweep cannot
 * answer on its own:
 *
 *   1. Who are the other members of a small family? `managedAgentsReadme` has
 *      family 2 and `managedAgentsDocs` family 4 — if those members are
 *      `managedAgentsReadme2`-style ladder siblings then a family of 2 IS a
 *      slot family, and the 197->198 caution is about something else. If they
 *      are unrelated bindings that merely collide after stripping, the family
 *      count is noise at the low end and the threshold has to clear it.
 *   2. Is the distribution bimodal? A threshold chosen in a VALLEY is a
 *      measurement; one chosen between two adjacent data points is a guess.
 *
 * Usage: npx tsx family-probe.ts <freshOutDir> [stem...]
 */
import {
  loadSide,
  readLedger,
  type Stmt
} from "../041-content-anchor/replay-lib.js";
import { stripLadderDecoration } from "./ladder.js";

function main(): void {
  const [freshDir, ...stems] = process.argv.slice(2);
  const fresh = loadSide(freshDir, readLedger(freshDir));

  const byStem = new Map<string, string[]>();
  for (const s of fresh) {
    for (const n of s.outerNames) {
      const stem = stripLadderDecoration(n);
      const list = byStem.get(stem) ?? [];
      list.push(n);
      byStem.set(stem, list);
    }
  }

  for (const stem of stems) {
    const members = byStem.get(stem) ?? [];
    console.log(`  ${stem}  family ${members.length}:`);
    console.log(`      ${members.slice(0, 12).join(", ")}`);
  }

  // Distribution over every module-level binding: is there a valley?
  const hist = new Map<number, number>();
  for (const [, members] of byStem) {
    const bucket = members.length;
    hist.set(bucket, (hist.get(bucket) ?? 0) + 1);
  }
  console.log(
    "\n  family-size distribution over ALL stems (stems at each size):"
  );
  const sizes = [...hist.keys()].sort((a, b) => a - b);
  for (const size of sizes.filter((s) => s <= 20)) {
    console.log(
      `    size ${String(size).padStart(3)}: ${String(hist.get(size)).padStart(6)} stems`
    );
  }
  const big = sizes.filter((s) => s > 20);
  console.log(
    `    size >20 : ${big.reduce((a, s) => a + (hist.get(s) ?? 0), 0)} stems (largest ${Math.max(...sizes)})`
  );

  // How many BINDINGS live in families of each size — the population a
  // threshold actually governs.
  let inSmall = 0;
  let inBig = 0;
  for (const [, members] of byStem) {
    if (members.length >= 5) inBig += members.length;
    else inSmall += members.length;
  }
  console.log(
    `\n  bindings in families of >=5: ${inBig}; in families of <5: ${inSmall}`
  );
}

main();
