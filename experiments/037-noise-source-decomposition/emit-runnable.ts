/**
 * Offline runnable emit from an already-humanified bundle + its ledger. Used to
 * generate the align-OFF (bundle-order) baseline tree from the SAME ledger the
 * align-ON pipeline run produced, so the ON vs OFF comparison differs ONLY in
 * emit order (identical file assignment). Set HUMANIFY_NO_EMIT_ALIGN=1 for the
 * OFF baseline; unset reproduces the aligned tree.
 *
 * Usage: emit-runnable.ts <humanified.js> <ledger.json> <outDir>
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { emitRunnableCjs } from "../../src/split/cjs-emit.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";

function main() {
  const [humPath, ledgerPath, outDir] = process.argv.slice(2);
  const code = fs.readFileSync(humPath, "utf8");
  const ledger = JSON.parse(
    fs.readFileSync(ledgerPath, "utf8")
  ) as StableSplitLedger;
  const tree = emitRunnableCjs(code, ledger);
  fs.rmSync(outDir, { recursive: true, force: true });
  for (const [rel, content] of tree) {
    const full = path.join(outDir, rel);
    fs.mkdirSync(path.dirname(full), { recursive: true });
    fs.writeFileSync(full, content);
  }
  console.log(
    `wrote ${tree.size} files to ${outDir} (align=${process.env.HUMANIFY_NO_EMIT_ALIGN === "1" ? "OFF" : "ON"})`
  );
}

main();
