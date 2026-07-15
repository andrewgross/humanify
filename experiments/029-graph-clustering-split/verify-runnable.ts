/**
 * Does the fine clustered tree survive the runnable CJS emit, or does it
 * have load-time reference cycles (which assertLoadTimeAcyclic throws on)?
 * This decides whether "runnable-only" needs the merge gate and thus whether
 * it changes the layout.
 *
 *   NODE_OPTIONS=--max-old-space-size=8192 tsx verify-runnable.ts 2.1.89
 */

import { emitRunnableCjs } from "../../src/split/cjs-emit.js";
import { stableSplitFromCode } from "../../src/split/stable-split.js";
import { loadBeautified } from "./lib/io.js";

async function main(): Promise<void> {
  const version = process.argv[2] ?? "2.1.89";
  const code = await loadBeautified(version);
  const stable = await stableSplitFromCode(code); // clustered, mechanical names
  if (!stable) throw new Error("not a wrapper");
  console.log(
    `clustered tree: ${stable.stats.files} files, ${stable.stats.folders} folders`
  );
  try {
    const start = Date.now();
    const tree = emitRunnableCjs(code, stable.ledger, stable.wrapper);
    console.log(
      `runnable emit: OK — load-time ACYCLIC. ${tree.size} files in ${Date.now() - start}ms`
    );
    const bundleFile = [...tree.keys()].find((f) => f.includes("_bundle"));
    console.log(`_bundle runtime: ${bundleFile ?? "(context unused)"}`);
    const entry = tree.get("index.js") ?? "";
    const requires = entry.split("\n").filter((l) => l.startsWith("require("));
    console.log(
      `entry: index.js at root, ${requires.length} requires; first 3:`
    );
    for (const line of requires.slice(0, 3)) console.log(`  ${line}`);
    console.log(
      "=> runnable-only needs NO merges; layout is unchanged (content gains require/exports)."
    );
  } catch (err) {
    console.log(
      `runnable emit: THREW — ${err instanceof Error ? err.message : String(err)}`
    );
    console.log(
      "=> load-time cycle(s) exist; runnable-only would need the merge gate (merges change layout)."
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
