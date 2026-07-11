/**
 * Drive the PRODUCTION stable split (src/split/stable-split.ts) offline on
 * an existing humanified output — the same function the pipeline's
 * --split path calls.
 *
 *   npx tsx run-stable-split.ts <humanified.js> <outDir> [--prior <ledger.json>]
 */
import fs from "node:fs";
import path from "node:path";
import {
  SPLIT_LEDGER_FILENAME,
  stableSplitFromCode
} from "../../src/split/stable-split.js";

const [input, outDir] = process.argv.slice(2);
const priorIdx = process.argv.indexOf("--prior");
const prior =
  priorIdx === -1
    ? undefined
    : JSON.parse(fs.readFileSync(process.argv[priorIdx + 1], "utf-8"));

if (!input || !outDir) {
  console.error(
    "usage: run-stable-split.ts <humanified.js> <outDir> [--prior <ledger.json>]"
  );
  process.exit(1);
}

const result = stableSplitFromCode(fs.readFileSync(input, "utf-8"), { prior });
if (!result) {
  console.error("not a wrapper bundle — stable split does not apply");
  process.exit(2);
}
fs.mkdirSync(outDir, { recursive: true });
for (const [file, content] of result.fileContents) {
  const filePath = path.join(outDir, file);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content);
}
fs.writeFileSync(
  path.join(outDir, SPLIT_LEDGER_FILENAME),
  JSON.stringify(result.ledger)
);
console.log(JSON.stringify(result.stats, null, 2));
