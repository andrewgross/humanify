/**
 * Inspect the top collision groups in a minified bundle.
 *
 * For each of the top N collision hashes, prints 2-3 representative function
 * source snippets and checks whether memberKey can disambiguate them.
 *
 * Usage:
 *   npx tsx experiments/012-minifier-sensitivity/inspect-collisions.ts <file>
 */

import { readFileSync } from "node:fs";
import { buildFingerprintData } from "../../test/e2e/harness/validate.js";
import { generate } from "../../src/babel-utils.js";

const TOP_N = 5;
const SAMPLES_PER_GROUP = 3;
const MAX_SOURCE_LINES = 5;

function truncateSource(code: string): string {
  const lines = code.split("\n");
  if (lines.length <= MAX_SOURCE_LINES) return code;
  return (
    lines.slice(0, MAX_SOURCE_LINES).join("\n") +
    "\n  // ... (" +
    lines.length +
    " lines total)"
  );
}

async function main(): Promise<void> {
  const filePath = process.argv[2];
  if (!filePath) {
    console.error("Usage: npx tsx inspect-collisions.ts <minified-file>");
    process.exit(1);
  }

  console.log(`Reading ${filePath}...`);
  const code = readFileSync(filePath, "utf-8");

  console.log(
    "Parsing and building fingerprints (this takes a while for 12MB)..."
  );
  const start = performance.now();
  const { functions, index } = buildFingerprintData(code, filePath);
  const elapsed = ((performance.now() - start) / 1000).toFixed(1);
  console.log(
    `Done in ${elapsed}s. ${functions.size} functions, ${index.byExactHash.size} unique hashes.\n`
  );

  // Sort collision groups by size descending
  const collisionGroups = [...index.byExactHash.entries()]
    .filter(([, ids]) => ids.length > 1)
    .sort((a, b) => b[1].length - a[1].length);

  console.log(`Total collision groups: ${collisionGroups.length}`);
  console.log(`Top ${TOP_N} groups:\n`);

  for (let i = 0; i < Math.min(TOP_N, collisionGroups.length); i++) {
    const [hash, sessionIds] = collisionGroups[i];
    console.log(`${"=".repeat(70)}`);
    console.log(`GROUP ${i + 1}: hash=${hash}  count=${sessionIds.length}`);
    console.log(`${"=".repeat(70)}`);

    // Collect memberKey values for this group
    const memberKeys = new Map<string, number>();
    for (const sid of sessionIds) {
      const fp = index.fingerprints.get(sid);
      const key = fp?.memberKey ?? "(none)";
      memberKeys.set(key, (memberKeys.get(key) ?? 0) + 1);
    }

    const uniqueKeys = [...memberKeys.entries()].sort((a, b) => b[1] - a[1]);
    const disambiguatable =
      uniqueKeys.length > 1 ||
      (uniqueKeys.length === 1 && uniqueKeys[0][0] !== "(none)");

    console.log(
      `\n  memberKey distribution (${uniqueKeys.length} distinct values):`
    );
    for (const [key, count] of uniqueKeys.slice(0, 10)) {
      console.log(`    ${key}: ${count}`);
    }
    if (uniqueKeys.length > 10) {
      console.log(`    ... and ${uniqueKeys.length - 10} more`);
    }
    console.log(
      `  Disambiguatable by memberKey: ${disambiguatable ? "YES (partially)" : "NO"}`
    );

    // Sample functions - try to pick ones with different memberKeys
    const sampled: string[] = [];
    const seenKeys = new Set<string>();

    // First pass: pick diverse memberKeys
    for (const sid of sessionIds) {
      if (sampled.length >= SAMPLES_PER_GROUP) break;
      const fp = index.fingerprints.get(sid);
      const key = fp?.memberKey ?? "(none)";
      if (!seenKeys.has(key)) {
        seenKeys.add(key);
        sampled.push(sid);
      }
    }

    // Second pass: fill remaining slots
    for (const sid of sessionIds) {
      if (sampled.length >= SAMPLES_PER_GROUP) break;
      if (!sampled.includes(sid)) {
        sampled.push(sid);
      }
    }

    for (let j = 0; j < sampled.length; j++) {
      const sid = sampled[j];
      const fn = functions.get(sid);
      const fp = index.fingerprints.get(sid);

      if (!fn) {
        console.log(`\n  [Sample ${j + 1}] ${sid} — function node not found`);
        continue;
      }

      const memberKey = fp?.memberKey ?? "(none)";
      const calleeCount = fn.internalCallees.size;
      const callerCount = fn.callers.size;

      let source: string;
      try {
        source = generate(fn.path.node).code;
      } catch {
        source = "(generate failed)";
      }

      console.log(`\n  [Sample ${j + 1}] sessionId=${sid}`);
      console.log(
        `  memberKey=${memberKey}  callees=${calleeCount}  callers=${callerCount}`
      );
      console.log(`  --------`);
      for (const line of truncateSource(source).split("\n")) {
        console.log(`  ${line}`);
      }
    }

    console.log("");
  }

  // Summary statistics
  console.log(`\n${"=".repeat(70)}`);
  console.log("SUMMARY");
  console.log(`${"=".repeat(70)}`);

  let totalColliding = 0;
  let disambiguatableByMemberKey = 0;
  for (const [, sessionIds] of collisionGroups) {
    totalColliding += sessionIds.length;
    const keys = new Set<string>();
    for (const sid of sessionIds) {
      const fp = index.fingerprints.get(sid);
      keys.add(fp?.memberKey ?? "(none)");
    }
    if (keys.size > 1 || (keys.size === 1 && !keys.has("(none)"))) {
      disambiguatableByMemberKey += sessionIds.length;
    }
  }

  console.log(`Total functions: ${functions.size}`);
  console.log(`Unique hashes: ${index.byExactHash.size}`);
  console.log(
    `Functions in collision groups: ${totalColliding} (${((totalColliding / functions.size) * 100).toFixed(1)}%)`
  );
  console.log(`Collision groups: ${collisionGroups.length}`);
  console.log(
    `Disambiguatable by memberKey: ${disambiguatableByMemberKey} of ${totalColliding} (${((disambiguatableByMemberKey / totalColliding) * 100).toFixed(1)}%)`
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
