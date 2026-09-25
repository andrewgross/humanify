/**
 * TEMPORARY ADAPTER — deleted by WP5.6d (docs/rust-port/17-formatter-swap.md).
 *
 *   npx tsx experiments/lib/ts-beautify.ts <input.js> <out.js>
 *
 * Writes the text the TS pipeline hands its naming stage: stages 1-6 (detect,
 * unpack, library filter, Babel beautify) of `src/unminify.ts`, and nothing
 * after. Until 5.6d ports stage 6, the Rust binary stops at stage 6 unless it
 * is handed exactly this text via `--beautified-input`, so
 * `run.sh --bin ... --ts-beautify-adapter` runs this once per pipeline launch.
 *
 * NOT `createBabelPlugin()(input)`: the Bun unpack rewrites the app file
 * before the beautify (vendored factories move out, references become
 * `lib_<hash>()` calls), and a beautify of the raw bundle differs from the
 * oracle's `text/fresh.js` by ~560K lines. Run through the real `unminify`
 * with ONLY the beautify plugin, no vendor namer and no prior, because none
 * of those reach the app file's text (the vendor namer names vendor/ paths;
 * the app refers to factories by hash). Validated byte-equal to the oracle
 * dump's `text/fresh.js` on 2.1.86 (2026-09-25) — re-validate before
 * trusting it on another bundle family.
 *
 * The OTHER TS-only input of the binary, `--inject-ts-hashes`, is NOT
 * producible here: its `partitions.json` statementHash family is computed
 * over the SHIPPED (renamed, split-input) text, i.e. it is a function of the
 * whole TS run. See the eval README's "Rust binary" section.
 */
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { detectBundle } from "../../src/detection/detect.js";
import { buildPipelineConfig } from "../../src/pipeline/config.js";
import { createBabelPlugin } from "../../src/plugins/babel/babel.js";
import { unminify } from "../../src/unminify.js";

async function main(): Promise<void> {
  const [input, out] = process.argv.slice(2);
  if (!input || !out) {
    console.error("usage: ts-beautify.ts <input.js> <out.js>");
    process.exit(2);
  }
  const code = fs.readFileSync(input, "utf8");
  const config = buildPipelineConfig(detectBundle(code), {});
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "ts-beautify-"));
  const babel = createBabelPlugin();
  const formatted: string[] = [];
  try {
    await unminify(
      code,
      scratch,
      config,
      [
        async (c, ctx) => {
          const text = await babel(c, ctx);
          formatted.push(text);
          return text;
        }
      ],
      { skipLibraries: true, skipFileWrite: true, log: () => {} }
    );
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
  // The binary takes ONE file's text and refuses a multi-file unpack, so
  // refuse here too rather than hand it the wrong file.
  if (formatted.length !== 1) {
    console.error(
      `ts-beautify: ${formatted.length} files reached the beautify; the binary's --beautified-input holds exactly one`
    );
    process.exit(1);
  }
  fs.writeFileSync(out, formatted[0]);
}

main().catch((e) => {
  console.error(`ts-beautify: ${e}`);
  process.exit(1);
});
