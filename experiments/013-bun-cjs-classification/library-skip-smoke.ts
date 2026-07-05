/**
 * Phase B step 3 smoke test: run the modified Bun unpack adapter + library
 * detector against the real bundle and report how many files would be
 * skipped from LLM rename.
 *
 * Usage:
 *   node --max-old-space-size=8192 --import tsx/esm \
 *     experiments/013-bun-cjs-classification/library-skip-smoke.ts \
 *     <bundle.js> <outputDir>
 */

import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";
import { BunUnpackAdapter } from "../../src/unpack/adapters/bun.js";
import { BunLibraryDetector } from "../../src/library-detection/adapters/bun.js";

const fmt = (n: number) => n.toLocaleString();

const [, , bundlePath, outDir] = process.argv;
if (!bundlePath || !outDir) {
  console.error("usage: library-skip-smoke.ts <bundle.js> <outputDir>");
  process.exit(1);
}

const t0 = performance.now();
const code = readFileSync(bundlePath, "utf-8");
const adapter = new BunUnpackAdapter();
const { files } = await adapter.unpack(code, outDir);
const tUnpack = performance.now();
console.log(
  `Unpacked: ${fmt(files.length)} files in ${((tUnpack - t0) / 1000).toFixed(1)}s`
);

const detector = new BunLibraryDetector();
const result = await detector.detectLibraries(files);
const tDetect = performance.now();
console.log(`Library detection: ${((tDetect - tUnpack) / 1000).toFixed(2)}s`);

console.log(`\n=== Library detection result ===`);
console.log(`  library files: ${fmt(result.libraryFiles.size)}`);
console.log(`  novel files:   ${fmt(result.novelFiles.length)}`);
console.log(`  mixed files:   ${fmt(result.mixedFiles.size)}`);

if (result.libraryFiles.size > 0) {
  const byName = new Map<string, number>();
  for (const det of result.libraryFiles.values()) {
    const name = det.libraryName ?? "<unknown>";
    byName.set(name, (byName.get(name) ?? 0) + 1);
  }
  const top = [...byName.entries()].sort((a, b) => b[1] - a[1]).slice(0, 10);
  console.log(`\nTop library names by count:`);
  for (const [name, count] of top) {
    console.log(`  ${String(count).padStart(4)}  ${name}`);
  }
}

console.log(`\nNovel files (would go to LLM):`);
for (const p of result.novelFiles) {
  console.log(`  ${p.split("/").pop()}`);
}

const ratio = (result.libraryFiles.size / files.length) * 100;
console.log(
  `\nLibrary skip ratio: ${ratio.toFixed(1)}% of unpacked files skipped from LLM`
);
