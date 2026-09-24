/**
 * The webcrack subprocess shim (WPB.2): the Rust unpack stage's webcrack
 * adapter runs webpack/browserify bundles through THIS script rather than
 * porting webcrack (docs/rust-port/10-work-breakdown.md, WPB.2).
 *
 *   <code on stdin> | npx tsx scripts/webcrack-shim.ts <output-dir>
 *
 * Runs the pipeline's own `webcrack()` (src/plugins/webcrack.ts — the file
 * this shim absorbs at the cutover, when src/ goes) and prints one JSON
 * line: `{"files":[{"path","metadata"?}],"bundleType"?}` — the TS
 * `WebcrackOutput`, which `humanify_core::unpack::webcrack` parses.
 */
import { webcrack } from "../src/plugins/webcrack.js";

async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) chunks.push(chunk as Buffer);
  return Buffer.concat(chunks).toString("utf-8");
}

async function main(): Promise<void> {
  const outputDir = process.argv[2];
  if (!outputDir) {
    process.stderr.write("usage: webcrack-shim.ts <output-dir> < bundle.js\n");
    process.exit(2);
  }
  const output = await webcrack(await readStdin(), outputDir);
  process.stdout.write(`${JSON.stringify(output)}\n`);
}

void main();
