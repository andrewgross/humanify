/**
 * Drive the PRODUCTION stable split (src/split/stable-split.ts) offline on
 * an existing humanified output — the same function the pipeline's
 * --split path calls.
 *
 *   npx tsx run-stable-split.ts <humanified.js> <outDir> [--prior <ledger.json>]
 */
import fs from "node:fs";
import path from "node:path";
import { OpenAICompatibleProvider } from "../../src/llm/openai-compatible.js";
import { createSplitNamer } from "../../src/split/split-namer.js";
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
const llmNames = process.argv.includes("--llm-names");

if (!input || !outDir) {
  console.error(
    "usage: run-stable-split.ts <humanified.js> <outDir> [--prior <ledger.json>] [--llm-names]"
  );
  process.exit(1);
}

async function main(): Promise<void> {
  const namer =
    llmNames && !prior
      ? createSplitNamer(
          new OpenAICompatibleProvider({
            endpoint:
              process.env.HUMANIFY_ENDPOINT ?? "http://192.168.1.234:8000/v1",
            apiKey: process.env.HUMANIFY_API_KEY ?? "local",
            model: process.env.HUMANIFY_MODEL ?? "openai/gpt-oss-20b",
            reasoningEffort: (process.env.HUMANIFY_REASONING_EFFORT ??
              "low") as "low" | "medium" | "high"
          })
        )
      : undefined;
  const t0 = Date.now();
  const result = await stableSplitFromCode(fs.readFileSync(input, "utf-8"), {
    prior,
    namer
  });
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
  console.log(`elapsed: ${((Date.now() - t0) / 1000).toFixed(1)}s`);
  console.log(JSON.stringify(result.stats, null, 2));
}
main();
