/**
 * Verify the PRODUCTION wiring: stableSplitFromCode(code, {clustered:true})
 * end-to-end on the real bundle. This exercises the concat-equivalence
 * assertion (throws if the emit is lossy), the ledger, and case-safety.
 *
 *   NODE_OPTIONS=--max-old-space-size=8192 tsx verify-prod.ts 2.1.89
 */

import { stableSplitFromCode } from "../../src/split/stable-split.js";
import { loadBeautified } from "./lib/io.js";
import { folderStats, lineCountsOf, sizeStats } from "./lib/metrics.js";

async function main(): Promise<void> {
  const version = process.argv[2] ?? "2.1.89";
  const code = await loadBeautified(version);
  const res = await stableSplitFromCode(code);
  if (!res)
    throw new Error("stableSplitFromCode returned null (not a wrapper)");
  console.log("concat-equivalence: PASSED (stableSplitFromCode did not throw)");

  const files = [...res.fileContents.keys()];
  const libFiles = files.filter((f) => f.startsWith("vendor/"));
  const appFiles = files.filter((f) => f.startsWith("src/"));
  const strays = files.filter(
    (f) => !f.startsWith("src/") && !f.startsWith("vendor/")
  );
  const appFC = new Map(
    [...res.fileContents].filter(([f]) => f.startsWith("src/"))
  );
  const s = sizeStats(lineCountsOf(appFC));
  const fstats = folderStats(appFiles);

  console.log(`\nstatements:   ${res.stats.statements}`);
  console.log(
    `total files:  ${files.length}  (${appFiles.length} app under src/ + ` +
      `${libFiles.length} libraries under vendor/ + ${strays.length} stray)`
  );
  if (strays.length > 0) {
    console.log(
      `  STRAY (outside src|vendor): ${strays.slice(0, 5).join(", ")}`
    );
  }
  console.log(
    `app folders:  ${fstats.folderCount}   maxDepth: ${fstats.maxDepth}`
  );
  console.log(
    `app file lines: median ${s.median.toFixed(0)}  mean ${s.mean.toFixed(0)}  max ${s.max}`
  );

  // Case-safety: no two paths collide under case-folding.
  const lower = new Map<string, string>();
  let collisions = 0;
  for (const f of files) {
    const lc = f.toLowerCase();
    if (lower.has(lc) && lower.get(lc) !== f) {
      collisions++;
      if (collisions <= 5)
        console.log(`  CASE COLLISION: ${lower.get(lc)}  vs  ${f}`);
    }
    lower.set(lc, f);
  }
  console.log(
    `\ncase-insensitive collisions: ${collisions} ${collisions === 0 ? "(safe)" : "(BUG)"}`
  );

  // Sample named paths (mechanical stems from dominant bindings).
  console.log(`\nsample app paths:`);
  for (const f of appFiles.slice(0, 6)) console.log(`  ${f}`);
  console.log(`sample library paths:`);
  for (const f of libFiles.slice(0, 4)) console.log(`  ${f}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
