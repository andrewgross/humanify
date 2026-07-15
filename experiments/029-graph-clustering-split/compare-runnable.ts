/** Review-tree file vs its runnable-CJS counterpart — same path, different
 * content. Shows what "runnable-only" does to a file.
 *   NODE_OPTIONS=--max-old-space-size=8192 tsx compare-runnable.ts 2.1.89 */

import { emitRunnableCjs } from "../../src/split/cjs-emit.js";
import { stableSplitFromCode } from "../../src/split/stable-split.js";
import { loadBeautified } from "./lib/io.js";

async function main(): Promise<void> {
  const code = await loadBeautified(process.argv[2] ?? "2.1.89");
  const stable = await stableSplitFromCode(code);
  if (!stable) throw new Error("not a wrapper");
  const runnable = emitRunnableCjs(code, stable.ledger, stable.wrapper);

  const pick = [...stable.fileContents.keys()].find((f) => {
    const c = stable.fileContents.get(f) ?? "";
    return !f.startsWith("libraries/") && c.length > 300 && c.length < 900;
  });
  if (!pick) throw new Error("no sample file");

  const review = stable.fileContents.get(pick) ?? "";
  const run = runnable.get(pick) ?? "";
  console.log(`file: ${pick}\n`);
  console.log(`REVIEW (${review.split("\n").length} lines) — head:`);
  console.log(review.split("\n").slice(0, 10).join("\n"));
  console.log(`\nRUNNABLE (${run.split("\n").length} lines) — head + tail:`);
  console.log(run.split("\n").slice(0, 12).join("\n"));
  console.log("  …");
  console.log(run.split("\n").slice(-6).join("\n"));

  const extraRoot = [...runnable.keys()].filter(
    (f) => !stable.fileContents.has(f)
  );
  console.log(
    `\nfiles added at root by runnable emit: ${extraRoot.join(", ")}`
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
