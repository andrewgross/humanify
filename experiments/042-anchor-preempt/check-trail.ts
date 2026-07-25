/**
 * Gate check 5 — did `anchorPreempt` fire WHERE the ceiling said, not merely
 * somewhere? A KPI that moves while a tier fires on statements nobody predicted
 * is a bug wearing a win's clothing (exp041's standing rule).
 *
 * Prints, per hop: the tier histogram, and every anchorPreempt statement with
 * the vote it overrode — so the list can be compared against Task A's
 * hand-read indices (85->86: 12 statements, 215->216: 2).
 *
 * Usage: npx tsx check-trail.ts <diag.json> [expectedIndex...]
 */
import * as fs from "node:fs";

interface Entry {
  index: number;
  names: string[];
  placedBy: string;
  file: string;
  evidence: { votes?: string[]; anchor?: string };
}

function main(): void {
  const [diagPath, ...expected] = process.argv.slice(2);
  const diag = JSON.parse(fs.readFileSync(diagPath, "utf8")) as {
    placementTrails?: { tiers: Record<string, number>; trails: Entry[] };
  };
  const trail = diag.placementTrails;
  if (!trail) {
    console.log(`${diagPath}: NO placementTrails (was --diagnostics on?)`);
    return;
  }
  console.log(`=== ${diagPath}`);
  console.log(`  tiers: ${JSON.stringify(trail.tiers)}`);
  const fired = trail.trails.filter((e) => e.placedBy === "anchorPreempt");
  console.log(`  anchorPreempt fired on ${fired.length} statements:`);
  for (const e of fired.sort((a, b) => a.index - b.index)) {
    console.log(
      `    #${e.index}  ${e.names.join(",")}\n` +
        `        placed: ${e.file}\n` +
        `        overrode name vote: ${JSON.stringify(e.evidence.votes ?? [])}`
    );
  }
  if (expected.length > 0) {
    const got = new Set(fired.map((e) => e.index));
    const want = new Set(expected.map(Number));
    const missing = [...want].filter((i) => !got.has(i));
    const extra = [...got].filter((i) => !want.has(i));
    console.log(
      `  vs Task A's hand-read list: ${want.size} expected, ` +
        `${missing.length} MISSING ${JSON.stringify(missing)}, ` +
        `${extra.length} UNPREDICTED ${JSON.stringify(extra)}`
    );
  }
}

main();
