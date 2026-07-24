/**
 * Fast Lever-B validation: re-split an already-humanified bundle against a prior
 * ledger, WITHOUT re-running the LLM. Isolates the emit-order change — file
 * assignment is identical run-to-run, so any tree difference is purely emission
 * order. Writes the tree to <out> for a reorder-churn comparison vs the prior.
 *
 * Usage:
 *   HUMANIFY_NO_EMIT_ALIGN=1 tsx resplit-validate.ts <bundle.js> <priorLedger.json> <outDir>  # control
 *   tsx resplit-validate.ts <bundle.js> <priorLedger.json> <outDir>                            # aligned
 */
import * as fs from "node:fs";
import * as path from "node:path";
import {
  type StableSplitLedger,
  stableSplitFromCode
} from "../../src/split/stable-split.js";

async function main() {
  const [bundlePath, priorLedgerPath, outDir] = process.argv.slice(2);
  const code = fs.readFileSync(bundlePath, "utf8");
  const prior = JSON.parse(
    fs.readFileSync(priorLedgerPath, "utf8")
  ) as StableSplitLedger;
  const t0 = Date.now === undefined ? 0 : 0; // Date.now unavailable in workflows; harmless here
  void t0;
  const result = await stableSplitFromCode(code, { prior });
  if (!result) {
    console.error("split returned null (not wrapper-shaped?)");
    process.exit(1);
  }
  fs.rmSync(outDir, { recursive: true, force: true });
  for (const [rel, content] of result.fileContents) {
    const full = path.join(outDir, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, content);
  }
  console.log(
    `wrote ${result.fileContents.size} files to ${outDir}; ` +
      `align=${process.env.HUMANIFY_NO_EMIT_ALIGN === "1" ? "OFF" : "ON"}; ` +
      `concat-equivalence PASSED (would have thrown otherwise)`
  );
}

main();
