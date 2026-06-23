/**
 * End-to-end Phase B unpack smoke test: run the modified BunUnpackAdapter
 * against the real 13MB claude-code 2.1.120 bundle. Report:
 *   - file count + size distribution
 *   - manifest entries by nameSource
 *   - sample filenames per source
 *   - elapsed time
 *
 * Usage:
 *   node --max-old-space-size=8192 --import tsx/esm \
 *     experiments/013-bun-cjs-classification/unpack-real.ts \
 *     <bundle.js> <outputDir>
 */

import { readFileSync, statSync } from "node:fs";
import { readdir, readFile } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import path from "node:path";
import {
  BUN_MODULES_MANIFEST,
  BunUnpackAdapter,
  type BunModulesManifest
} from "../../src/unpack/adapters/bun.js";

const fmt = (n: number) => n.toLocaleString();

const [, , bundlePath, outDir] = process.argv;
if (!bundlePath || !outDir) {
  console.error("usage: unpack-real.ts <bundle.js> <outputDir>");
  process.exit(1);
}

const t0 = performance.now();
const code = readFileSync(bundlePath, "utf-8");
const adapter = new BunUnpackAdapter();
const result = await adapter.unpack(code, outDir);
const elapsed = performance.now() - t0;

console.log(
  `Unpacked ${fmt(result.files.length)} files in ${(elapsed / 1000).toFixed(1)}s`
);

const manifestRaw = await readFile(
  path.join(outDir, BUN_MODULES_MANIFEST),
  "utf-8"
);
const manifest = JSON.parse(manifestRaw) as BunModulesManifest;

console.log(`\nManifest: ${fmt(manifest.factories.length)} factories`);

const bySource: Record<string, number> = {};
for (const f of manifest.factories) {
  bySource[f.nameSource] = (bySource[f.nameSource] ?? 0) + 1;
}
console.log("\nBy nameSource:");
for (const [src, count] of Object.entries(bySource)) {
  console.log(`  ${src}: ${fmt(count)}`);
}

const samples: Record<string, string[]> = {};
for (const f of manifest.factories) {
  const arr = samples[f.nameSource] ?? [];
  if (arr.length < 5) arr.push(f.fileName);
  samples[f.nameSource] = arr;
}
console.log("\nSamples:");
for (const [src, names] of Object.entries(samples)) {
  console.log(`  ${src}: ${names.join(", ")}`);
}

// Spot-check the runtime
if (manifest.runtimeFile) {
  const stat = statSync(path.join(outDir, manifest.runtimeFile));
  console.log(`\nRuntime: ${manifest.runtimeFile} (${fmt(stat.size)} bytes)`);
}

// Spot-check biggest factory
const sorted = [...manifest.factories]
  .map((f) => ({ ...f, size: statSync(path.join(outDir, f.fileName)).size }))
  .sort((a, b) => b.size - a.size);
console.log(`\nLargest factories:`);
for (const f of sorted.slice(0, 5)) {
  console.log(
    `  ${fmt(f.size).padStart(10)}  ${f.fileName} (source: ${f.nameSource})`
  );
}

const files = await readdir(outDir);
console.log(`\nTotal files in outDir: ${fmt(files.length)}`);
