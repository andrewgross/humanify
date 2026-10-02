/**
 * exp094: re-score the committed exp092 baseline dumps under the exp094
 * canonicalizer (the head-erasure refusal) — the must-set delta of the
 * ground-truth soundness fix on the REAL corpus, without re-running the
 * matcher (the slices in a dump are input-derived, so the old dumps answer
 * the canonicalizer question exactly; the matcher question needs new dumps
 * and is answered by the fixture regeneration + a fresh corpus run).
 *
 * Usage: npx tsx rescore-dumps.ts <runs-dir> <baseline-json>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import {
  aggregate,
  scoreMatchDump,
  type PackageScorecard
} from "../lib/match-truth/score.js";

const runsDir = path.resolve(process.argv[2]);
const baselinePath = path.resolve(process.argv[3]);

const baseline = JSON.parse(fs.readFileSync(baselinePath, "utf8")) as {
  total: ReturnType<typeof aggregate>;
  packages: PackageScorecard[];
};

const before = baseline.total;
const packages = [];
for (const pkg of baseline.packages) {
  const dumpFile = path.join(runsDir, pkg.package, "dump.json");
  if (!fs.existsSync(dumpFile)) {
    console.log(`${pkg.package}: no dump`);
    continue;
  }
  const card = scoreMatchDump(JSON.parse(fs.readFileSync(dumpFile, "utf8")));
  packages.push({
    package: pkg.package,
    oldVersion: pkg.oldVersion,
    newVersion: pkg.newVersion,
    file: pkg.file,
    scorecard: card
  });
  const b = pkg.scorecard.functions;
  const a = card.functions;
  const line =
    `${pkg.package}: must ${a.mustMatch} (was ${b.mustMatch}), ` +
    `matched ${a.matchedOfMust} (was ${b.matchedOfMust}), ` +
    `should ${a.shouldMatch} (was ${b.shouldMatch})`;
  console.log(
    a.mustMatch !== b.mustMatch ? `CHANGED ${line}` : `       ${line}`
  );
}
const after = aggregate(packages);
console.log(
  `\nTOTAL must ${after.mustMatch} (was ${before.mustMatch}), ` +
    `matched ${after.matchedOfMust} (was ${before.matchedOfMust}), ` +
    `recall ${after.recall?.toFixed(4)} (was ${before.recall?.toFixed(4)}), ` +
    `should ${after.shouldMatch} (was ${before.shouldMatch})`
);
